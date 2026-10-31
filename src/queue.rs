//! SLURM queue lookup: is a job still pending or running?
//!
//! `squeue` is called **once** per program run; the output is parsed by a
//! pure function so the matching logic can be tested without a cluster.
//!
//! Matching rules (see [`Queue::assign`]):
//! * A SLURM job belongs to the file stem `base` if its name is exactly
//!   `base` or starts with `base.` (`mol_1` does **not** match `mol_10.sp.sh`).
//!   If several stems of the same directory match (`x.sp` and `x.sp.pbe0`),
//!   the longest one wins.
//! * Running **and** pending jobs must work in the analysed directory, so a
//!   job with the same name in another folder never changes the status.
//!
//! If `squeue` cannot be queried, the caller gets warnings and every job is
//! reported as [`QueueState::Unavailable`] instead of silently "not in queue".

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::analysis::QueueState;

/// Format string passed to `squeue -o`: job id, job name, compact state, working directory.
pub const SQUEUE_FORMAT: &str = "%i|%j|%t|%Z";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueJob {
    pub id: String,
    pub name: String,
    /// SLURM compact state code: `R`, `PD`, `CG`, …
    pub state: String,
    pub workdir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueueSource {
    /// `squeue` was queried successfully.
    Loaded,
    /// The user disabled the lookup (`--no-queue`).
    #[default]
    Disabled,
    /// `squeue` could not be queried; the queue state is unknown.
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Queue {
    pub jobs: Vec<QueueJob>,
    pub source: QueueSource,
}

/// Result of querying SLURM: the queue plus human-readable warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueLoad {
    pub queue: Queue,
    pub warnings: Vec<String>,
}

/// Job states in which the job holds (or is about to hold) nodes.
const RUNNING_STATES: &[&str] = &["R", "CG", "CF", "S", "ST", "SO", "RS"];
/// Job states in which the job waits.
const PENDING_STATES: &[&str] = &["PD", "RQ", "RH", "RF", "RD"];

impl Queue {
    /// Queue for `--no-queue`: every job is reported as [`QueueState::Unknown`].
    pub fn disabled() -> Queue {
        Queue { jobs: Vec::new(), source: QueueSource::Disabled }
    }

    fn failed(warning: String) -> QueueLoad {
        QueueLoad { queue: Queue { jobs: Vec::new(), source: QueueSource::Failed }, warnings: vec![warning] }
    }

    /// Query SLURM for the current user's jobs.
    pub fn from_squeue() -> QueueLoad {
        let user = match std::env::var("USER").or_else(|_| std::env::var("LOGNAME")) {
            Ok(u) if !u.trim().is_empty() => u,
            _ => return Queue::failed("neither USER nor LOGNAME is set, cannot query squeue".to_string()),
        };
        let output = Command::new("squeue").args(["-u", &user, "-h", "-o", SQUEUE_FORMAT]).output();
        match output {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Queue::failed(
                "`squeue` not found (no SLURM on this machine?); RUNNING/PENDING jobs cannot be detected. \
                 Use --no-queue to skip the lookup."
                    .to_string(),
            ),
            Err(e) => Queue::failed(format!("could not run `squeue`: {e}")),
            Ok(o) if !o.status.success() => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                let stderr = stderr.trim();
                let detail = if stderr.is_empty() { String::new() } else { format!(": {stderr}") };
                Queue::failed(format!("`squeue` failed ({}){detail}", o.status))
            }
            Ok(o) => Queue::parse(&String::from_utf8_lossy(&o.stdout)),
        }
    }

    /// Parse `squeue -h -o "%i|%j|%t|%Z"` output.  Malformed lines are
    /// skipped and reported as warnings.
    pub fn parse(text: &str) -> QueueLoad {
        let mut jobs = Vec::new();
        let mut malformed: Vec<&str> = Vec::new();
        let mut warnings = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(4, '|').map(str::trim).collect();
            if parts.len() < 4 || parts[0].is_empty() || parts[1].is_empty() || parts[2].is_empty() {
                malformed.push(line);
                continue;
            }
            if parts[3].is_empty() {
                warnings.push(format!(
                    "squeue job {} (`{}`) has no working directory, matched by name only",
                    parts[0], parts[1]
                ));
            }
            jobs.push(QueueJob {
                id: parts[0].to_string(),
                name: parts[1].to_string(),
                state: parts[2].to_uppercase(),
                workdir: parts[3].to_string(),
            });
        }
        if !malformed.is_empty() {
            warnings.push(format!(
                "ignored {} squeue line(s) not in the format `{SQUEUE_FORMAT}`, e.g. `{}`",
                malformed.len(),
                malformed[0]
            ));
        }
        QueueLoad { queue: Queue { jobs, source: QueueSource::Loaded }, warnings }
    }

    /// Queue state of every stem in `dir`.  Stems without a matching job are
    /// missing from the map (= [`QueueState::Unknown`]), unless the queue
    /// could not be loaded, then every stem is [`QueueState::Unavailable`].
    pub fn assign(&self, dir: &Path, stems: &[String]) -> HashMap<String, QueueState> {
        let mut map = HashMap::new();
        match self.source {
            QueueSource::Disabled => return map,
            QueueSource::Failed => {
                for s in stems {
                    map.insert(s.clone(), QueueState::Unavailable);
                }
                return map;
            }
            QueueSource::Loaded => {}
        }
        let dir_canon = dir.canonicalize().ok();
        let dir_abs = std::path::absolute(dir).ok();
        for job in &self.jobs {
            let Some(stem) = owning_stem(&job.name, stems) else { continue };
            if !same_directory(&job.workdir, dir_canon.as_deref(), dir_abs.as_deref()) {
                continue;
            }
            let state = if RUNNING_STATES.contains(&job.state.as_str()) {
                QueueState::Running { job_id: job.id.clone(), workdir: job.workdir.clone() }
            } else if PENDING_STATES.contains(&job.state.as_str()) {
                QueueState::Pending
            } else {
                continue;
            };
            // Running beats pending (e.g. a running job and a requeued copy).
            let keep_existing = matches!(map.get(stem), Some(QueueState::Running { .. }));
            if !keep_existing {
                map.insert(stem.to_string(), state);
            }
        }
        map
    }

    /// Convenience wrapper for a single stem.
    pub fn lookup(&self, base: &str, dir: &Path) -> QueueState {
        self.assign(dir, &[base.to_string()]).remove(base).unwrap_or_default()
    }
}

/// Does the SLURM job name belong to `base`?  Exact match or `base.` prefix.
pub fn name_matches(job_name: &str, base: &str) -> bool {
    job_name == base || (job_name.len() > base.len() && job_name.starts_with(base) && job_name[base.len()..].starts_with('.'))
}

/// The longest stem the job name belongs to.
fn owning_stem<'a>(job_name: &str, stems: &'a [String]) -> Option<&'a str> {
    stems
        .iter()
        .filter(|s| name_matches(job_name, s))
        .max_by_key(|s| s.len())
        .map(String::as_str)
}

/// Is the job's working directory the analysed directory?  Paths are
/// compared after resolving symlinks; if the job's directory cannot be
/// resolved (e.g. not mounted here), the absolute paths are compared.
/// An empty working directory cannot be checked and counts as a match.
fn same_directory(workdir: &str, dir_canon: Option<&Path>, dir_abs: Option<&Path>) -> bool {
    if workdir.is_empty() {
        return true;
    }
    let job_dir = PathBuf::from(workdir);
    match (job_dir.canonicalize().ok(), dir_canon) {
        (Some(j), Some(d)) => j == d,
        _ => dir_abs.is_some_and(|d| d == job_dir.as_path()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stems(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    fn here() -> PathBuf {
        std::env::temp_dir().canonicalize().unwrap()
    }

    #[test]
    fn parse_squeue_lines_and_report_malformed() {
        let load = Queue::parse("123|foo.sp.sh|R|/scratch/a\n124|bar.opt.sh|pd|\n\n garbage\n");
        assert_eq!(load.queue.source, QueueSource::Loaded);
        assert_eq!(load.queue.jobs.len(), 2);
        assert_eq!(load.queue.jobs[0].id, "123");
        assert_eq!(load.queue.jobs[0].workdir, "/scratch/a");
        assert_eq!(load.queue.jobs[1].state, "PD");
        assert_eq!(load.warnings.len(), 2, "{:?}", load.warnings);
        assert!(load.warnings.iter().any(|w| w.contains("bar.opt.sh") && w.contains("no working directory")));
        assert!(load.warnings.iter().any(|w| w.contains("ignored 1") && w.contains("garbage")));
    }

    #[test]
    fn workdir_may_contain_separator() {
        let load = Queue::parse("1|a.sh|R|/weird|dir\n");
        assert_eq!(load.queue.jobs[0].workdir, "/weird|dir");
    }

    #[test]
    fn name_rule_is_exact_or_dot_prefix() {
        assert!(name_matches("mol_1", "mol_1"));
        assert!(name_matches("mol_1.sp.sh", "mol_1"));
        assert!(!name_matches("mol_10.sp.sh", "mol_1"));
        assert!(!name_matches("mol_10", "mol_1"));
        assert!(!name_matches("xmol_1.sh", "mol_1"));
        assert!(!name_matches("mol", "mol_1"));
    }

    #[test]
    fn mol_1_is_not_running_when_mol_10_runs() {
        let dir = here();
        let q = Queue::parse(&format!("7|mol_10.sp.sh|R|{}\n", dir.display())).queue;
        let map = q.assign(&dir, &stems(&["mol_1", "mol_10"]));
        assert_eq!(map.get("mol_1"), None);
        assert!(matches!(map.get("mol_10"), Some(QueueState::Running { job_id, .. }) if job_id == "7"));
    }

    #[test]
    fn longest_stem_wins_for_dotted_stems() {
        let dir = here();
        let q = Queue::parse(&format!("8|x.sp.pbe0.sh|PD|{}\n", dir.display())).queue;
        let map = q.assign(&dir, &stems(&["x.sp", "x.sp.pbe0"]));
        assert_eq!(map.get("x.sp"), None);
        assert_eq!(map.get("x.sp.pbe0"), Some(&QueueState::Pending));
    }

    #[test]
    fn running_requires_same_directory() {
        let dir = here();
        let q = Queue::parse(&format!("1|foo.sh|R|{}\n2|foo.sh|R|/definitely/not/here\n", dir.display())).queue;
        assert_eq!(q.lookup("foo", &dir), QueueState::Running { job_id: "1".into(), workdir: dir.display().to_string() });
        let q = Queue::parse("2|foo.sh|R|/definitely/not/here\n").queue;
        assert_eq!(q.lookup("foo", &dir), QueueState::Unknown);
    }

    #[test]
    fn pending_requires_same_directory() {
        let dir = here();
        // finished in this directory, pending with the same name somewhere else
        let q = Queue::parse("5|mol_1.sp.sh|PD|/somewhere/2_sp\n").queue;
        assert_eq!(q.lookup("mol_1", &dir), QueueState::Unknown);
        let q = Queue::parse(&format!("5|mol_1.sp.sh|PD|{}\n", dir.display())).queue;
        assert_eq!(q.lookup("mol_1", &dir), QueueState::Pending);
    }

    #[test]
    fn unresolvable_directory_compared_as_absolute_path() {
        assert!(same_directory("/not/mounted/here", None, Some(Path::new("/not/mounted/here"))));
        assert!(same_directory("/not/mounted/here/", None, Some(Path::new("/not/mounted/here"))));
        assert!(!same_directory("/not/mounted/there", None, Some(Path::new("/not/mounted/here"))));
        assert!(same_directory("", None, None));
    }

    #[test]
    fn running_beats_pending() {
        let dir = here();
        let d = dir.display();
        let q = Queue::parse(&format!("1|a.sh|PD|{d}\n2|a.sh|R|{d}\n3|a.sh|PD|{d}\n")).queue;
        assert!(matches!(q.lookup("a", &dir), QueueState::Running { job_id, .. } if job_id == "2"));
    }

    #[test]
    fn finished_states_are_ignored() {
        let dir = here();
        let q = Queue::parse(&format!("1|a.sh|CD|{}\n", dir.display())).queue;
        assert_eq!(q.lookup("a", &dir), QueueState::Unknown);
    }

    #[test]
    fn failed_and_disabled_queues() {
        let failed = Queue { jobs: Vec::new(), source: QueueSource::Failed };
        assert_eq!(failed.lookup("x", Path::new(".")), QueueState::Unavailable);
        assert_eq!(Queue::disabled().lookup("x", Path::new(".")), QueueState::Unknown);
        assert_eq!(Queue::parse("").queue.lookup("x", Path::new(".")), QueueState::Unknown);
    }
}

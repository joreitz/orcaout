//! SLURM queue lookup: is a job still pending or running?
//!
//! `squeue` is called **once** per program run; the output is parsed by a
//! pure function so the matching logic can be tested without a cluster.
//! Any failure (no SLURM, no `USER`, squeue error) yields an empty queue —
//! never an error.

use std::path::Path;
use std::process::Command;

use crate::analysis::QueueState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueJob {
    pub id: String,
    pub name: String,
    /// SLURM state code: `R`, `PD`, `CG`, …
    pub state: String,
    pub workdir: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Queue {
    pub jobs: Vec<QueueJob>,
}

/// Format string passed to `squeue -o`.
pub const SQUEUE_FORMAT: &str = "%i|%j|%t|%Z";

impl Queue {
    /// Query SLURM for the current user's jobs.  Returns an empty queue when
    /// `squeue` is not available.
    pub fn from_squeue() -> Queue {
        let Ok(user) = std::env::var("USER") else {
            return Queue::default();
        };
        let output = Command::new("squeue").args(["-u", &user, "-h", "-o", SQUEUE_FORMAT]).output();
        match output {
            Ok(o) if o.status.success() => Queue::parse(&String::from_utf8_lossy(&o.stdout)),
            _ => Queue::default(),
        }
    }

    /// Parse `squeue -h -o "%i|%j|%t|%Z"` output.
    pub fn parse(text: &str) -> Queue {
        let jobs = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.trim().splitn(4, '|');
                let id = parts.next()?.trim();
                let name = parts.next()?.trim();
                let state = parts.next()?.trim();
                let workdir = parts.next().unwrap_or("").trim();
                if id.is_empty() {
                    return None;
                }
                Some(QueueJob { id: id.to_string(), name: name.to_string(), state: state.to_uppercase(), workdir: workdir.to_string() })
            })
            .collect();
        Queue { jobs }
    }

    /// Look up the queue state of the job with file stem `base` in `dir`.
    ///
    /// A job matches when its name contains `base`.  A running job
    /// additionally has to work in `dir` (same directory after
    /// canonicalisation) so that identically named jobs in other folders
    /// are not confused.
    pub fn lookup(&self, base: &str, dir: &Path) -> QueueState {
        let canonical_dir = dir.canonicalize().ok();
        let mut pending = false;
        for job in self.jobs.iter().filter(|j| j.name.contains(base)) {
            match job.state.as_str() {
                "R" | "CG" | "S" => {
                    let same_dir = match (&canonical_dir, Path::new(&job.workdir).canonicalize().ok()) {
                        (Some(a), Some(b)) => a == &b,
                        _ => job.workdir.is_empty(), // no workdir info: trust the name
                    };
                    if same_dir {
                        return QueueState::Running { job_id: job.id.clone(), workdir: job.workdir.clone() };
                    }
                }
                "PD" | "CF" | "RQ" | "RH" => pending = true,
                _ => {}
            }
        }
        if pending { QueueState::Pending } else { QueueState::Unknown }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_squeue_lines() {
        let q = Queue::parse("123|foo.sp.sh|R|/scratch/a\n124|bar.opt.sh|PD|\n\n garbage\n");
        assert_eq!(q.jobs.len(), 2, "malformed lines are dropped");
        assert_eq!(q.jobs[0].id, "123");
        assert_eq!(q.jobs[0].workdir, "/scratch/a");
        assert_eq!(q.jobs[1].state, "PD");
        assert_eq!(q.jobs[1].workdir, "");
    }

    #[test]
    fn lookup_running_requires_same_directory() {
        let tmp = std::env::temp_dir();
        let here = tmp.canonicalize().unwrap();
        let q = Queue::parse(&format!("1|foo.sh|R|{}\n2|foo.sh|R|/definitely/not/here\n", here.display()));
        assert_eq!(q.lookup("foo", &tmp), QueueState::Running { job_id: "1".into(), workdir: here.display().to_string() });
        assert_eq!(q.lookup("other", &tmp), QueueState::Unknown);
        let q = Queue::parse("2|foo.sh|R|/definitely/not/here\n");
        assert_eq!(q.lookup("foo", &tmp), QueueState::Unknown);
    }

    #[test]
    fn lookup_pending_ignores_directory() {
        let q = Queue::parse("5|cola3_n2_4.sp.wb97x-d4rev.sh|PD|/anywhere\n");
        assert_eq!(q.lookup("cola3_n2_4.sp.wb97x-d4rev", Path::new("/tmp")), QueueState::Pending);
        assert_eq!(q.lookup("cola3_n2_5", Path::new("/tmp")), QueueState::Unknown);
    }

    #[test]
    fn empty_queue() {
        assert_eq!(Queue::parse("").lookup("x", Path::new(".")), QueueState::Unknown);
    }
}

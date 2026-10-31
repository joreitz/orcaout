//! Orchestration: find jobs on disk, analyse them in parallel, print the
//! summary and write `outorca.csv` / `outorca.err` (+ `*.best.xyz`).

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use rayon::prelude::*;

use crate::analysis::{AnalysisOptions, JobReport, Observation, QueueState, Status, evaluate};
use crate::cli::Args;
use crate::columns::{Table, build_table};
use crate::input::{InputInfo, JobType, info_from_input, parse_input_text};
use crate::output::{OutputData, parse_output_file};
use crate::queue::Queue;

/// Outputs with fewer lines than this are treated as empty.
const MIN_LINES: u64 = 5;

/// One directory to analyse, optionally restricted to a set of file stems.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub dir: PathBuf,
    /// `None` = every job in the directory.
    pub stems: Option<Vec<String>>,
}

/// Result of analysing one directory.
#[derive(Debug)]
pub struct DirResult {
    pub dir: PathBuf,
    pub reports: Vec<JobReport>,
    pub written: Vec<PathBuf>,
    /// Non-fatal problems while writing files.
    pub io_errors: Vec<String>,
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

fn stem_of(path: &Path) -> Option<String> {
    path.file_stem().and_then(|s| s.to_str()).map(str::to_string)
}

/// Does this directory contain any `.inp` or `.out` file?
fn has_orca_files(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().any(|e| {
                let p = e.path();
                p.is_file() && (has_extension(&p, "inp") || has_extension(&p, "out"))
            })
        })
        .unwrap_or(false)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, errors: &mut Vec<String>) {
    if has_orca_files(dir) {
        out.push(dir.to_path_buf());
    }
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            errors.push(format!("cannot read directory {}: {e}", dir.display()));
            return;
        }
    };
    let mut subdirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.')))
        .collect();
    subdirs.sort();
    for sub in subdirs {
        walk(&sub, out, errors);
    }
}

/// Turn the command line paths into a list of directories (+ stem filters).
/// Problems with individual paths are returned as messages, not errors.
pub fn collect_targets(args: &Args) -> (Vec<Target>, Vec<String>) {
    let mut problems = Vec::new();
    let mut by_dir: BTreeMap<PathBuf, Option<Vec<String>>> = BTreeMap::new();
    let paths: Vec<PathBuf> = if args.paths.is_empty() { vec![PathBuf::from(".")] } else { args.paths.clone() };

    for path in paths {
        if path.is_dir() {
            if args.recursive {
                let mut dirs = Vec::new();
                walk(&path, &mut dirs, &mut problems);
                for d in dirs {
                    by_dir.insert(d, None);
                }
            } else {
                by_dir.insert(path, None);
            }
        } else if path.is_file() {
            if !(has_extension(&path, "inp") || has_extension(&path, "out")) {
                problems.push(format!("{}: not an .inp or .out file, skipped", path.display()));
                continue;
            }
            let Some(stem) = stem_of(&path) else {
                problems.push(format!("{}: cannot determine file name", path.display()));
                continue;
            };
            let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
            match by_dir.entry(dir).or_insert_with(|| Some(Vec::new())) {
                Some(stems) if !stems.contains(&stem) => stems.push(stem),
                _ => {}
            }
        } else {
            problems.push(format!("{}: no such file or directory", path.display()));
        }
    }
    let targets = by_dir.into_iter().map(|(dir, stems)| Target { dir, stems }).collect();
    (targets, problems)
}

/// Stems of the jobs in a directory: every `.inp`; if there are none, every `.out`.
pub fn discover_stems(dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut inps = Vec::new();
    let mut outs = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("cannot read directory {}", dir.display()))? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        if has_extension(&path, "inp") {
            inps.extend(stem_of(&path));
        } else if has_extension(&path, "out") {
            outs.extend(stem_of(&path));
        }
    }
    let mut stems = if inps.is_empty() { outs } else { inps };
    stems.sort();
    stems.dedup();
    Ok(stems)
}

/// Look at `<dir>/<stem>.out`.
pub fn observe_output(path: &Path) -> Observation {
    match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Observation::Missing,
        Err(e) => return Observation::Unreadable(e.to_string()),
        Ok(meta) if meta.len() == 0 => return Observation::Empty,
        Ok(_) => {}
    }
    match parse_output_file(path) {
        Ok(data) if data.lines_read < MIN_LINES && data.final_energies.is_empty() => Observation::Empty,
        Ok(data) => Observation::Parsed(Box::new(data)),
        Err(e) => Observation::Unreadable(format!("{e:#}")),
    }
}

/// Read the input: from `<stem>.inp`, or from the input echoed in the output.
fn read_input(inp_path: &Path, observation: &Observation) -> (InputInfo, Vec<String>) {
    let mut notes = Vec::new();
    match info_from_input(inp_path) {
        Ok(info) => return (info, notes),
        Err(e) if inp_path.exists() => {
            notes.push(format!("could not read {}: {e:#}", inp_path.display()));
        }
        Err(_) => {}
    }
    if let Observation::Parsed(out) = observation
        && let Some(text) = &out.embedded_input
    {
        notes.push("no .inp file, using the input echoed in the .out".to_string());
        return (parse_input_text(text), notes);
    }
    notes.push("no .inp file and no input echoed in the .out".to_string());
    (InputInfo::default(), notes)
}

/// Analyse one job (`<dir>/<stem>.{inp,out}`).
pub fn process_job(dir: &Path, stem: &str, opts: &AnalysisOptions, queue_state: QueueState) -> JobReport {
    let inp_path = dir.join(format!("{stem}.inp"));
    let out_path = dir.join(format!("{stem}.out"));
    let observation = observe_output(&out_path);
    let (input, notes) = read_input(&inp_path, &observation);
    let mut report = evaluate(stem, input, observation, queue_state, opts);
    if !notes.is_empty() {
        // informational, shown after the summary line
        let insert_at = usize::from(!report.messages.is_empty() && report.status <= Status::Warning);
        for (i, n) in notes.into_iter().enumerate() {
            report.messages.insert(insert_at + i, n);
        }
    }
    report
}

/// Analyse all jobs of a target in parallel.
pub fn analyze_target(target: &Target, args: &Args, queue: &Queue) -> anyhow::Result<DirResult> {
    let stems = match &target.stems {
        Some(s) => s.clone(),
        None => discover_stems(&target.dir)?,
    };
    // Queue assignment uses every stem of the directory, so that the longest
    // matching stem wins even when only some files were requested.
    let all_stems = discover_stems(&target.dir).unwrap_or_else(|_| stems.clone());
    let queue_states = queue.assign(&target.dir, &all_stems);
    let mut reports: Vec<JobReport> = stems
        .par_iter()
        .map(|stem| {
            let state = queue_states.get(stem).cloned().unwrap_or_default();
            process_job(&target.dir, stem, &args.analysis, state)
        })
        .collect();
    reports.sort_by(|a, b| a.name.cmp(&b.name));

    let mut result = DirResult { dir: target.dir.clone(), reports, written: Vec::new(), io_errors: Vec::new() };
    if args.write_xyz {
        write_xyz_files(&mut result);
    }
    write_csv_files(&mut result, &args.csv_name);
    write_err_file(&mut result, &args.csv_name, args.show_workdir);
    Ok(result)
}

/// `<stem>.best.xyz` for converged optimisations (like the Python original).
fn write_xyz_files(result: &mut DirResult) {
    for r in &result.reports {
        if !(r.job_type == JobType::Opt || r.job_type == JobType::Optfreq) || r.status > Status::Warning {
            continue;
        }
        let Some(out) = &r.output else { continue };
        if !out.geom_converged || out.final_geometry.is_empty() {
            continue;
        }
        let path = result.dir.join(format!("{}.best.xyz", r.name));
        match fs::write(&path, xyz_text(r, out)) {
            Ok(()) => result.written.push(path),
            Err(e) => result.io_errors.push(format!("could not write {}: {e}", path.display())),
        }
    }
}

pub fn xyz_text(r: &JobReport, out: &OutputData) -> String {
    let mut s = format!("{}\n", out.final_geometry.len());
    s.push_str(&format!(
        "{}   {}, C: {}, M: {}, <S^2>: {}\n",
        r.energy.map(|e| e.to_string()).unwrap_or_default(),
        r.input.method.as_deref().unwrap_or("?"),
        r.charge.map(|c| c.to_string()).unwrap_or_default(),
        r.multiplicity.map(|m| m.to_string()).unwrap_or_default(),
        r.spin.map(|sp| format!("{:.3}", sp.s2)).unwrap_or_default(),
    ));
    for a in &out.final_geometry {
        s.push_str(&format!("{:<4}{:>14.6}{:>14.6}{:>14.6}\n", a.symbol, a.x, a.y, a.z));
    }
    s
}

/// Group the CSV rows per job type; one file per job type when mixed.
pub fn tables_for(reports: &[JobReport]) -> Vec<(JobType, Table)> {
    let mut by_type: BTreeMap<JobType, Vec<&JobReport>> = BTreeMap::new();
    for r in reports {
        by_type.entry(r.job_type).or_default();
        if r.in_csv {
            by_type.get_mut(&r.job_type).expect("inserted above").push(r);
        }
    }
    by_type.into_iter().map(|(jt, rows)| (jt, build_table(jt, &rows))).collect()
}

/// File name of the CSV for a job type: `outorca.csv` if it is the only job
/// type in the directory, `outorca_<job>.csv` otherwise.
pub fn csv_file_name(base: &str, job: JobType, mixed: bool) -> String {
    if !mixed {
        return base.to_string();
    }
    match base.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}_{job}.{ext}"),
        None => format!("{base}_{job}"),
    }
}

fn write_csv_files(result: &mut DirResult, csv_name: &str) {
    let tables = tables_for(&result.reports);
    if tables.is_empty() {
        return;
    }
    let mixed = tables.len() > 1;
    for (jt, table) in tables {
        let path = result.dir.join(csv_file_name(csv_name, jt, mixed));
        match fs::write(&path, table.to_csv()) {
            Ok(()) => result.written.push(path),
            Err(e) => result.io_errors.push(format!("could not write {}: {e}", path.display())),
        }
    }
}

fn err_file_name(csv_name: &str) -> String {
    match csv_name.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.err"),
        None => format!("{csv_name}.err"),
    }
}

fn write_err_file(result: &mut DirResult, csv_name: &str, show_workdir: bool) {
    if result.reports.is_empty() {
        return;
    }
    let text = result.reports.iter().map(|r| format_report(r, false, show_workdir)).collect::<String>();
    let path = result.dir.join(err_file_name(csv_name));
    match fs::write(&path, text) {
        Ok(()) => result.written.push(path),
        Err(e) => result.io_errors.push(format!("could not write {}: {e}", path.display())),
    }
}

fn color_code(status: Status) -> &'static str {
    match status {
        Status::Ok => "32",
        Status::Warning => "93",
        Status::Pending => "34",
        Status::Running => "36",
        Status::NotSubmitted => "0",
        Status::Error => "31",
    }
}

/// One job as text: `STATUS   name   first message` + further messages indented.
pub fn format_report(r: &JobReport, color: bool, show_workdir: bool) -> String {
    let head = format!("{:<8} {:<50}", r.status.label(), r.name);
    let head = if color { format!("\x1b[{}m{head}\x1b[0m", color_code(r.status)) } else { head };
    let mut msgs: Vec<String> = r.messages.clone();
    if !show_workdir {
        for m in msgs.iter_mut() {
            if let Some(i) = m.find(" (WorkDir:") {
                m.truncate(i);
            }
        }
    }
    let mut s = String::new();
    match msgs.split_first() {
        Some((first, rest)) => {
            s.push_str(&format!("{head}  {first}\n"));
            for m in rest {
                s.push_str(&format!("{:<59}  {m}\n", ""));
            }
        }
        None => s.push_str(&format!("{head}\n")),
    }
    s
}

fn init_threads(n: usize) {
    if n > 0 {
        // Fails only if a global pool exists already (e.g. in tests); harmless.
        let _ = rayon::ThreadPoolBuilder::new().num_threads(n).build_global();
    }
}

/// Entry point.  Returns the process exit code.
pub fn run(args: &Args) -> anyhow::Result<i32> {
    init_threads(args.threads);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let color = args.color && stdout.is_terminal();

    let (targets, problems) = collect_targets(args);
    for p in &problems {
        eprintln!("ERROR    {p}");
    }
    if targets.is_empty() {
        eprintln!("No .inp/.out files found.");
        return Ok(1);
    }

    let queue = if args.no_queue {
        Queue::disabled()
    } else {
        let load = Queue::from_squeue();
        for w in &load.warnings {
            eprintln!("WARNING  squeue: {w}");
        }
        load.queue
    };
    let mut exit = 0;

    for target in &targets {
        if targets.len() > 1 {
            writeln!(out, "\n{}\nDirectory: {}\n{}", "=".repeat(80), target.dir.display(), "=".repeat(80))?;
        }
        let result = match analyze_target(target, args, &queue) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("ERROR    {e:#}");
                exit = 1;
                continue;
            }
        };
        if result.reports.is_empty() {
            writeln!(out, "No .inp/.out files in {}.", target.dir.display())?;
            continue;
        }
        let job_types: Vec<String> = {
            let mut v: Vec<JobType> = result.reports.iter().map(|r| r.job_type).collect();
            v.sort();
            v.dedup();
            v.iter().map(|j| j.to_string().to_uppercase()).collect()
        };
        if !args.quiet {
            writeln!(out, "Job type(s): {}. Atom indices start at 0 as in ORCA.", job_types.join(", "))?;
            if result.reports.iter().any(|r| r.job_type.has_frequencies()) {
                writeln!(out, "Imaginary frequencies are relevant if <= {} cm**-1.", args.analysis.imaginary_threshold)?;
            }
            if args.analysis.accept_not_fully_converged {
                writeln!(out, "NOT checking whether wave functions are fully converged (-ncf)!")?;
            }
            if !args.analysis.warn_spin_contamination {
                writeln!(out, "NOT checking for spin contamination (-npolf)!")?;
            }
        }
        for r in &result.reports {
            if args.quiet && r.status == Status::Ok {
                continue;
            }
            write!(out, "{}", format_report(r, color, args.show_workdir))?;
        }
        let counts = count_statuses(&result.reports);
        writeln!(out, "{counts}")?;
        for e in &result.io_errors {
            eprintln!("ERROR    {e}");
            exit = 1;
        }
        let written: Vec<String> = result
            .written
            .iter()
            .filter(|p| !p.to_string_lossy().ends_with(".best.xyz"))
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        let n_xyz = result.written.iter().filter(|p| p.to_string_lossy().ends_with(".best.xyz")).count();
        let xyz_note = if n_xyz > 0 { format!(" and {n_xyz} .best.xyz file(s)") } else { String::new() };
        writeln!(out, "Wrote {}{} in {}", written.join(", "), xyz_note, target.dir.display())?;
    }
    Ok(exit)
}

pub fn count_statuses(reports: &[JobReport]) -> String {
    let mut counts: BTreeMap<Status, usize> = BTreeMap::new();
    for r in reports {
        *counts.entry(r.status).or_default() += 1;
    }
    let parts: Vec<String> = counts.iter().map(|(s, n)| format!("{n} {}", s.label())).collect();
    format!("{} job(s): {}", reports.len(), parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_names() {
        assert_eq!(csv_file_name("outorca.csv", JobType::Sp, false), "outorca.csv");
        assert_eq!(csv_file_name("outorca.csv", JobType::Sp, true), "outorca_sp.csv");
        assert_eq!(csv_file_name("res", JobType::Optfreq, true), "res_optfreq");
        assert_eq!(err_file_name("outorca.csv"), "outorca.err");
        assert_eq!(err_file_name("res"), "res.err");
    }

    #[test]
    fn format_report_hides_workdir_unless_requested() {
        let mut r = evaluate("x", InputInfo::default(), Observation::Missing, crate::analysis::QueueState::Unknown, &AnalysisOptions::default());
        r.messages = vec!["job 1 is running (WorkDir: /a)".into(), "second".into()];
        r.status = Status::Running;
        let plain = format_report(&r, false, false);
        assert_eq!(plain, format!("{:<8} {:<50}  job 1 is running\n{:<59}  second\n", "RUNNING", "x", ""));
        assert!(format_report(&r, false, true).contains("(WorkDir: /a)"));
        assert!(format_report(&r, true, false).starts_with("\x1b[36m"));
    }
}

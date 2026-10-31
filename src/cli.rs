//! Command line parsing (hand written: the flags are few and we keep the
//! short multi-letter spellings of the Python original such as `-ncf`).

use std::path::PathBuf;

use crate::analysis::AnalysisOptions;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    /// Directories or `.inp`/`.out` files; empty means the current directory.
    pub paths: Vec<PathBuf>,
    pub recursive: bool,
    pub analysis: AnalysisOptions,
    /// Print the SLURM working directory of running jobs.
    pub show_workdir: bool,
    pub color: bool,
    /// Base name of the CSV file (default `outorca.csv`).
    pub csv_name: String,
    /// Write `<name>.best.xyz` for converged optimisations.
    pub write_xyz: bool,
    /// Do not query SLURM.
    pub no_queue: bool,
    /// Number of worker threads (0 = rayon default).
    pub threads: usize,
    pub quiet: bool,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            paths: Vec::new(),
            recursive: false,
            analysis: AnalysisOptions::default(),
            show_workdir: false,
            color: true,
            csv_name: "outorca.csv".to_string(),
            write_xyz: true,
            no_queue: false,
            threads: 0,
            quiet: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CliAction {
    Run(Box<Args>),
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError(pub String);

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CliError {}

pub const USAGE: &str = "\
orcaout - summarise ORCA calculations into a CSV file

USAGE:
    orcaout [OPTIONS] [PATH...]

    PATH may be a directory (default: current directory) or individual
    .inp/.out files.  An .out file without .inp is analysed using the input
    echoed in the output.

OPTIONS:
    -r,    --recursive         Analyse all sub-directories that contain .inp files
    -ncf,  --noconvflag        Accept 'not fully converged' wave functions into the CSV
    -npolf,--nopolflag         Do not warn about spin contamination (<S^2> deviation >= 0.10)
    -imag, --imaginary <CM-1>  Frequencies at or below this value count as imaginary [default: -20]
    -a,    --all               Also write failed jobs to the CSV (adds a STATUS column)
    -cwd,  --workdir           Show the SLURM working directory of running jobs
    -of,   --onefile <OUT>     Analyse a single .out file (same as passing it as PATH)
    -o,    --output <NAME>     CSV file name [default: outorca.csv]
           --no-xyz            Do not write <name>.best.xyz for converged optimisations
           --no-queue          Do not query SLURM (squeue)
    -j,    --threads <N>       Number of parallel workers [default: number of CPUs]
    -q,    --quiet             Only print warnings and errors
           --no-color          Disable coloured output (also honours NO_COLOR)
    -h,    --help              Show this help
    -V,    --version           Show the version
";

fn value<'a>(iter: &mut impl Iterator<Item = &'a str>, flag: &str) -> Result<&'a str, CliError> {
    iter.next().ok_or_else(|| CliError(format!("option `{flag}` needs a value")))
}

/// Parse command line arguments (without the program name).
pub fn parse<I, S>(args: I) -> Result<CliAction, CliError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let owned: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let mut iter = owned.iter().map(String::as_str);
    let mut a = Args::default();
    let mut only_paths = false;

    while let Some(arg) = iter.next() {
        if only_paths || !arg.starts_with('-') || arg == "-" {
            a.paths.push(PathBuf::from(arg));
            continue;
        }
        // `--flag=value`
        let (flag, inline_value) = match arg.split_once('=') {
            Some((f, v)) if arg.starts_with("--") => (f, Some(v)),
            _ => (arg, None),
        };
        let mut take = |flag: &str| -> Result<String, CliError> {
            match inline_value {
                Some(v) => Ok(v.to_string()),
                None => value(&mut iter, flag).map(str::to_string),
            }
        };
        match flag {
            "--" => only_paths = true,
            "-h" | "--help" => return Ok(CliAction::Help),
            "-V" | "--version" => return Ok(CliAction::Version),
            "-r" | "--recursive" => a.recursive = true,
            "-ncf" | "--noconvflag" => a.analysis.accept_not_fully_converged = true,
            "-npolf" | "--nopolflag" => a.analysis.warn_spin_contamination = false,
            "-a" | "--all" => a.analysis.include_failed = true,
            "-cwd" | "--workdir" => a.show_workdir = true,
            "--no-xyz" => a.write_xyz = false,
            "--no-queue" => a.no_queue = true,
            "--no-color" => a.color = false,
            "-q" | "--quiet" => a.quiet = true,
            "-imag" | "--imaginary" => {
                let v = take(flag)?;
                a.analysis.imaginary_threshold =
                    v.parse().map_err(|_| CliError(format!("`{v}` is not a number (for {flag})")))?;
            }
            "-of" | "--onefile" => a.paths.push(PathBuf::from(take(flag)?)),
            "-o" | "--output" => {
                let v = take(flag)?;
                if v.is_empty() || v.contains('/') {
                    return Err(CliError("`--output` must be a plain file name".into()));
                }
                a.csv_name = v;
            }
            "-j" | "--threads" => {
                let v = take(flag)?;
                a.threads = v.parse().map_err(|_| CliError(format!("`{v}` is not a valid thread count")))?;
            }
            _ => return Err(CliError(format!("unknown option `{arg}` (try --help)"))),
        }
    }
    if std::env::var_os("NO_COLOR").is_some() {
        a.color = false;
    }
    Ok(CliAction::Run(Box::new(a)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Args {
        match parse(args).unwrap() {
            CliAction::Run(a) => *a,
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn defaults() {
        let a = run(&[]);
        assert!(a.paths.is_empty());
        assert!(!a.recursive);
        assert_eq!(a.analysis.imaginary_threshold, -20.0);
        assert_eq!(a.csv_name, "outorca.csv");
        assert!(a.write_xyz);
    }

    #[test]
    fn python_style_flags() {
        let a = run(&["-ncf", "-npolf", "-imag", "-50", "-cwd", "-r", "-of", "x.out"]);
        assert!(a.analysis.accept_not_fully_converged);
        assert!(!a.analysis.warn_spin_contamination);
        assert_eq!(a.analysis.imaginary_threshold, -50.0);
        assert!(a.show_workdir);
        assert!(a.recursive);
        assert_eq!(a.paths, vec![PathBuf::from("x.out")]);
    }

    #[test]
    fn long_flags_with_equals_and_paths() {
        let a = run(&["--imaginary=-10.5", "--output=res.csv", "--threads", "3", "dir1", "--all", "dir2"]);
        assert_eq!(a.analysis.imaginary_threshold, -10.5);
        assert_eq!(a.csv_name, "res.csv");
        assert_eq!(a.threads, 3);
        assert!(a.analysis.include_failed);
        assert_eq!(a.paths, vec![PathBuf::from("dir1"), PathBuf::from("dir2")]);
    }

    #[test]
    fn double_dash_ends_options() {
        let a = run(&["--", "-weird-dir"]);
        assert_eq!(a.paths, vec![PathBuf::from("-weird-dir")]);
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse(["-h"]).unwrap(), CliAction::Help);
        assert_eq!(parse(["--version"]).unwrap(), CliAction::Version);
    }

    #[test]
    fn errors() {
        assert!(parse(["--bogus"]).is_err());
        assert!(parse(["-imag"]).is_err());
        assert!(parse(["-imag", "abc"]).is_err());
        assert!(parse(["-j", "-1"]).is_err());
        assert!(parse(["-o", "a/b.csv"]).is_err());
    }
}

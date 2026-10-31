//! CSV column registry.
//!
//! Every column is one entry in [`COLUMNS`]: a header name and a function
//! that extracts the cell value from a [`JobReport`].  Each [`JobType`] has
//! its own ordered list of *required* columns ([`required_columns`]) that
//! reproduces the headers of the Python original.  All other columns are
//! *optional*: they are appended (in registry order) only when at least one
//! row in the file has a value for them.
//!
//! # Adding a column
//!
//! 1. Make sure the value is available on [`JobReport`] (or its `output`).
//! 2. Add a `Column { name, value }` entry to [`COLUMNS`] at the position
//!    where it should appear among the optional columns.
//! 3. If the column must always be present for some job types, add its name
//!    to those lists in [`required_columns`].

use crate::analysis::JobReport;
use crate::input::JobType;

pub const SEPARATOR: char = ';';

pub struct Column {
    pub name: &'static str,
    pub value: fn(&JobReport) -> Option<String>,
}

fn out(r: &JobReport) -> Option<&crate::output::OutputData> {
    r.output.as_ref()
}

fn is_opt(r: &JobReport) -> bool {
    matches!(r.job_type, JobType::Opt | JobType::Optfreq)
}

fn bool_str(b: bool) -> String {
    if b { "True".to_string() } else { "False".to_string() }
}

fn fmt_pairs<A: std::fmt::Display, B: std::fmt::Display>(pairs: impl Iterator<Item = (A, B)>) -> String {
    let inner: Vec<String> = pairs.map(|(a, b)| format!("({a},{b})")).collect();
    format!("[{}]", inner.join(","))
}

/// The registry.  Order matters only for optional columns.
pub static COLUMNS: &[Column] = &[
    Column { name: "NAME", value: |r| Some(r.name.clone()) },
    Column { name: "JOB", value: |r| Some(r.job_type.as_str().to_string()) },
    Column { name: "METHOD", value: |r| r.input.method.clone() },
    Column { name: "BASIS", value: |r| r.input.basis_set.clone() },
    Column { name: "ENERGY", value: |r| r.energy.map(|e| format!("{e}")) },
    Column { name: "CHRG", value: |r| r.charge.map(|c| c.to_string()) },
    Column { name: "M", value: |r| r.multiplicity.map(|m| m.to_string()) },
    Column { name: "S^2", value: |r| r.spin.map(|s| format!("{:.3}", s.s2.abs())) },
    Column { name: "N_FOD", value: |r| out(r).and_then(|o| o.n_fod).map(|n| format!("{n}")) },
    Column { name: "SCFCONV", value: |r| r.scf.map(|s| s.as_str().to_string()) },
    Column { name: "GEOCONV", value: |r| out(r).filter(|_| is_opt(r)).map(|o| bool_str(o.geom_converged)) },
    Column { name: "GEOCYCLES", value: |r| out(r).filter(|_| is_opt(r)).and_then(|o| o.geom_cycles).map(|c| c.to_string()) },
    // CP-SCF (analytic freq, NMR) or, for TD-DFT, the Davidson solver; empty if none ran.
    Column {
        name: "CPSCFCONV",
        value: |r| out(r).and_then(|o| crate::analysis::response_convergence(o, r.job_type)).map(bool_str),
    },
    Column { name: "ZPVE", value: |r| r.thermo.map(|t| format!("{}", t.zpve)) },
    Column { name: "dU", value: |r| r.thermo.map(|t| format!("{}", t.du)) },
    Column { name: "dH", value: |r| r.thermo.map(|t| format!("{}", t.dh)) },
    Column { name: "TdS", value: |r| r.thermo.map(|t| format!("{}", t.tds)) },
    Column { name: "dG", value: |r| r.thermo.map(|t| format!("{}", t.dg)) },
    Column { name: "CHAR", value: |r| r.character.map(|c| c.as_str().to_string()) },
    Column {
        name: "IMAGS",
        value: |r| r.character.map(|_| r.imaginary.iter().map(|f| format!("{f}")).collect::<Vec<_>>().join("|")),
    },
    Column { name: "NUM_GHST_ATMS", value: |r| (r.job_type == JobType::Nmr).then(|| r.input.num_ghost_atoms().to_string()) },
    Column {
        name: "NICS[ID,ISO]",
        value: |r| {
            (r.job_type == JobType::Nmr)
                .then(|| format!("[{}]", r.nics.iter().map(|(i, v)| format!("[{i}, {v}]")).collect::<Vec<_>>().join(", ")))
        },
    },
    Column {
        name: "SCAN[COORD,E]",
        value: |r| out(r).filter(|o| !o.scan_surface.is_empty()).map(|o| fmt_pairs(o.scan_surface.iter().copied())),
    },
    Column {
        name: "EXC[STATE,EV]",
        value: |r| {
            out(r)
                .filter(|o| !o.tddft_states.is_empty())
                .map(|o| fmt_pairs(o.tddft_states.iter().map(|s| (s.state, s.energy_ev))))
        },
    },
    Column {
        name: "SPEC[EV,FOSC]",
        value: |r| out(r).filter(|o| !o.absorption.is_empty()).map(|o| fmt_pairs(o.absorption.iter().copied())),
    },
    Column { name: "DISP", value: |r| out(r).and_then(|o| o.dispersion_eh).map(|d| format!("{d}")) },
    Column { name: "DIPOLE", value: |r| out(r).and_then(|o| o.dipole_debye).map(|d| format!("{d}")) },
    Column { name: "RUNTIME", value: |r| out(r).and_then(|o| o.run_time_s).map(|t| format!("{t}")) },
    Column { name: "STATUS", value: |r| r.is_failed().then(|| r.status.label().to_string()) },
];

const SP: &[&str] = &["NAME", "JOB", "METHOD", "ENERGY", "CHRG", "M", "S^2", "SCFCONV"];
const FTDFT: &[&str] = &["NAME", "JOB", "METHOD", "ENERGY", "CHRG", "M", "N_FOD", "SCFCONV"];
const OPT: &[&str] = &["NAME", "JOB", "METHOD", "ENERGY", "CHRG", "M", "S^2", "SCFCONV", "GEOCONV"];
const FREQ: &[&str] = &[
    "NAME", "JOB", "METHOD", "ENERGY", "CHRG", "M", "S^2", "SCFCONV", "CPSCFCONV", "ZPVE", "dU", "dH", "TdS", "dG", "CHAR",
    "IMAGS",
];
const OPTFREQ: &[&str] = &[
    "NAME", "JOB", "METHOD", "ENERGY", "CHRG", "M", "S^2", "SCFCONV", "GEOCONV", "CPSCFCONV", "ZPVE", "dU", "dH", "TdS", "dG",
    "CHAR", "IMAGS",
];
const NMR: &[&str] =
    &["NAME", "JOB", "METHOD", "ENERGY", "CHRG", "M", "S^2", "SCFCONV", "CPSCFCONV", "NUM_GHST_ATMS", "NICS[ID,ISO]"];
const SCAN: &[&str] = &["NAME", "JOB", "METHOD", "ENERGY", "CHRG", "M", "S^2", "SCFCONV", "SCAN[COORD,E]"];
const TDDFT: &[&str] = &["NAME", "JOB", "METHOD", "ENERGY", "CHRG", "M", "S^2", "SCFCONV", "CPSCFCONV", "EXC[STATE,EV]"];
const INVALID: &[&str] = &["NAME", "JOB"];

/// Columns that are always written for a job type, in order.
pub fn required_columns(job: JobType) -> &'static [&'static str] {
    match job {
        JobType::Sp => SP,
        JobType::Ftdft => FTDFT,
        JobType::Opt => OPT,
        JobType::Freq => FREQ,
        JobType::Optfreq => OPTFREQ,
        JobType::Nmr => NMR,
        JobType::Scan => SCAN,
        JobType::Tddft => TDDFT,
        JobType::Invalid => INVALID,
    }
}

fn column(name: &str) -> &'static Column {
    COLUMNS
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("column `{name}` is listed as required but missing from COLUMNS"))
}

/// Header + rows for one job type.  `reports` are expected to be of that
/// job type and already filtered for `in_csv`.
pub fn build_table(job: JobType, reports: &[&JobReport]) -> Table {
    let required = required_columns(job);
    let mut columns: Vec<&'static Column> = required.iter().map(|n| column(n)).collect();
    for c in COLUMNS {
        if required.contains(&c.name) {
            continue;
        }
        if reports.iter().any(|r| (c.value)(r).is_some_and(|v| !v.is_empty())) {
            columns.push(c);
        }
    }
    let header: Vec<String> = columns.iter().map(|c| c.name.to_string()).collect();
    let rows: Vec<Vec<String>> = reports
        .iter()
        .map(|r| columns.iter().map(|c| sanitize((c.value)(r).unwrap_or_default())).collect())
        .collect();
    Table { header, rows }
}

/// Cells must not contain the separator or line breaks.
fn sanitize(cell: String) -> String {
    if cell.contains(SEPARATOR) || cell.contains('\n') || cell.contains('\r') {
        cell.replace(SEPARATOR, ",").replace(['\n', '\r'], " ")
    } else {
        cell
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn to_csv(&self) -> String {
        let sep = SEPARATOR.to_string();
        let mut s = self.header.join(&sep);
        for row in &self.rows {
            s.push('\n');
            s.push_str(&row.join(&sep));
        }
        s.push('\n');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{AnalysisOptions, Observation, QueueState, evaluate};
    use crate::input::parse_input_text;
    use crate::output::parse_output_text;

    fn report(inp: &str, out: &str) -> JobReport {
        evaluate(
            "job",
            parse_input_text(inp),
            Observation::Parsed(Box::new(parse_output_text(out))),
            QueueState::Unknown,
            &AnalysisOptions::default(),
        )
    }

    #[test]
    fn every_required_column_exists_in_registry() {
        for jt in JobType::ALL {
            for name in required_columns(jt) {
                assert!(COLUMNS.iter().any(|c| c.name == *name), "{jt}: {name}");
            }
        }
    }

    #[test]
    fn registry_names_are_unique() {
        let mut names: Vec<_> = COLUMNS.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), COLUMNS.len());
    }

    #[test]
    fn python_headers_are_reproduced() {
        assert_eq!(required_columns(JobType::Sp).join(";"), "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV");
        assert_eq!(required_columns(JobType::Ftdft).join(";"), "NAME;JOB;METHOD;ENERGY;CHRG;M;N_FOD;SCFCONV");
        assert_eq!(required_columns(JobType::Opt).join(";"), "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;GEOCONV");
        assert_eq!(
            required_columns(JobType::Freq).join(";"),
            "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;CPSCFCONV;ZPVE;dU;dH;TdS;dG;CHAR;IMAGS"
        );
        assert_eq!(
            required_columns(JobType::Optfreq).join(";"),
            "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;GEOCONV;CPSCFCONV;ZPVE;dU;dH;TdS;dG;CHAR;IMAGS"
        );
        assert_eq!(
            required_columns(JobType::Nmr).join(";"),
            "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;CPSCFCONV;NUM_GHST_ATMS;NICS[ID,ISO]"
        );
    }

    #[test]
    fn sp_table_with_optional_columns_only_when_present() {
        let r = report(
            "! uks wb97x-d4rev def2-qzvppd\n*xyz -2 4\nCo 0 0 0\n*",
            "FINAL SINGLE POINT ENERGY   -4128.719797087585\nExpectation value of <S**2>     :     3.767755\nIdeal value S*(S+1) for S=1.5   :     3.750000\nDeviation                       :     0.017755\nDispersion correction           -0.278380990\nMagnitude (Debye)      :      7.734068328\n****ORCA TERMINATED NORMALLY****\n",
        );
        let t = build_table(JobType::Sp, &[&r]);
        assert_eq!(t.header.join(";"), "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;BASIS;DISP;DIPOLE");
        assert_eq!(
            t.rows[0].join(";"),
            "job;sp;wb97x-d4rev;-4128.719797087585;-2;4;3.768;True;def2-qzvppd;-0.27838099;7.734068328"
        );
        assert_eq!(
            t.to_csv(),
            "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;BASIS;DISP;DIPOLE\njob;sp;wb97x-d4rev;-4128.719797087585;-2;4;3.768;True;def2-qzvppd;-0.27838099;7.734068328\n"
        );
    }

    #[test]
    fn no_optional_columns_without_values() {
        let r = report("! r2scan-3c\n* xyz 0 1\nH 0 0 0\n*", "FINAL SINGLE POINT ENERGY   -1.0\n****ORCA TERMINATED NORMALLY****\n");
        let t = build_table(JobType::Sp, &[&r]);
        assert_eq!(t.header.join(";"), "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV");
        assert_eq!(t.rows[0].join(";"), "job;sp;r2scan-3c;-1;0;1;;True");
    }

    #[test]
    fn empty_table_has_only_header() {
        let t = build_table(JobType::Opt, &[]);
        assert_eq!(t.to_csv(), "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;GEOCONV\n");
    }

    #[test]
    fn scan_column_is_plottable_tuples() {
        let r = report(
            "! r2scan-3c opt\n%geom\n scan\n  b 0 1 = 0.7, 0.9, 3\n end\nend\n* xyz 0 1\nH 0 0 0\nH 0 0 0.7\n*",
            "FINAL SINGLE POINT ENERGY   -1.0\nThe Calculated Surface using the 'Actual Energy'\n   0.70000000  -1.11675931\n   0.80000000  -1.16500000\n   0.90000000  -1.15000000\n\n****ORCA TERMINATED NORMALLY****\n",
        );
        let t = build_table(JobType::Scan, &[&r]);
        assert_eq!(t.header.last().map(String::as_str), Some("SCAN[COORD,E]"));
        assert_eq!(t.rows[0].last().map(String::as_str), Some("[(0.7,-1.11675931),(0.8,-1.165),(0.9,-1.15)]"));
    }

    #[test]
    fn status_column_only_for_failed_rows() {
        let good = report("! r2scan-3c\n* xyz 0 1\nH 0 0 0\n*", "FINAL SINGLE POINT ENERGY   -1.0\n****ORCA TERMINATED NORMALLY****\n");
        let t = build_table(JobType::Sp, &[&good]);
        assert!(!t.header.contains(&"STATUS".to_string()));
        let mut bad = good.clone();
        bad.status = crate::analysis::Status::Error;
        let t = build_table(JobType::Sp, &[&good, &bad]);
        assert!(t.header.contains(&"STATUS".to_string()));
        assert_eq!(t.rows[1].last().map(String::as_str), Some("ERROR"));
        assert_eq!(t.rows[0].last().map(String::as_str), Some(""));
    }

    #[test]
    fn separator_in_cell_is_sanitized() {
        assert_eq!(sanitize("a;b\nc".into()), "a,b c");
        assert_eq!(sanitize("plain".into()), "plain");
    }
}

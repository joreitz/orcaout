//! Parsing of ORCA input files (`.inp`).
//!
//! Only the information that the CSV report needs is extracted: job type,
//! method, basis set, charge, multiplicity and the coordinate block (to count
//! atoms and locate ghost atoms for NICS).  Everything is best effort: a
//! field that cannot be determined is `None`, parsing never fails.

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use anyhow::Context;

use crate::known::{known_basis_sets, known_methods};

/// Kind of calculation requested in the input.  The order of the variants
/// is the order in which per-job-type CSV files are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum JobType {
    Sp,
    Opt,
    Optfreq,
    Freq,
    Scan,
    Tddft,
    Nmr,
    /// Fermi-smeared / FOD single point (`! FOD` or `%scf SmearTemp`).
    Ftdft,
    /// No `!` line could be found.
    #[default]
    Invalid,
}

impl JobType {
    pub const ALL: [JobType; 9] = [
        JobType::Sp,
        JobType::Opt,
        JobType::Optfreq,
        JobType::Freq,
        JobType::Scan,
        JobType::Tddft,
        JobType::Nmr,
        JobType::Ftdft,
        JobType::Invalid,
    ];

    /// Lower-case name used in the `JOB` column and in file names.
    pub fn as_str(self) -> &'static str {
        match self {
            JobType::Sp => "sp",
            JobType::Opt => "opt",
            JobType::Optfreq => "optfreq",
            JobType::Freq => "freq",
            JobType::Scan => "scan",
            JobType::Tddft => "tddft",
            JobType::Nmr => "nmr",
            JobType::Ftdft => "ftdft",
            JobType::Invalid => "invalid",
        }
    }

    pub fn is_optimization(self) -> bool {
        matches!(self, JobType::Opt | JobType::Optfreq | JobType::Scan)
    }

    pub fn has_frequencies(self) -> bool {
        matches!(self, JobType::Freq | JobType::Optfreq)
    }
}

impl fmt::Display for JobType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Everything we know about an input file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputInfo {
    pub job_type: JobType,
    pub method: Option<String>,
    pub basis_set: Option<String>,
    pub charge: Option<i32>,
    pub multiplicity: Option<u32>,
    /// Number of real atoms (ghost atoms and point charges excluded).
    /// `None` when the coordinates come from an external file.
    pub num_atoms: Option<usize>,
    /// Indices (ORCA numbering, starting at 0) of ghost atoms (`H:` syntax).
    pub ghost_ids: Vec<usize>,
    /// Raw coordinate lines between `* xyz C M` and `*`, original case.
    pub coordinates: Vec<String>,
}

impl InputInfo {
    pub fn num_ghost_atoms(&self) -> usize {
        self.ghost_ids.len()
    }
}

/// Extract the electronic-structure method from the keyword set.
pub fn method_from_keywords(keywords: &HashSet<&str>, methods: &HashSet<&str>) -> Option<String> {
    keywords.iter().find(|k| methods.contains(*k)).map(|k| k.to_string())
}

/// Extract the orbital basis set from the keyword set.
pub fn basisset_from_keywords(keywords: &HashSet<&str>, basis_sets: &HashSet<&str>) -> Option<String> {
    keywords.iter().find(|k| basis_sets.contains(*k)).map(|k| k.to_string())
}

/// Strip an ORCA comment (`#` to end of line).
fn strip_comment(line: &str) -> &str {
    line.split('#').next().unwrap_or("")
}

/// Parse the text of an ORCA input file.
pub fn parse_input_text(content: &str) -> InputInfo {
    let mut keywords: HashSet<String> = HashSet::new();
    let mut block_tokens: HashSet<String> = HashSet::new();
    let mut info = InputInfo::default();

    // Coordinate block state
    let mut in_coords = false;
    let mut in_coords_block = false; // %coords ... end
    let mut coordinates: Vec<String> = Vec::new();
    let mut external_coordinates = false;

    for raw in content.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_lowercase();

        if in_coords {
            if lower == "*" || lower.starts_with("* ") || lower == "end" {
                in_coords = false;
            } else {
                coordinates.push(line.to_string());
            }
            continue;
        }

        if lower.starts_with('!') {
            keywords.extend(lower.trim_start_matches('!').split_whitespace().map(str::to_string));
            continue;
        }

        // Coordinate header: `* xyz C M`, `*xyz C M`, `* int C M`, `* gzmt C M`, `* xyzfile C M file`
        if lower.starts_with('*') {
            let mut tokens = lower.trim_start_matches('*').split_whitespace();
            if let Some(kind) = tokens.next() {
                let (charge, mult) = (tokens.next(), tokens.next());
                if let Some(c) = charge.and_then(|c| c.parse::<i32>().ok()) {
                    info.charge = Some(c);
                }
                if let Some(m) = mult.and_then(|m| m.parse::<u32>().ok()) {
                    info.multiplicity = Some(m);
                }
                match kind {
                    "xyz" | "int" | "gzmt" => in_coords = true,
                    "xyzfile" | "gzmtfile" | "intfile" => external_coordinates = true,
                    _ => {}
                }
            }
            continue;
        }

        // %coords block (alternative way to give charge/multiplicity/coordinates)
        if lower.starts_with("%coords") {
            in_coords_block = true;
            block_tokens.extend(lower.split_whitespace().map(str::to_string));
            continue;
        }
        if in_coords_block {
            let mut tokens = lower.split_whitespace();
            match tokens.next() {
                Some("charge") => info.charge = tokens.next().and_then(|v| v.parse().ok()).or(info.charge),
                Some("mult") | Some("multiplicity") => {
                    info.multiplicity = tokens.next().and_then(|v| v.parse().ok()).or(info.multiplicity)
                }
                Some("coords") => in_coords = true,
                Some("end") => in_coords_block = false,
                _ => {}
            }
            continue;
        }

        block_tokens.extend(lower.split_whitespace().map(str::to_string));
    }

    if keywords.is_empty() {
        info.job_type = JobType::Invalid;
        return info;
    }

    let keyword_refs: HashSet<&str> = keywords.iter().map(String::as_str).collect();
    let block_refs: HashSet<&str> = block_tokens.iter().map(String::as_str).collect();
    info.job_type = decide_job_type(&keyword_refs, &block_refs);
    info.method = method_from_keywords(&keyword_refs, &known_methods());
    info.basis_set = basisset_from_keywords(&keyword_refs, &known_basis_sets());

    // Atom bookkeeping
    let mut real_atoms = 0usize;
    for (idx, coord) in coordinates.iter().enumerate() {
        let mut tokens = coord.split_whitespace();
        let first = tokens.next().unwrap_or("");
        let second = tokens.next().unwrap_or("");
        let is_ghost = first.ends_with(':') || second == ":" || second.starts_with(':');
        let is_point_charge = first.eq_ignore_ascii_case("q");
        if is_ghost {
            info.ghost_ids.push(idx);
        } else if !is_point_charge {
            real_atoms += 1;
        }
    }
    info.num_atoms = if external_coordinates && coordinates.is_empty() { None } else { Some(real_atoms) };
    info.coordinates = coordinates;
    info
}

/// Read an ORCA input file and parse it.  Bytes that are not valid UTF-8 are
/// replaced instead of aborting.
pub fn info_from_input(path: &Path) -> anyhow::Result<InputInfo> {
    let bytes = std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    Ok(parse_input_text(&String::from_utf8_lossy(&bytes)))
}

pub fn decide_job_type(keywords: &HashSet<&str>, block_tokens: &HashSet<&str>) -> JobType {
    let any = |set: &HashSet<&str>, keys: &[&str]| keys.iter().any(|k| set.contains(k));

    let is_opt = any(keywords, &["opt", "tightopt", "verytightopt", "looseopt", "normalopt", "copt", "l-opt", "l-optl"]);
    let is_freq = any(keywords, &["freq", "numfreq", "anfreq"]);
    let is_scan = any(block_tokens, &["scan"]);
    let is_nmr = any(keywords, &["nmr"]);
    let is_tddft = any(keywords, &["tddft", "tda", "cis", "cisd"]) || any(block_tokens, &["%tddft", "%cis", "nroots"]);
    let is_ftdft = any(keywords, &["smear", "fod"]) || any(block_tokens, &["smeartemp"]);

    match (is_scan, is_opt, is_freq) {
        (true, _, _) => JobType::Scan,
        (false, true, true) => JobType::Optfreq,
        (false, true, false) => JobType::Opt,
        (false, false, true) => JobType::Freq,
        (false, false, false) if is_nmr => JobType::Nmr,
        (false, false, false) if is_tddft => JobType::Tddft,
        (false, false, false) if is_ftdft => JobType::Ftdft,
        (false, false, false) => JobType::Sp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H2: &str = "* xyz 0 1\nH 0.0 0.0 0.0\nH 0.0 0.0 0.74\n*";

    #[test]
    fn opt_at_end_of_line() {
        let info = parse_input_text("! wb97x-3c opt");
        assert_eq!(info.job_type, JobType::Opt);
        assert_eq!(info.method, Some("wb97x-3c".to_string()));
        assert_eq!(info.basis_set, None);
    }

    #[test]
    fn optfreq_over_two_bang_lines() {
        let info = parse_input_text("! wb97x-3c tightopt\n! freq");
        assert_eq!(info.job_type, JobType::Optfreq);
    }

    #[test]
    fn opt_only_in_comment() {
        assert_eq!(parse_input_text("! r2scan-3c # opt").job_type, JobType::Sp);
    }

    #[test]
    fn scan_beats_opt() {
        let input = "! r2scan-3c opt\n%geom\n  scan\n    b 0 1 = 0.7, 1.5, 9\n  end\nend";
        assert_eq!(parse_input_text(input).job_type, JobType::Scan);
    }

    #[test]
    fn method_and_basis_set() {
        let info = parse_input_text("! PBE0 def2-TZVP D4 SP");
        assert_eq!(info.method, Some("pbe0".to_string()));
        assert_eq!(info.basis_set, Some("def2-tzvp".to_string()));
    }

    #[test]
    fn no_bang_line_is_invalid() {
        let info = parse_input_text("%method\n  functional pbe0\nend");
        assert_eq!(info.job_type, JobType::Invalid);
        assert_eq!(info.method, None);
        assert_eq!(info.basis_set, None);
    }

    #[test]
    fn charge_multiplicity_and_atoms() {
        let info = parse_input_text(&format!("! r2scan-3c\n{H2}"));
        assert_eq!(info.charge, Some(0));
        assert_eq!(info.multiplicity, Some(1));
        assert_eq!(info.num_atoms, Some(2));
        assert!(info.ghost_ids.is_empty());
        assert_eq!(info.coordinates.len(), 2);
    }

    #[test]
    fn negative_charge_without_space_after_star() {
        let info = parse_input_text("! UKS SP WB97X-D4REV DEF2-QZVPPD\n*xyz -2 4\n  P 1.0 2.0 3.0\n  Co 0.0 0.0 0.0\n*\n");
        assert_eq!(info.charge, Some(-2));
        assert_eq!(info.multiplicity, Some(4));
        assert_eq!(info.num_atoms, Some(2));
        assert_eq!(info.method.as_deref(), Some("wb97x-d4rev"));
        assert_eq!(info.basis_set.as_deref(), Some("def2-qzvppd"));
        assert_eq!(info.job_type, JobType::Sp);
    }

    #[test]
    fn ghost_atoms_are_detected_and_not_counted() {
        let input = "! pbe0 def2-tzvp nmr\n* xyz 0 1\nC 0 0 0\nH: 0 0 1\nH : 0 0 2\nC 0 1 0\n*";
        let info = parse_input_text(input);
        assert_eq!(info.job_type, JobType::Nmr);
        assert_eq!(info.ghost_ids, vec![1, 2]);
        assert_eq!(info.num_ghost_atoms(), 2);
        assert_eq!(info.num_atoms, Some(2));
    }

    #[test]
    fn point_charges_are_not_atoms() {
        let info = parse_input_text("! hf\n* xyz 0 1\nO 0 0 0\nQ -0.8 1 1 1\n*");
        assert_eq!(info.num_atoms, Some(1));
    }

    #[test]
    fn xyzfile_gives_charge_but_no_atom_count() {
        let info = parse_input_text("! r2scan-3c opt\n* xyzfile 1 2 h2.xyz");
        assert_eq!(info.charge, Some(1));
        assert_eq!(info.multiplicity, Some(2));
        assert_eq!(info.num_atoms, None);
        assert_eq!(info.job_type, JobType::Opt);
    }

    #[test]
    fn coords_block_is_understood() {
        let input = "! hf\n%coords\n  CTyp xyz\n  Charge -1\n  Mult 2\n  Units Angs\n  coords\n    O 0 0 0\n    H 0 0 1\n  end\nend";
        let info = parse_input_text(input);
        assert_eq!(info.charge, Some(-1));
        assert_eq!(info.multiplicity, Some(2));
        assert_eq!(info.num_atoms, Some(2));
    }

    #[test]
    fn fermi_smearing_and_fod() {
        assert_eq!(parse_input_text("! r2scan-3c\n%scf\n  SmearTemp 5000\nend").job_type, JobType::Ftdft);
        assert_eq!(parse_input_text("! fod").job_type, JobType::Ftdft);
    }

    #[test]
    fn tddft_via_block_or_keyword() {
        assert_eq!(parse_input_text("! wb97x-d3 def2-tzvp\n%tddft\n  nroots 10\nend").job_type, JobType::Tddft);
        assert_eq!(parse_input_text("! b3lyp tddft").job_type, JobType::Tddft);
    }

    #[test]
    fn garbage_input_is_invalid_and_does_not_panic() {
        let info = parse_input_text("asdf\n\n*\n* xyz\n! \n");
        assert_eq!(info.job_type, JobType::Invalid);
        let info = parse_input_text("");
        assert_eq!(info.job_type, JobType::Invalid);
    }
}

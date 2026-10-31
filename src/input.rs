use crate::HashSet;
use crate::known_basis_sets;
use crate::Path;
use crate::known_methods;
use crate::read_to_string;
use anyhow::Context;

#[derive(Debug, PartialEq)]
pub enum  JobType {
    Sp,
    Opt,
    Optfreq,
    Freq,
    Scan,
    Tddft,
    Nmr,
    Ftdft,
    Invalid
}

#[derive(Debug)]
pub struct InputInfo {
    pub method: Option<String>,
    pub job_type: JobType,
    pub basis_set: Option<String>
}

/// Extracting electronic structure method from keyword list
pub fn method_from_keywords(keywords: &HashSet<&str>, methods: &HashSet<&str>) -> Option<String> {
    keywords
        .iter()
        .find(|k| methods.contains(*k))
        .map(|k| k.to_string())
}

/// Extracting basis_set from keyword list
pub fn basisset_from_keywords(keywords: &HashSet<&str>, basis_sets: &HashSet<&str>) -> Option<String> {
    keywords
        .iter()
        .find(|k| basis_sets.contains(*k))
        .map(|k| k.to_string())
}

// Getting the JobType from Head line of input files
/// Determine the job type from the text of an ORCA input file.
pub fn parse_input_text(content: &str) -> InputInfo {
    let content = content.to_lowercase();
    let mut keywords: HashSet<&str> = HashSet::new();
    let mut block_tokens: HashSet<&str> = HashSet::new();

    for line in content.lines() {
        // Drop comments: everything after '#'
        let line_prepared = line.split('#').next().unwrap_or("").trim_start();
        if line_prepared.starts_with('!') {
            keywords.extend(line_prepared.trim_start_matches('!').split_whitespace());
        } else {
            block_tokens.extend(line_prepared.split_whitespace());
        }
    }

    if keywords.is_empty() {
        let input = InputInfo {
            job_type: JobType::Invalid,
            method: None,
            basis_set: None
        };
        return input
    }

    let basis_sets = known_basis_sets();
    let methods = known_methods();
    InputInfo {
        job_type: decide_job_type(&keywords, &block_tokens),
        method: method_from_keywords(&keywords, &methods),
        basis_set: basisset_from_keywords(&keywords, &basis_sets)
    }
}

/// Read an ORCA input file and determine its job type.
pub fn info_from_input(path: &Path) -> anyhow::Result<InputInfo> {
    let content = read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    Ok(parse_input_text(&content))
}

pub fn decide_job_type(keywords: &HashSet<&str>, block_tokens: &HashSet<&str>) -> JobType {
    // Check for geometry optimization calculations
    let is_opt = ["opt", "tightopt", "verytightopt", "looseopt"]
        .iter()
        .any(|k| keywords.contains(k));
    // Check for frequency calculations
    let is_freq: bool = ["freq", "numfreq"]
        .iter()
        .any(|k| keywords.contains(k));
    // Check for PES scans
    let is_scan: bool = ["scan"]
        .iter()
        .any(|k| block_tokens.contains(k));
    // Check for NNMR calculations
    let is_nmr: bool = ["nmr"]
        .iter()
        .any(|k| keywords.contains(k));
    // Check for TDDFT calculations
    let is_tddft: bool = {["tddft"].iter().any(|k| keywords.contains(k)) || ["%tddft", "nroots"].iter().any(|k| block_tokens.contains(k))};
    let is_ftdft: bool = {["smear", "fod"].iter().any(|k| keywords.contains(k)) || ["smeartemp"].iter().any(|k| block_tokens.contains(k))};

    match (is_scan, is_opt, is_freq) {
        (true, _, _)                      => JobType::Scan,
        (false, true, true)               => JobType::Optfreq,
        (false, true, false)              => JobType::Opt,
        (false, false, true)              => JobType::Freq,
        (false, false, false) if is_nmr   => JobType::Nmr,
        (false, false, false) if is_tddft => JobType::Tddft,
        (false, false, false) if is_ftdft => JobType::Ftdft,
        (false, false, false)             => JobType::Sp
    }
}

mod known;
mod input;
mod status;
use crate::status::process_input;
use crate::known::{known_methods, known_basis_sets};
use input::{JobType, info_from_input, parse_input_text};

use std::{env::current_dir, path::PathBuf};
use std::fs::read_to_string;
use std::path::Path;
use std::collections::HashSet;

fn main() -> anyhow::Result<()> {
    
    let current_dir = current_dir()?;
    let read=  std::fs::read_dir(current_dir)?;
    
    // Init. list of all important files in current Directory
    let mut inputs: Vec<PathBuf> = Vec::new();
    
    // Collecting all .inp files of the current Directory in `inputs`
    for entry in read {
        let entry = entry?;
        let path = entry.path();
        if let Some(ext) = path.extension() {
            if ext == "inp" && path.is_file() {
                    inputs.push(path)
            }
        }
    };

    //Sorting inputs for reliable sorting
    inputs.sort();

    // Parsing
    //for path in &inputs {
    //    let tmp_result  = info_from_input(path)?;
    //    if tmp_result.job_type == JobType::Invalid {
    //        eprintln!("{} has no '!' line", path.display());
    //    }
    //    println!("{}: {:?}/{:?}", path.display(), tmp_result.method, tmp_result.basis_set);
    //}
    
    let mut results = Vec::new();
    for input in &inputs {
        results.push(process_input(input)?);
    }
    for r in &results {
        println!("{:<40} {:?} {:?}", r.name, r.info.job_type, r.out_state);
    }

    //Return
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

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
}



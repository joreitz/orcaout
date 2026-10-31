use crate::Path;
use crate::input::{InputInfo, info_from_input};
use anyhow::Context;

#[derive(Debug)]
pub enum OutFileState {
    Missing,
    Empty,
    Present,
}

pub struct JobResult {
    pub name: String,
    pub info: InputInfo,
    pub out_state: OutFileState,
}

pub fn process_input(input: &Path) -> anyhow::Result<JobResult> {
    let info = info_from_input(input)?;
    let out_path = input.with_extension("out");

    let out_state = match std::fs::metadata(&out_path) {
        Ok(meta) if meta.len() < 1000 => OutFileState::Empty,
        Ok(_) => OutFileState::Present,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => OutFileState::Missing,
        Err(e) => return Err(e).with_context(|| format!("could not access {}", out_path.display())),
        // your match from before, each arm yields an OutFileState
    };

    let name = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();

    Ok(JobResult { name, info, out_state })
}

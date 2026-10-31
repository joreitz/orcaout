//! `orcaout` — summarise ORCA quantum-chemistry calculations into CSV files.
//!
//! Pipeline: [`input`] parses the `.inp`, [`output`] streams through the
//! `.out`, [`analysis`] turns both into a status + derived values,
//! [`columns`] renders the per-job-type CSV, [`app`] ties it together.

pub mod analysis;
pub mod app;
pub mod cli;
pub mod columns;
pub mod input;
pub mod known;
pub mod output;
pub mod queue;

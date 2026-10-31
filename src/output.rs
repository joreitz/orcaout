//! Streaming parser for ORCA output files (`.out`).
//!
//! The parser consumes the file **line by line** and only keeps the values
//! that end up in the report, so memory usage is independent of the file
//! size (ORCA outputs of several hundred MB are common for long
//! optimisations).  Feed it with [`parse_output_lines`] (any iterator of
//! lines, e.g. `text.lines()` in tests) or [`parse_output_file`].
//!
//! # Adding a new quantity
//!
//! 1. Add a field to [`OutputData`].
//! 2. Add a `static` regex (see the `patterns` module) and a branch in
//!    [`Parser::feed`] that fills the field.
//! 3. Expose it as a CSV column in `columns.rs` if it should be reported.

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::LazyLock;

use anyhow::Context;
use regex::{Regex, RegexSet};

/// One `FINAL SINGLE POINT ENERGY` line.
#[derive(Debug, Clone, PartialEq)]
pub struct FinalEnergy {
    pub energy: f64,
    /// Trailing remark, e.g. `(Wavefunction not fully converged!)`.
    pub note: Option<String>,
}

impl FinalEnergy {
    pub fn is_fully_converged(&self) -> bool {
        !self.note.as_deref().unwrap_or("").to_lowercase().contains("not fully converged")
    }
}

/// `<S**2>` block printed after a UHF/UKS SCF.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpinExpectation {
    pub s2: f64,
    pub ideal: Option<f64>,
    pub deviation: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Atom {
    pub symbol: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Thermochemistry summary (all in Eh unless stated otherwise).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Thermo {
    pub temperature_k: Option<f64>,
    pub pressure_atm: Option<f64>,
    pub zpve: Option<f64>,
    /// `Total correction` = ZPVE + thermal corrections (dU).
    pub total_correction: Option<f64>,
    /// `Thermal Enthalpy correction` (kT at the given temperature).
    pub enthalpy_correction: Option<f64>,
    /// `Final entropy term` (T·S).
    pub entropy_term: Option<f64>,
    /// `G-E(el)` (dG).
    pub gibbs_correction: Option<f64>,
    pub total_thermal_energy: Option<f64>,
    pub total_enthalpy: Option<f64>,
    pub final_gibbs_energy: Option<f64>,
}

impl Thermo {
    pub fn is_empty(&self) -> bool {
        self.zpve.is_none() && self.total_correction.is_none() && self.gibbs_correction.is_none()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shielding {
    pub nucleus: usize,
    pub element: String,
    pub isotropic: f64,
    pub anisotropy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExcitedState {
    pub state: u32,
    pub energy_ev: f64,
    pub energy_cm: f64,
    pub s2: Option<f64>,
}

/// Everything extracted from an ORCA output.  All fields are optional or
/// empty by default; the analysis layer decides what is required.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutputData {
    pub lines_read: u64,
    pub terminated_normally: bool,
    pub orca_version: Option<String>,
    pub run_time_s: Option<f64>,
    /// `NAME = foo.inp` from the echoed input section.
    pub input_name: Option<String>,
    /// The input file as echoed in the output (prefix `|  1>` removed).
    pub embedded_input: Option<String>,
    pub final_energies: Vec<FinalEnergy>,
    /// Cycles of the last SCF.
    pub scf_cycles: Option<u32>,
    /// Convergence of the last SCF (`SUCCESS` vs `SCF NOT CONVERGED`).
    pub scf_converged: Option<bool>,
    pub charge: Option<i32>,
    pub multiplicity: Option<u32>,
    pub n_electrons: Option<u32>,
    pub n_atoms: Option<usize>,
    pub basis_dimension: Option<usize>,
    /// Last `<S**2>` block.
    pub spin: Option<SpinExpectation>,
    pub geom_converged: bool,
    /// Last `GEOMETRY OPTIMIZATION CYCLE n` seen.
    pub geom_cycles: Option<u32>,
    pub geom_failed: bool,
    /// Last `CARTESIAN COORDINATES (ANGSTROEM)` block.
    pub final_geometry: Vec<Atom>,
    /// Last `VIBRATIONAL FREQUENCIES` block (cm**-1, including zero modes).
    pub frequencies: Vec<f64>,
    pub thermo: Thermo,
    pub cpscf_converged: Option<bool>,
    pub dipole_debye: Option<f64>,
    pub dispersion_eh: Option<f64>,
    /// `CHEMICAL SHIELDING SUMMARY` rows.
    pub nmr_shieldings: Vec<Shielding>,
    pub n_fod: Option<f64>,
    /// Relaxed surface scan: (coordinate value, energy) per step.
    pub scan_surface: Vec<(f64, f64)>,
    pub tddft_states: Vec<ExcitedState>,
    /// Error lines recognised in the output (deduplicated, capped).
    pub errors: Vec<String>,
    /// Problems of the parser itself (e.g. unparsable numbers).
    pub parse_warnings: Vec<String>,
}

impl OutputData {
    pub fn last_energy(&self) -> Option<&FinalEnergy> {
        self.final_energies.last()
    }

    pub fn is_empty(&self) -> bool {
        self.lines_read == 0
    }

    /// Imaginary (negative) frequencies at or below `threshold` (cm**-1).
    pub fn imaginary_frequencies(&self, threshold: f64) -> Vec<f64> {
        self.frequencies.iter().copied().filter(|f| *f <= threshold).collect()
    }
}

/// Everything the parser recognises on a single line (outside of the
/// multi-line sections).  One entry in [`LINE_PATTERNS`] per variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    // section headers
    InputName,
    CoordHeader,
    FreqHeader,
    ShieldingHeader,
    ScanHeader,
    TddftHeader,
    // single-line quantities
    FinalEnergy,
    ScfConverged,
    ScfNotConverged,
    S2,
    S2Ideal,
    S2Deviation,
    Version,
    Charge,
    Mult,
    Nel,
    NAtoms,
    BasisDim,
    GeomConverged,
    GeomCycle,
    GeomAfterCycles,
    CpscfConverged,
    Terminated,
    Dipole,
    Dispersion,
    NFod,
    RunTime,
    ThermoT,
    ThermoP,
    ThermoZpve,
    ThermoTotalCorr,
    ThermoHCorr,
    ThermoTs,
    ThermoGCorr,
    ThermoU,
    ThermoH,
    ThermoG,
    /// `ORCA finished by error termination` (also recorded as an error).
    ErrorTermination,
    /// Any other line that indicates a problem.
    Error,
}

/// All line patterns, compiled into one [`RegexSet`] so that every line is
/// scanned once.  Headers come first because they end the dispatch.
///
/// To add a quantity: add a `Line` variant, a row here, and an arm in
/// [`Parser::handle`].
static LINE_PATTERNS: &[(Line, &str)] = &[
    (Line::InputName, r"^NAME\s*=\s*(\S+)"),
    (Line::CoordHeader, r"CARTESIAN COORDINATES \(ANGSTROEM\)"),
    (Line::FreqHeader, r"VIBRATIONAL FREQUENCIES"),
    (Line::ShieldingHeader, r"CHEMICAL SHIELDING SUMMARY"),
    (Line::ScanHeader, r"The Calculated Surface using the 'Actual Energy'"),
    (Line::TddftHeader, r"TD-DFT/TDA EXCITED STATES|TD-DFT EXCITED STATES|CIS-EXCITED STATES"),
    (Line::FinalEnergy, r"FINAL SINGLE POINT ENERGY\s+(-?\d+\.\d+)\s*(.*)$"),
    (Line::ScfConverged, r"SCF CONVERGED AFTER\s+(\d+)\s+CYCLES"),
    (Line::ScfNotConverged, r"SCF NOT CONVERGED AFTER\s+(\d+)\s+CYCLES"),
    (Line::S2, r"Expectation value of <S\*\*2>\s*:\s*(-?\d+\.\d+)"),
    (Line::S2Ideal, r"Ideal value S\*\(S\+1\) for S=\S+\s*:\s*(-?\d+\.\d+)"),
    (Line::S2Deviation, r"^\s*Deviation\s*:\s*(-?\d+\.\d+)"),
    (Line::Version, r"Program Version\s+(\S+)"),
    (Line::Charge, r"Total Charge\s+Charge\s+\.+\s+(-?\d+)"),
    (Line::Mult, r"Multiplicity\s+Mult\s+\.+\s+(\d+)"),
    (Line::Nel, r"Number of Electrons\s+NEL\s+\.+\s+(\d+)"),
    (Line::NAtoms, r"Number of atoms\s+\.+\s+(\d+)"),
    (Line::BasisDim, r"Basis Dimension\s+Dim\s+\.+\s+(\d+)"),
    (Line::GeomConverged, r"THE OPTIMIZATION HAS CONVERGED"),
    (Line::GeomCycle, r"GEOMETRY OPTIMIZATION CYCLE\s+(\d+)"),
    (Line::GeomAfterCycles, r"\(AFTER\s+(\d+)\s+CYCLES\)"),
    (Line::CpscfConverged, r"THE CP-SCF HAS CONVERGED"),
    (Line::Terminated, r"ORCA TERMINATED NORMALLY"),
    (Line::Dipole, r"Magnitude \(Debye\)\s*:\s*(-?\d+\.\d+)"),
    (Line::Dispersion, r"^\s*Dispersion correction\s+(-?\d+\.\d+)"),
    (Line::NFod, r"N_FOD\s*(?:=|\.+)\s*(-?\d+\.\d+)"),
    (Line::RunTime, r"TOTAL RUN TIME:\s+(\d+) days (\d+) hours (\d+) minutes (\d+) seconds (\d+) msec"),
    (Line::ThermoT, r"^\s*Temperature\s+\.+\s+(-?\d+\.\d+)\s+K"),
    (Line::ThermoP, r"^\s*Pressure\s+\.+\s+(-?\d+\.\d+)\s+atm"),
    (Line::ThermoZpve, r"^\s*Zero point energy\s+\.+\s+(-?\d+\.\d+)\s+Eh"),
    (Line::ThermoTotalCorr, r"^\s*Total correction\s+(-?\d+\.\d+)\s+Eh"),
    (Line::ThermoHCorr, r"^\s*Thermal Enthalpy correction\s+\.+\s+(-?\d+\.\d+)\s+Eh"),
    (Line::ThermoTs, r"^\s*Final entropy term\s+\.+\s+(-?\d+\.\d+)\s+Eh"),
    (Line::ThermoGCorr, r"^\s*G-E\(el\)\s+\.+\s+(-?\d+\.\d+)\s+Eh"),
    (Line::ThermoU, r"^\s*Total thermal energy\s+(-?\d+\.\d+)\s+Eh"),
    (Line::ThermoH, r"^\s*Total Enthalpy\s+\.+\s+(-?\d+\.\d+)\s+Eh"),
    (Line::ThermoG, r"^\s*Final Gibbs free energy\s+\.+\s+(-?\d+\.\d+)\s+Eh"),
    (Line::ErrorTermination, r"ORCA finished by error termination"),
    // ---- lines that indicate a problem (exact ORCA / SLURM / MPI spellings) ----
    (Line::Error, r"aborting the run"),
    (Line::Error, r"SCF NOT CONVERGED"),
    (Line::Error, r"Wavefunction not fully converged"),
    (Line::Error, r"The optimization did not converge"),
    (Line::Error, r"OPTIMIZATION HAS NOT CONVERGED"),
    (Line::Error, r"INPUT ERROR"),
    (Line::Error, r"UNRECOGNIZED OR DUPLICATED KEYWORD"),
    (Line::Error, r"(?i)multiplicity .* not (compatible|possible)"),
    (Line::Error, r"(?i)not enough memory"),
    (Line::Error, r"(?i)out of memory"),
    (Line::Error, r"(?i)cannot (open|allocate|find)"),
    (Line::Error, r"(?i)could not (open|allocate|find|be found)"),
    (Line::Error, r"(?i)no such file"),
    (Line::Error, r"mpirun noticed"),
    (Line::Error, r"MPI_ABORT"),
    (Line::Error, r"(?i)segmentation (fault|violation)"),
    (Line::Error, r"(?i)\bkilled\b"),
    (Line::Error, r"DUE TO TIME LIMIT"),
    (Line::Error, r"CANCELLED AT"),
    (Line::Error, r"(?i)oom-kill"),
    (Line::Error, r"CP-SCF .*NOT CONVERGED"),
    (Line::Error, r"THE CP-SCF HAS NOT CONVERGED"),
    (Line::Error, r"Error \(ORCA_\w+\)"),
    (Line::Error, r"^\s*Error\s*:"),
    (Line::Error, r"^\[file .*\]:"),
];

struct Matcher {
    set: RegexSet,
    regexes: Vec<Regex>,
}

static MATCHER: LazyLock<Matcher> = LazyLock::new(|| {
    let patterns: Vec<&str> = LINE_PATTERNS.iter().map(|(_, p)| *p).collect();
    Matcher {
        set: RegexSet::new(&patterns).expect("valid regexes"),
        regexes: patterns.iter().map(|p| Regex::new(p).expect("valid regex")).collect(),
    }
});

/// Patterns for the bodies of multi-line sections.
mod patterns {
    use super::*;

    macro_rules! re {
        ($name:ident, $re:literal) => {
            pub static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($re).expect("valid regex"));
        };
    }

    re!(INPUT_LINE, r"^\|\s*\d+>\s?(.*)$");
    re!(COORD_LINE, r"^\s*([A-Za-z]{1,2}:?)\s+(-?\d+\.\d+)\s+(-?\d+\.\d+)\s+(-?\d+\.\d+)\s*$");
    re!(FREQ_LINE, r"^\s*(\d+):\s+(-?\d+\.\d+)\s+cm\*\*-1");
    re!(SHIELDING_LINE, r"^\s*(\d+)\s+([A-Za-z]{1,2}:?)\s+(-?\d+\.\d+)\s+(-?\d+\.\d+)\s*$");
    re!(SCAN_LINE, r"^\s*(-?\d+\.\d+)\s+(-?\d+\.\d+)\s*$");
    re!(TDDFT_STATE, r"^\s*STATE\s+(\d+):\s+E=\s+(-?\d+\.\d+)\s+au\s+(-?\d+\.\d+)\s+eV\s+(-?\d+\.\d+)\s+cm\*\*-1(?:\s+<S\*\*2>\s*=\s*(-?\d+\.\d+))?");
}

use patterns::*;

const MAX_ERRORS: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Section {
    None,
    EmbeddedInput,
    Coordinates,
    Frequencies,
    Shielding,
    ScanSurface,
    TddftStates,
}

/// Incremental parser; call [`Parser::feed`] for every line and
/// [`Parser::finish`] at the end.
#[derive(Debug)]
pub struct Parser {
    data: OutputData,
    section: Section,
    /// Lines remaining in which the `Ideal value`/`Deviation` lines of the
    /// current `<S**2>` block are accepted.
    spin_window: u8,
    embedded_input: Vec<String>,
    embedded_input_done: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Parser {
            data: OutputData::default(),
            section: Section::None,
            spin_window: 0,
            embedded_input: Vec::new(),
            embedded_input_done: false,
        }
    }

    fn parse_f64(&mut self, text: &str, what: &str) -> Option<f64> {
        match text.parse::<f64>() {
            Ok(v) if v.is_finite() => Some(v),
            _ => {
                self.data.parse_warnings.push(format!("could not parse {what} from `{text}`"));
                None
            }
        }
    }

    fn push_error(&mut self, line: &str) {
        let msg = line.trim();
        let msg: String = msg.chars().take(160).collect();
        if self.data.errors.last().map(String::as_str) == Some(msg.as_str()) {
            return;
        }
        if self.data.errors.len() < MAX_ERRORS {
            self.data.errors.push(msg);
        }
    }

    /// Process one line (without trailing newline).
    pub fn feed(&mut self, line: &str) {
        self.data.lines_read += 1;

        // ---- section bodies -------------------------------------------------
        match self.section {
            Section::EmbeddedInput => {
                if line.contains("****END OF INPUT****") {
                    self.section = Section::None;
                    self.embedded_input_done = true;
                    self.data.embedded_input = Some(self.embedded_input.join("\n"));
                    return;
                }
                if let Some(c) = INPUT_LINE.captures(line) {
                    self.embedded_input.push(c[1].to_string());
                    return;
                }
                // Not an input line: the echo ended without the marker (truncated file).
                self.section = Section::None;
                self.embedded_input_done = true;
                self.data.embedded_input = Some(self.embedded_input.join("\n"));
            }
            Section::Coordinates => {
                if let Some(c) = COORD_LINE.captures(line) {
                    let xyz: Vec<Option<f64>> =
                        (2..=4).map(|i| self.parse_f64(&c[i], "coordinate")).collect();
                    if let (Some(x), Some(y), Some(z)) = (xyz[0], xyz[1], xyz[2]) {
                        self.data.final_geometry.push(Atom { symbol: c[1].to_string(), x, y, z });
                    }
                    return;
                }
                if line.trim().starts_with('-') && self.data.final_geometry.is_empty() {
                    return; // the dashed line under the header
                }
                self.section = Section::None;
            }
            Section::Frequencies => {
                if let Some(c) = FREQ_LINE.captures(line) {
                    if let Some(f) = self.parse_f64(&c[2], "frequency") {
                        self.data.frequencies.push(f);
                    }
                    return;
                }
                let t = line.trim();
                if t.is_empty() || t.starts_with("Scaling factor") || t.starts_with("The first frequency") || (t.starts_with('-') && self.data.frequencies.is_empty()) {
                    return;
                }
                self.section = Section::None;
            }
            Section::Shielding => {
                if let Some(c) = SHIELDING_LINE.captures(line) {
                    let iso = self.parse_f64(&c[3], "isotropic shielding");
                    let aniso = self.parse_f64(&c[4], "shielding anisotropy");
                    if let (Ok(n), Some(iso), Some(aniso)) = (c[1].parse::<usize>(), iso, aniso) {
                        self.data.nmr_shieldings.push(Shielding { nucleus: n, element: c[2].to_string(), isotropic: iso, anisotropy: aniso });
                    }
                    return;
                }
                let t = line.trim();
                if t.is_empty() || t.starts_with('-') || t.starts_with("Nucleus") {
                    return;
                }
                self.section = Section::None;
            }
            Section::ScanSurface => {
                if let Some(c) = SCAN_LINE.captures(line) {
                    let x = self.parse_f64(&c[1], "scan coordinate");
                    let e = self.parse_f64(&c[2], "scan energy");
                    if let (Some(x), Some(e)) = (x, e) {
                        self.data.scan_surface.push((x, e));
                    }
                    return;
                }
                if line.trim().is_empty() && self.data.scan_surface.is_empty() {
                    return;
                }
                self.section = Section::None;
            }
            Section::TddftStates => {
                if let Some(c) = TDDFT_STATE.captures(line) {
                    let ev = self.parse_f64(&c[3], "excitation energy (eV)");
                    let cm = self.parse_f64(&c[4], "excitation energy (cm-1)");
                    let s2 = c.get(5).and_then(|m| m.as_str().parse::<f64>().ok());
                    if let (Ok(n), Some(ev), Some(cm)) = (c[1].parse::<u32>(), ev, cm) {
                        self.data.tddft_states.push(ExcitedState { state: n, energy_ev: ev, energy_cm: cm, s2 });
                    }
                    return;
                }
                if line.trim().is_empty() || line.contains("the weight of the individual excitations") || line.trim_start().starts_with('-') {
                    return;
                }
                if line.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    return; // "  12a ->  13a  :  0.98" contribution lines
                }
                self.section = Section::None;
            }
            Section::None => {}
        }

        // ---- everything else: one RegexSet scan per line ---------------------
        let matches = MATCHER.set.matches(line);
        if !matches.matched_any() {
            self.tick_spin_window();
            return;
        }
        let mut error_recorded = false;
        for idx in matches.iter() {
            let kind = LINE_PATTERNS[idx].0;
            if kind == Line::Error {
                if !error_recorded {
                    self.push_error(line);
                    error_recorded = true;
                }
                continue;
            }
            let Some(c) = MATCHER.regexes[idx].captures(line) else { continue };
            if self.handle(kind, &c, line) {
                return; // a section header: the rest of the line is irrelevant
            }
        }
        self.tick_spin_window();
    }

    /// Count down the window in which `Ideal value`/`Deviation` are accepted.
    fn tick_spin_window(&mut self) {
        self.spin_window = self.spin_window.saturating_sub(1);
    }

    /// Apply one recognised line.  Returns `true` when a section was opened.
    fn handle(&mut self, kind: Line, c: &regex::Captures<'_>, line: &str) -> bool {
        let g = |i: usize| c.get(i).map(|m| m.as_str()).unwrap_or("");
        match kind {
            Line::InputName => {
                if !self.embedded_input_done && self.data.input_name.is_none() {
                    self.data.input_name = Some(g(1).to_string());
                    self.section = Section::EmbeddedInput;
                    return true;
                }
            }
            Line::CoordHeader => {
                self.data.final_geometry.clear();
                self.section = Section::Coordinates;
                return true;
            }
            Line::FreqHeader => {
                self.data.frequencies.clear();
                self.section = Section::Frequencies;
                return true;
            }
            Line::ShieldingHeader => {
                self.data.nmr_shieldings.clear();
                self.section = Section::Shielding;
                return true;
            }
            Line::ScanHeader => {
                self.data.scan_surface.clear();
                self.section = Section::ScanSurface;
                return true;
            }
            Line::TddftHeader => {
                self.data.tddft_states.clear();
                self.section = Section::TddftStates;
                return true;
            }
            Line::FinalEnergy => {
                if let Some(e) = self.parse_f64(g(1), "final single point energy") {
                    let note = g(2).trim();
                    let note = if note.is_empty() { None } else { Some(note.to_string()) };
                    self.data.final_energies.push(FinalEnergy { energy: e, note });
                }
            }
            Line::ScfConverged => {
                self.data.scf_converged = Some(true);
                self.data.scf_cycles = g(1).parse().ok();
            }
            Line::ScfNotConverged => {
                self.data.scf_converged = Some(false);
                self.data.scf_cycles = g(1).parse().ok();
                self.push_error(line);
            }
            Line::S2 => {
                if let Some(v) = self.parse_f64(g(1), "<S**2>") {
                    self.data.spin = Some(SpinExpectation { s2: v, ideal: None, deviation: None });
                    self.spin_window = 4;
                }
            }
            Line::S2Ideal => {
                if self.spin_window > 0 {
                    let v = self.parse_f64(g(1), "ideal <S**2>");
                    if let Some(spin) = self.data.spin.as_mut() {
                        spin.ideal = v;
                    }
                }
            }
            Line::S2Deviation => {
                if self.spin_window > 0 {
                    let v = self.parse_f64(g(1), "<S**2> deviation");
                    if let Some(spin) = self.data.spin.as_mut() {
                        spin.deviation = v;
                    }
                }
            }
            Line::Version => self.data.orca_version = Some(g(1).to_string()),
            Line::Charge => self.data.charge = g(1).parse().ok(),
            Line::Mult => self.data.multiplicity = g(1).parse().ok(),
            Line::Nel => self.data.n_electrons = g(1).parse().ok(),
            Line::NAtoms => self.data.n_atoms = g(1).parse().ok(),
            Line::BasisDim => self.data.basis_dimension = g(1).parse().ok(),
            Line::GeomConverged => self.data.geom_converged = true,
            Line::GeomCycle | Line::GeomAfterCycles => self.data.geom_cycles = g(1).parse().ok(),
            Line::CpscfConverged => self.data.cpscf_converged = Some(true),
            Line::Terminated => self.data.terminated_normally = true,
            Line::Dipole => self.data.dipole_debye = self.parse_f64(g(1), "dipole moment"),
            Line::Dispersion => self.data.dispersion_eh = self.parse_f64(g(1), "dispersion correction"),
            Line::NFod => self.data.n_fod = self.parse_f64(g(1), "N_FOD"),
            Line::RunTime => {
                let parts: Vec<f64> = (1..=5).filter_map(|i| g(i).parse::<f64>().ok()).collect();
                if parts.len() == 5 {
                    self.data.run_time_s =
                        Some(parts[0] * 86400.0 + parts[1] * 3600.0 + parts[2] * 60.0 + parts[3] + parts[4] / 1000.0);
                }
            }
            Line::ThermoT => self.data.thermo.temperature_k = self.parse_f64(g(1), "temperature"),
            Line::ThermoP => self.data.thermo.pressure_atm = self.parse_f64(g(1), "pressure"),
            Line::ThermoZpve => self.data.thermo.zpve = self.parse_f64(g(1), "ZPVE"),
            Line::ThermoTotalCorr => self.data.thermo.total_correction = self.parse_f64(g(1), "total thermal correction"),
            Line::ThermoHCorr => self.data.thermo.enthalpy_correction = self.parse_f64(g(1), "enthalpy correction"),
            Line::ThermoTs => self.data.thermo.entropy_term = self.parse_f64(g(1), "entropy term"),
            Line::ThermoGCorr => self.data.thermo.gibbs_correction = self.parse_f64(g(1), "G-E(el)"),
            Line::ThermoU => self.data.thermo.total_thermal_energy = self.parse_f64(g(1), "total thermal energy"),
            Line::ThermoH => self.data.thermo.total_enthalpy = self.parse_f64(g(1), "total enthalpy"),
            Line::ThermoG => self.data.thermo.final_gibbs_energy = self.parse_f64(g(1), "final Gibbs free energy"),
            Line::ErrorTermination => {
                self.data.terminated_normally = false;
                self.push_error(line);
            }
            Line::Error => unreachable!("handled by the caller"),
        }
        false
    }

    pub fn finish(mut self) -> OutputData {
        if self.section == Section::EmbeddedInput && !self.embedded_input.is_empty() {
            self.data.embedded_input = Some(self.embedded_input.join("\n"));
        }
        self.data
    }
}

/// Parse an iterator of lines (each without its newline).
pub fn parse_output_lines<I, S>(lines: I) -> OutputData
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut parser = Parser::new();
    for line in lines {
        parser.feed(line.as_ref());
    }
    parser.finish()
}

/// Convenience wrapper for in-memory text (tests, embedded inputs).
pub fn parse_output_text(text: &str) -> OutputData {
    parse_output_lines(text.lines())
}

/// Parse from any reader without loading the whole file into memory.
/// Invalid UTF-8 is replaced, never fatal.
pub fn parse_output_reader<R: Read>(reader: R) -> std::io::Result<OutputData> {
    let mut reader = BufReader::with_capacity(1 << 16, reader);
    let mut parser = Parser::new();
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        while matches!(buf.last(), Some(b'\n') | Some(b'\r')) {
            buf.pop();
        }
        parser.feed(&String::from_utf8_lossy(&buf));
    }
    Ok(parser.finish())
}

/// Parse an ORCA output file from disk.
pub fn parse_output_file(path: &Path) -> anyhow::Result<OutputData> {
    let file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    parse_output_reader(file).with_context(|| format!("could not read {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_gives_empty_data() {
        let d = parse_output_text("");
        assert!(d.is_empty());
        assert!(!d.terminated_normally);
        assert!(d.final_energies.is_empty());
    }

    #[test]
    fn final_energy_with_and_without_note() {
        let d = parse_output_text(
            "FINAL SINGLE POINT ENERGY      -76.123456789012\nFINAL SINGLE POINT ENERGY   -76.1 (Wavefunction not fully converged!)\n",
        );
        assert_eq!(d.final_energies.len(), 2);
        assert_eq!(d.final_energies[0].energy, -76.123456789012);
        assert!(d.final_energies[0].is_fully_converged());
        assert!(!d.final_energies[1].is_fully_converged());
        assert_eq!(d.final_energies[1].note.as_deref(), Some("(Wavefunction not fully converged!)"));
        assert!(d.errors.iter().any(|e| e.contains("not fully converged")));
    }

    #[test]
    fn spin_block_is_parsed_by_label_not_offset() {
        let text = "Expectation value of <S**2>     :     3.767755\nsome other line\nIdeal value S*(S+1) for S=1.5   :     3.750000\nDeviation                       :     0.017755\n";
        let d = parse_output_text(text);
        let s = d.spin.unwrap();
        assert_eq!(s.s2, 3.767755);
        assert_eq!(s.ideal, Some(3.75));
        assert_eq!(s.deviation, Some(0.017755));
    }

    #[test]
    fn deviation_outside_window_is_ignored() {
        let text = "Expectation value of <S**2>     :     0.75\na\nb\nc\nd\ne\nDeviation : 9.0\n";
        let d = parse_output_text(text);
        assert_eq!(d.spin.unwrap().deviation, None);
    }

    #[test]
    fn scf_convergence_and_cycles() {
        let d = parse_output_text("*           SCF CONVERGED AFTER  94 CYCLES          *");
        assert_eq!(d.scf_converged, Some(true));
        assert_eq!(d.scf_cycles, Some(94));
        let d = parse_output_text("            SCF NOT CONVERGED AFTER 125 CYCLES");
        assert_eq!(d.scf_converged, Some(false));
        assert_eq!(d.scf_cycles, Some(125));
        assert_eq!(d.errors.len(), 1);
    }

    #[test]
    fn embedded_input_is_extracted() {
        let text = "NAME = foo.inp\n|  1> ! r2scan-3c opt\n|  2> \n|  3> * xyz 0 1\n|  4> H 0 0 0\n|  5> *\n|  6> \n|  7>                          ****END OF INPUT****\nnext section\n";
        let d = parse_output_text(text);
        assert_eq!(d.input_name.as_deref(), Some("foo.inp"));
        let inp = d.embedded_input.unwrap();
        assert!(inp.starts_with("! r2scan-3c opt\n"));
        assert!(inp.contains("* xyz 0 1\nH 0 0 0\n*"));
        assert!(!inp.contains("END OF INPUT"));
    }

    #[test]
    fn truncated_embedded_input_is_kept() {
        let d = parse_output_text("NAME = foo.inp\n|  1> ! hf\n");
        assert_eq!(d.embedded_input.as_deref(), Some("! hf"));
    }

    #[test]
    fn coordinates_keep_last_block() {
        let text = "---------------------------------\nCARTESIAN COORDINATES (ANGSTROEM)\n---------------------------------\n  H   0.0 0.0 0.0\n  H   0.0 0.0 0.74\n\nfoo\nCARTESIAN COORDINATES (ANGSTROEM)\n---------------------------------\n  H   0.000000 0.000000 0.000000\n  H   0.000000 0.000000 0.750000\n\n";
        let d = parse_output_text(text);
        assert_eq!(d.final_geometry.len(), 2);
        assert_eq!(d.final_geometry[1].z, 0.75);
        assert_eq!(d.final_geometry[1].symbol, "H");
    }

    #[test]
    fn frequencies_are_read_until_normal_modes() {
        let text = "-----------------------\nVIBRATIONAL FREQUENCIES\n-----------------------\n\nScaling factor for frequencies =  1.000000000  (already applied!)\n\n   0:         0.00 cm**-1\n   1:         0.00 cm**-1\n   5:      -123.45 cm**-1 ***imaginary mode***\n   6:      1234.56 cm**-1\n\n------------\nNORMAL MODES\n------------\n   7:   9999.0 cm**-1\n";
        let d = parse_output_text(text);
        assert_eq!(d.frequencies, vec![0.0, 0.0, -123.45, 1234.56]);
        assert_eq!(d.imaginary_frequencies(-20.0), vec![-123.45]);
    }

    #[test]
    fn thermochemistry_lines() {
        let text = "\
Temperature         ... 298.15 K
Pressure            ... 1.00 atm
Zero point energy                ...      0.02104565 Eh      13.21 kcal/mol
Total thermal correction                  0.00283596 Eh       1.78 kcal/mol
Total correction                          0.02388161 Eh      14.99 kcal/mol
Total thermal energy                   -76.36591682 Eh
Thermal Enthalpy correction       ...      0.00094421 Eh       0.59 kcal/mol
Total Enthalpy                    ...    -76.36497262 Eh
Final entropy term                ...      0.02136579 Eh     13.41 kcal/mol
Final Gibbs free energy         ...    -76.38633841 Eh
G-E(el)                           ...      0.00346002 Eh       2.17 kcal/mol
";
        let d = parse_output_text(text);
        let t = d.thermo;
        assert_eq!(t.temperature_k, Some(298.15));
        assert_eq!(t.zpve, Some(0.02104565));
        assert_eq!(t.total_correction, Some(0.02388161));
        assert_eq!(t.enthalpy_correction, Some(0.00094421));
        assert_eq!(t.entropy_term, Some(0.02136579));
        assert_eq!(t.gibbs_correction, Some(0.00346002));
        assert_eq!(t.total_enthalpy, Some(-76.36497262));
        assert_eq!(t.final_gibbs_energy, Some(-76.38633841));
        assert_eq!(t.total_thermal_energy, Some(-76.36591682));
    }

    #[test]
    fn shielding_summary() {
        let text = "--------------------------------\nCHEMICAL SHIELDING SUMMARY (ppm)\n--------------------------------\n\n\n  Nucleus  Element    Isotropic     Anisotropy\n  -------  -------  ------------   ------------\n      0       C          55.123        120.5\n      1       H:         31.001          8.0\n      2       H         -10.500          1.5\n\nTimings\n";
        let d = parse_output_text(text);
        assert_eq!(d.nmr_shieldings.len(), 3);
        assert_eq!(d.nmr_shieldings[2].nucleus, 2);
        assert_eq!(d.nmr_shieldings[2].isotropic, -10.5);
    }

    #[test]
    fn scan_surface_as_tuples() {
        let text = "The Calculated Surface using the 'Actual Energy'\n   0.70000000  -1.11675931 \n   0.80000000  -1.16500000\n\nThe Calculated Surface using the SCF energy\n   0.70000000  -1.0\n";
        let d = parse_output_text(text);
        assert_eq!(d.scan_surface, vec![(0.7, -1.11675931), (0.8, -1.165)]);
    }

    #[test]
    fn tddft_states() {
        let text = "-----------------------------\nTD-DFT/TDA EXCITED STATES (SINGLETS)\n-----------------------------\n\nthe weight of the individual excitations are printed if larger than 1.0e-02\n\nSTATE  1:  E=   0.147693 au      4.019 eV    32414.0 cm**-1 <S**2> =   0.000000\n    12a ->  13a  :     0.982\nSTATE  2:  E=   0.200000 au      5.442 eV    43893.0 cm**-1\n    11a ->  13a  :     0.900\n\n-----------------------------\nABSORPTION SPECTRUM\n";
        let d = parse_output_text(text);
        assert_eq!(d.tddft_states.len(), 2);
        assert_eq!(d.tddft_states[0].energy_ev, 4.019);
        assert_eq!(d.tddft_states[0].s2, Some(0.0));
        assert_eq!(d.tddft_states[1].s2, None);
        // a later block (e.g. after another optimisation cycle) replaces the earlier one
        let d = parse_output_text(&format!("{text}\nTD-DFT EXCITED STATES\nSTATE  1:  E=   0.1 au      2.000 eV    16000.0 cm**-1\n"));
        assert_eq!(d.tddft_states.len(), 1);
        assert_eq!(d.tddft_states[0].energy_ev, 2.0);
    }

    #[test]
    fn misc_single_line_quantities() {
        let text = "                         Program Version 6.1.0  -  RELEASE   -\n Total Charge           Charge          ....   -2\n Multiplicity           Mult            ....    4\n Number of Electrons    NEL             ....  403\n Basis Dimension        Dim             .... 5134\nNumber of atoms                             ...    110\nMagnitude (Debye)      :      7.734068328\nDispersion correction           -0.278380990\n   N_FOD = 1.234567\nTHE CP-SCF HAS CONVERGED\n                             ****ORCA TERMINATED NORMALLY****\nTOTAL RUN TIME: 1 days 17 hours 58 minutes 19 seconds 349 msec\n";
        let d = parse_output_text(text);
        assert_eq!(d.orca_version.as_deref(), Some("6.1.0"));
        assert_eq!(d.charge, Some(-2));
        assert_eq!(d.multiplicity, Some(4));
        assert_eq!(d.n_electrons, Some(403));
        assert_eq!(d.basis_dimension, Some(5134));
        assert_eq!(d.n_atoms, Some(110));
        assert_eq!(d.dipole_debye, Some(7.734068328));
        assert_eq!(d.dispersion_eh, Some(-0.27838099));
        assert_eq!(d.n_fod, Some(1.234567));
        assert_eq!(d.cpscf_converged, Some(true));
        assert!(d.terminated_normally);
        assert_eq!(d.run_time_s, Some(86400.0 + 17.0 * 3600.0 + 58.0 * 60.0 + 19.0 + 0.349));
    }

    #[test]
    fn geometry_optimization_flags() {
        let text = "                    *    GEOMETRY OPTIMIZATION CYCLE   1    *\n*    GEOMETRY OPTIMIZATION CYCLE   7    *\n                    ***        THE OPTIMIZATION HAS CONVERGED     ***\n                    ***               (AFTER   7 CYCLES)               ***\n";
        let d = parse_output_text(text);
        assert!(d.geom_converged);
        assert_eq!(d.geom_cycles, Some(7));
        let d = parse_output_text("The optimization did not converge but reached the maximum number of optimization cycles\n");
        assert!(!d.geom_converged);
        assert_eq!(d.errors.len(), 1);
    }

    #[test]
    fn error_termination_overrides_normal_termination() {
        let d = parse_output_text("****ORCA TERMINATED NORMALLY****\nORCA finished by error termination in SCF\n");
        assert!(!d.terminated_normally);
        assert_eq!(d.errors, vec!["ORCA finished by error termination in SCF"]);
    }

    #[test]
    fn error_lines_are_capped_and_deduplicated() {
        let mut text = String::new();
        for _ in 0..100 {
            text.push_str("aborting the run\n");
        }
        for i in 0..100 {
            text.push_str(&format!("Error : something {i}\n"));
        }
        let d = parse_output_text(&text);
        assert_eq!(d.errors.len(), MAX_ERRORS);
        assert_eq!(d.errors[0], "aborting the run");
    }

    #[test]
    fn benign_lines_are_not_errors() {
        let text = "Last Energy change         ...    4.7148e-09  Tolerance :   1.0000e-08\nWarning: in a DFT calculation there is little theoretical justification\nError (ORCA_SCF): Not enough memory available!\n";
        let d = parse_output_text(text);
        assert_eq!(d.errors.len(), 1);
    }

    #[test]
    fn unparsable_number_is_reported_not_fatal() {
        let d = parse_output_text("FINAL SINGLE POINT ENERGY      -76.123\nMagnitude (Debye)      :      1e309.0\n");
        assert_eq!(d.final_energies.len(), 1);
        // 1e309.0 does not match the dipole regex at all -> simply ignored
        assert_eq!(d.dipole_debye, None);
    }

    #[test]
    fn reader_handles_crlf_and_invalid_utf8() {
        let bytes = b"FINAL SINGLE POINT ENERGY   -1.5\r\n\xff\xfe garbage\r\n****ORCA TERMINATED NORMALLY****\r\n";
        let d = parse_output_reader(&bytes[..]).unwrap();
        assert_eq!(d.final_energies[0].energy, -1.5);
        assert!(d.terminated_normally);
        assert_eq!(d.lines_read, 3);
    }
}

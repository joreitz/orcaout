//! Turns parsed input + output into a [`JobReport`]: a status (OK, WARNING,
//! ERROR, RUNNING, …), human readable messages and the derived values that
//! the CSV columns read.  Everything here is a pure function of its
//! arguments so it can be unit tested without files.

use std::fmt;

use crate::input::{InputInfo, JobType};
use crate::output::OutputData;

/// Boltzmann constant in Eh/K (CODATA 2018).
pub const K_B_EH: f64 = 3.166_811_563e-6;

/// Tunables that come from the command line.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisOptions {
    /// `-ncf`: accept "not fully converged" wave functions into the CSV.
    pub accept_not_fully_converged: bool,
    /// `-npolf` negated: warn when `<S**2>` deviates by more than `spin_deviation_max`.
    pub warn_spin_contamination: bool,
    /// Frequencies at or below this value (cm**-1) count as imaginary.
    pub imaginary_threshold: f64,
    /// Threshold for the spin contamination warning.
    pub spin_deviation_max: f64,
    /// `--all`: failed jobs are written to the CSV as well.
    pub include_failed: bool,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        AnalysisOptions {
            accept_not_fully_converged: false,
            warn_spin_contamination: true,
            imaginary_threshold: -20.0,
            spin_deviation_max: 0.10,
            include_failed: false,
        }
    }
}

/// What was found on disk / in the queue for one job.
#[derive(Debug, Clone, PartialEq)]
pub enum Observation {
    /// No `.out` file.
    Missing,
    /// `.out` exists but is (nearly) empty.
    Empty,
    /// `.out` could not be read (permission, I/O error, …).
    Unreadable(String),
    Parsed(Box<OutputData>),
}

/// Queue information about the job (SLURM).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum QueueState {
    #[default]
    Unknown,
    Pending,
    Running {
        job_id: String,
        workdir: String,
    },
    /// `squeue` could not be queried, the job may or may not be in the queue.
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Ok,
    Warning,
    Pending,
    Running,
    NotSubmitted,
    Error,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Warning => "WARNING",
            Status::Pending => "PENDING",
            Status::Running => "RUNNING",
            Status::NotSubmitted => "MISSING",
            Status::Error => "ERROR",
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// SCF convergence as reported in the `SCFCONV` column (same strings as the
/// Python original).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScfConvergence {
    Converged,
    NotFully,
    Failed,
}

impl ScfConvergence {
    pub fn as_str(self) -> &'static str {
        match self {
            ScfConvergence::Converged => "True",
            ScfConvergence::NotFully => "notfully",
            ScfConvergence::Failed => "False",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpinCheck {
    pub s2: f64,
    pub ideal: Option<f64>,
    pub deviation: Option<f64>,
    pub significant: bool,
}

/// Thermochemical corrections relative to the electronic energy (Eh).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermoResult {
    pub zpve: f64,
    pub du: f64,
    pub dh: f64,
    pub tds: f64,
    pub dg: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Character {
    Minimum,
    TransitionState,
    Fail,
}

impl Character {
    pub fn as_str(self) -> &'static str {
        match self {
            Character::Minimum => "MIN",
            Character::TransitionState => "TS",
            Character::Fail => "FAIL",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobReport {
    /// File stem (`foo` for `foo.inp` / `foo.out`).
    pub name: String,
    pub job_type: JobType,
    pub status: Status,
    pub messages: Vec<String>,
    /// Whether the row is written to the CSV.
    pub in_csv: bool,
    pub input: InputInfo,
    pub output: Option<OutputData>,
    pub queue: QueueState,
    // ---- derived values -----------------------------------------------------
    pub energy: Option<f64>,
    pub charge: Option<i32>,
    pub multiplicity: Option<u32>,
    pub scf: Option<ScfConvergence>,
    pub spin: Option<SpinCheck>,
    pub thermo: Option<ThermoResult>,
    pub character: Option<Character>,
    pub imaginary: Vec<f64>,
    /// `(ghost atom index, isotropic shielding)` for NICS jobs.
    pub nics: Vec<(usize, f64)>,
}

impl JobReport {
    fn new(name: &str, input: InputInfo, queue: QueueState) -> Self {
        JobReport {
            name: name.to_string(),
            job_type: input.job_type,
            status: Status::Ok,
            messages: Vec::new(),
            in_csv: false,
            charge: input.charge,
            multiplicity: input.multiplicity,
            input,
            output: None,
            queue,
            energy: None,
            scf: None,
            spin: None,
            thermo: None,
            character: None,
            imaginary: Vec::new(),
            nics: Vec::new(),
        }
    }

    fn raise(&mut self, status: Status, msg: impl Into<String>) {
        self.messages.push(msg.into());
        if status > self.status {
            self.status = status;
        }
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.status, Status::Error | Status::NotSubmitted | Status::Pending | Status::Running)
    }
}

/// Determine the `SCFCONV` value from the output.
pub fn scf_convergence(out: &OutputData) -> ScfConvergence {
    match out.last_energy() {
        None => ScfConvergence::Failed,
        Some(e) if !e.is_fully_converged() => ScfConvergence::NotFully,
        Some(_) if out.scf_converged == Some(false) => ScfConvergence::Failed,
        Some(_) => ScfConvergence::Converged,
    }
}

/// Convergence of the response solver(s) that ran, for the `CPSCFCONV` column.
///
/// * CP-SCF (analytic frequencies, NMR, TD-DFT gradients): `THE CP-SCF HAS CONVERGED`.
/// * TD-DFT/CIS excitation energies come from a Davidson diagonalisation, not
///   from a CP-SCF; for TD-DFT jobs its convergence is reported here as well.
///
/// `Some(false)` if any solver that ran did not converge, `Some(true)` if all
/// that ran converged, `None` if no response solver ran at all (e.g. `NumFreq`).
pub fn response_convergence(out: &OutputData, job: JobType) -> Option<bool> {
    let davidson = if job == JobType::Tddft { out.tddft_converged } else { None };
    match (out.cpscf_converged, davidson) {
        (None, None) => None,
        (a, b) => Some(a != Some(false) && b != Some(false)),
    }
}

/// Compute dU, dH, TdS, dG from the thermochemistry block.
///
/// dH = dU + kT (ORCA prints this as `Thermal Enthalpy correction`; if that
/// line is missing kT is computed at the printed temperature, 298.15 K by
/// default — the Python original hard-coded 0.00094421 Eh).
pub fn thermo_result(out: &OutputData) -> Option<ThermoResult> {
    let t = &out.thermo;
    let zpve = t.zpve?;
    let du = t.total_correction?;
    let tds = t.entropy_term?;
    let dg = t.gibbs_correction?;
    let kt = t
        .enthalpy_correction
        .unwrap_or_else(|| K_B_EH * t.temperature_k.unwrap_or(298.15));
    Some(ThermoResult { zpve, du, dh: du + kt, tds, dg })
}

/// Classify a stationary point by the number of imaginary modes.
pub fn character(num_imaginary: usize) -> Character {
    match num_imaginary {
        0 => Character::Minimum,
        1 => Character::TransitionState,
        _ => Character::Fail,
    }
}

fn fmt_list(values: &[f64]) -> String {
    values.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join("|")
}

/// Evaluate one job.
pub fn evaluate(
    name: &str,
    input: InputInfo,
    observation: Observation,
    queue: QueueState,
    opts: &AnalysisOptions,
) -> JobReport {
    let mut r = JobReport::new(name, input, queue.clone());

    if r.job_type == JobType::Invalid {
        r.raise(Status::Error, "could not find a `!` keyword line in the input");
    }

    let out = match observation {
        Observation::Missing => {
            match &queue {
                QueueState::Pending => r.raise(Status::Pending, "no .out file yet, job is pending in the queue"),
                QueueState::Running { job_id, .. } => {
                    r.raise(Status::Running, format!("no .out file yet, job {job_id} is running"))
                }
                QueueState::Unknown => r.raise(Status::NotSubmitted, "no .out file, job not submitted?"),
                QueueState::Unavailable => {
                    r.raise(Status::NotSubmitted, "no .out file; queue status unknown (squeue failed), job may be pending")
                }
            }
            return r;
        }
        Observation::Empty => {
            match &queue {
                QueueState::Running { job_id, workdir } => {
                    r.raise(Status::Running, format!("job {job_id} is running (WorkDir: {workdir})"))
                }
                QueueState::Unavailable => r.raise(
                    Status::Error,
                    ".out file is empty; queue status unknown (squeue failed), job may still be running",
                ),
                _ => r.raise(Status::Error, ".out file is empty or contains no ORCA output"),
            }
            return r;
        }
        Observation::Unreadable(why) => {
            r.raise(Status::Error, format!("could not read .out file: {why}"));
            return r;
        }
        Observation::Parsed(out) => *out,
    };

    // ---- consistency between .inp and .out ---------------------------------
    if let Some(inp_name) = &out.input_name {
        let stem = inp_name.strip_suffix(".inp").unwrap_or(inp_name);
        if stem != name {
            r.raise(Status::Warning, format!(".out was produced from `{inp_name}`, not from `{name}.inp`"));
        }
    }
    if r.charge.is_none() {
        r.charge = out.charge;
    } else if out.charge.is_some() && out.charge != r.charge {
        r.raise(Status::Warning, format!("charge in .inp ({:?}) differs from .out ({:?})", r.charge, out.charge));
    }
    if r.multiplicity.is_none() {
        r.multiplicity = out.multiplicity;
    } else if out.multiplicity.is_some() && out.multiplicity != r.multiplicity {
        r.raise(Status::Warning, format!("multiplicity in .inp ({:?}) differs from .out ({:?})", r.multiplicity, out.multiplicity));
    }
    for w in &out.parse_warnings {
        r.raise(Status::Warning, format!("parser: {w}"));
    }

    // ---- termination / SCF ------------------------------------------------------
    let scf = scf_convergence(&out);
    r.scf = Some(scf);
    r.energy = out.last_energy().map(|e| e.energy);

    let running_here = matches!(&queue, QueueState::Running { .. });
    if !out.terminated_normally {
        if running_here {
            let (job_id, workdir) = match &queue {
                QueueState::Running { job_id, workdir } => (job_id.as_str(), workdir.as_str()),
                _ => unreachable!(),
            };
            let mut msg = format!("job {job_id} is running");
            if let Some(c) = out.geom_cycles {
                msg.push_str(&format!(", geometry cycle {c}"));
            }
            if let Some(c) = out.scf_cycles {
                msg.push_str(&format!(", last SCF: {c} cycles"));
            }
            msg.push_str(&format!(" (WorkDir: {workdir})"));
            r.raise(Status::Running, msg);
            return r;
        }
        // "not fully converged" is decided by -ncf below, everything else is fatal.
        let other_errors: Vec<&String> = out.errors.iter().filter(|e| !e.contains("not fully converged")).collect();
        if !other_errors.is_empty() {
            for e in other_errors {
                r.raise(Status::Error, e.clone());
            }
        } else if queue == QueueState::Unavailable {
            r.raise(
                Status::Error,
                "ORCA did not terminate normally; queue status unknown (squeue failed), job may still be running",
            );
        } else if scf != ScfConvergence::NotFully {
            r.raise(Status::Error, "ORCA did not terminate normally (killed, time limit or still running elsewhere?)");
        }
    }

    match scf {
        ScfConvergence::Failed => {
            if out.scf_converged == Some(false) {
                let cycles = out.scf_cycles.map(|c| format!(" after {c} cycles")).unwrap_or_default();
                r.raise(Status::Error, format!("SCF did not converge{cycles}"));
            } else if out.scf_converged.is_none() && out.final_energies.is_empty() {
                r.raise(Status::Error, "no SCF energy found in the output");
            } else {
                r.raise(Status::Error, "no final single point energy found");
            }
        }
        ScfConvergence::NotFully => {
            if opts.accept_not_fully_converged {
                r.raise(Status::Warning, "wave function not fully converged (accepted because of -ncf)");
            } else {
                r.raise(Status::Error, "wave function not fully converged (use -ncf to accept)");
            }
        }
        ScfConvergence::Converged => {}
    }

    // ---- spin contamination ------------------------------------------------------
    if let Some(spin) = out.spin {
        let significant = spin.deviation.is_some_and(|d| d >= opts.spin_deviation_max);
        r.spin = Some(SpinCheck { s2: spin.s2, ideal: spin.ideal, deviation: spin.deviation, significant });
        if significant && opts.warn_spin_contamination {
            r.raise(
                Status::Warning,
                format!(
                    "significant (>= {}) spin contamination: <S^2>_ideal: {:.3}, <S^2>_scf: {:.3}, dS^2: {:.3}",
                    opts.spin_deviation_max,
                    spin.ideal.unwrap_or(f64::NAN),
                    spin.s2,
                    spin.deviation.unwrap_or(f64::NAN)
                ),
            );
        }
    } else if r.multiplicity.is_some_and(|m| m > 1) && r.job_type != JobType::Ftdft && scf != ScfConvergence::Failed {
        r.raise(Status::Warning, "could not extract <S^2> although multiplicity > 1 (ROKS?), check output manually");
    }

    // ---- job specific checks -----------------------------------------------------
    let jt = r.job_type;
    if jt.is_optimization() && jt != JobType::Scan {
        if !out.geom_converged {
            let cycles = out.geom_cycles.map(|c| format!(" (after {c} cycles)")).unwrap_or_default();
            r.raise(Status::Error, format!("geometry optimization NOT converged{cycles}"));
        } else if out.geom_cycles.is_none() {
            r.raise(Status::Warning, "optimization converged but the cycle count could not be read");
        }
    }
    if jt == JobType::Scan && out.scan_surface.is_empty() {
        r.raise(Status::Error, "no relaxed surface scan energies found");
    }
    if jt.has_frequencies() {
        if out.frequencies.is_empty() {
            r.raise(Status::Error, "could not extract frequencies, check output manually");
        } else {
            r.imaginary = out.imaginary_frequencies(opts.imaginary_threshold);
            r.character = Some(character(r.imaginary.len()));
        }
        match thermo_result(&out) {
            Some(t) => r.thermo = Some(t),
            None => r.raise(Status::Warning, "thermochemistry block incomplete"),
        }
    }
    if jt == JobType::Nmr {
        if out.nmr_shieldings.is_empty() {
            r.raise(Status::Error, "no chemical shielding summary found");
        } else {
            let ghosts = &r.input.ghost_ids;
            r.nics = out
                .nmr_shieldings
                .iter()
                .filter(|s| ghosts.contains(&s.nucleus))
                .map(|s| (s.nucleus, s.isotropic))
                .collect();
            if ghosts.is_empty() {
                r.raise(Status::Warning, "no ghost atoms in the input, NICS column stays empty");
            } else if r.nics.len() != ghosts.len() {
                r.raise(Status::Warning, format!("shieldings found for {} of {} ghost atoms", r.nics.len(), ghosts.len()));
            }
        }
    }
    if jt == JobType::Ftdft && out.n_fod.is_none() {
        r.raise(Status::Warning, "no N_FOD value found");
    }
    if jt == JobType::Tddft && out.tddft_states.is_empty() {
        r.raise(Status::Error, "no excited states found");
    }
    // Response solvers are only judged when they actually ran.
    if out.cpscf_converged == Some(false) {
        r.raise(Status::Error, "CP-SCF was started but did not converge");
    }
    if out.tddft_converged == Some(false) {
        r.raise(Status::Error, "TD-DFT/CIS Davidson diagonalisation was started but did not converge");
    }

    // ---- summary line for successful jobs ----------------------------------------
    if r.status <= Status::Warning {
        let mut parts: Vec<String> = Vec::new();
        if let Some(s) = r.spin {
            parts.push(format!("<S^2>: {:.3}", s.s2));
        }
        if let Some(n) = out.n_fod {
            parts.push(format!("N_FOD: {n:.3}"));
        }
        if let Some(c) = out.geom_cycles.filter(|_| jt.is_optimization()) {
            parts.push(format!("#geom_cycles: {c:>3}"));
        }
        if let Some(c) = r.character {
            parts.push(format!("CHAR: {:>4}", c.as_str()));
            if !r.imaginary.is_empty() {
                parts.push(format!("IMAGS: {}", fmt_list(&r.imaginary)));
            }
        }
        if jt == JobType::Nmr {
            parts.push(format!("#ghost atoms: {}", r.input.num_ghost_atoms()));
        }
        if jt == JobType::Scan {
            parts.push(format!("#scan steps: {}", out.scan_surface.len()));
        }
        if jt == JobType::Tddft {
            parts.push(format!("#states: {}", out.tddft_states.len()));
        }
        if !parts.is_empty() {
            r.messages.insert(0, parts.join(", "));
        }
    }

    r.in_csv = r.status <= Status::Warning || opts.include_failed;
    r.output = Some(out);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::parse_input_text;
    use crate::output::parse_output_text;

    fn sp_input() -> InputInfo {
        parse_input_text("! uks wb97x-d4rev def2-qzvppd\n*xyz -2 4\nCo 0 0 0\n*")
    }

    const GOOD_SP: &str = "FINAL SINGLE POINT ENERGY   -4128.719797087585\nExpectation value of <S**2>     :     3.767755\nIdeal value S*(S+1) for S=1.5   :     3.750000\nDeviation                       :     0.017755\n****ORCA TERMINATED NORMALLY****\n";

    fn parsed(text: &str) -> Observation {
        Observation::Parsed(Box::new(parse_output_text(text)))
    }

    #[test]
    fn good_sp_is_ok() {
        let r = evaluate("x", sp_input(), parsed(GOOD_SP), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Ok);
        assert!(r.in_csv);
        assert_eq!(r.energy, Some(-4128.719797087585));
        assert_eq!(r.scf, Some(ScfConvergence::Converged));
        assert_eq!(r.spin.unwrap().s2, 3.767755);
        assert!(!r.spin.unwrap().significant);
        assert_eq!(r.messages[0], "<S^2>: 3.768");
    }

    #[test]
    fn missing_out_depends_on_queue() {
        let r = evaluate("x", sp_input(), Observation::Missing, QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::NotSubmitted);
        assert!(!r.in_csv);
        let r = evaluate("x", sp_input(), Observation::Missing, QueueState::Pending, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Pending);
    }

    #[test]
    fn empty_out_is_error_unless_running() {
        let r = evaluate("x", sp_input(), Observation::Empty, QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Error);
        let q = QueueState::Running { job_id: "1".into(), workdir: "/w".into() };
        let r = evaluate("x", sp_input(), Observation::Empty, q, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Running);
    }

    #[test]
    fn unreadable_out_is_error() {
        let r = evaluate("x", sp_input(), Observation::Unreadable("permission denied".into()), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Error);
        assert!(r.messages[0].contains("permission denied"));
    }

    #[test]
    fn not_terminated_is_error_or_running() {
        let text = "FINAL SINGLE POINT ENERGY   -1.0\nSCF CONVERGED AFTER 10 CYCLES\n";
        let r = evaluate("x", sp_input(), parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Error);
        assert!(r.messages[0].contains("did not terminate normally"));
        let q = QueueState::Running { job_id: "42".into(), workdir: "/scratch".into() };
        let r = evaluate("x", sp_input(), parsed(text), q, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Running);
        assert!(r.messages[0].contains("42"));
        assert!(!r.in_csv);
    }

    #[test]
    fn scf_not_converged() {
        let text = "SCF NOT CONVERGED AFTER 125 CYCLES\nORCA finished by error termination in SCF\n";
        let r = evaluate("x", sp_input(), parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Error);
        assert_eq!(r.scf, Some(ScfConvergence::Failed));
        assert!(r.messages.iter().any(|m| m.contains("error termination in SCF")));
        assert!(r.messages.iter().any(|m| m.contains("after 125 cycles")));
    }

    #[test]
    fn not_fully_converged_respects_ncf() {
        let text = "FINAL SINGLE POINT ENERGY   -1.0 (Wavefunction not fully converged!)\n";
        let r = evaluate("x", sp_input(), parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Error);
        assert!(!r.in_csv);
        let opts = AnalysisOptions { accept_not_fully_converged: true, ..Default::default() };
        let r = evaluate("x", sp_input(), parsed(text), QueueState::Unknown, &opts);
        assert_eq!(r.status, Status::Warning);
        assert!(r.in_csv);
        assert_eq!(r.scf, Some(ScfConvergence::NotFully));
    }

    #[test]
    fn spin_contamination_warning_can_be_silenced() {
        let text = "FINAL SINGLE POINT ENERGY   -1.0\nExpectation value of <S**2>     :     1.05\nIdeal value S*(S+1) for S=0.5   :     0.75\nDeviation                       :     0.30\n****ORCA TERMINATED NORMALLY****\n";
        let r = evaluate("x", sp_input(), parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Warning);
        assert!(r.spin.unwrap().significant);
        let opts = AnalysisOptions { warn_spin_contamination: false, ..Default::default() };
        let r = evaluate("x", sp_input(), parsed(text), QueueState::Unknown, &opts);
        assert_eq!(r.status, Status::Ok);
        assert!(r.spin.unwrap().significant);
    }

    #[test]
    fn missing_s2_for_open_shell_warns_but_singlet_does_not() {
        let text = "FINAL SINGLE POINT ENERGY   -1.0\n****ORCA TERMINATED NORMALLY****\n";
        let r = evaluate("x", sp_input(), parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Warning);
        let singlet = parse_input_text("! hf\n* xyz 0 1\nH 0 0 0\n*");
        let r = evaluate("x", singlet, parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Ok);
        assert!(r.messages.is_empty());
    }

    #[test]
    fn input_name_mismatch_warns() {
        let text = "NAME = other.inp\n|  1> ! hf\n|  2> ****END OF INPUT****\nFINAL SINGLE POINT ENERGY   -1.0\n****ORCA TERMINATED NORMALLY****\n";
        let singlet = parse_input_text("! hf\n* xyz 0 1\nH 0 0 0\n*");
        let r = evaluate("mine", singlet, parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Warning);
        assert!(r.messages.iter().any(|m| m.contains("other.inp")));
    }

    #[test]
    fn charge_from_output_when_input_lacks_it() {
        let text = " Total Charge           Charge          ....   -1\n Multiplicity           Mult            ....    2\nFINAL SINGLE POINT ENERGY   -1.0\nExpectation value of <S**2>     :     0.75\n****ORCA TERMINATED NORMALLY****\n";
        let inp = parse_input_text("! uhf\n* xyzfile -1 2 a.xyz");
        let r = evaluate("x", inp, parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.charge, Some(-1));
        assert_eq!(r.multiplicity, Some(2));
        let inp = parse_input_text("! uhf");
        let r = evaluate("x", inp, parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!((r.charge, r.multiplicity), (Some(-1), Some(2)));
    }

    #[test]
    fn opt_requires_geometry_convergence() {
        let inp = parse_input_text("! r2scan-3c opt\n* xyz 0 1\nH 0 0 0\nH 0 0 0.7\n*");
        let not_conv = "GEOMETRY OPTIMIZATION CYCLE   3\nFINAL SINGLE POINT ENERGY   -1.0\nThe optimization did not converge but reached the maximum number of optimization cycles\n****ORCA TERMINATED NORMALLY****\n";
        let r = evaluate("x", inp.clone(), parsed(not_conv), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Error);
        assert!(r.messages.iter().any(|m| m.contains("NOT converged (after 3 cycles)")));
        let conv = "GEOMETRY OPTIMIZATION CYCLE   3\nTHE OPTIMIZATION HAS CONVERGED\nFINAL SINGLE POINT ENERGY   -1.1\n****ORCA TERMINATED NORMALLY****\n";
        let r = evaluate("x", inp, parsed(conv), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Ok);
        assert_eq!(r.energy, Some(-1.1));
        assert!(r.messages[0].contains("#geom_cycles:   3"));
    }

    #[test]
    fn freq_character_and_thermo() {
        let inp = parse_input_text("! r2scan-3c freq\n* xyz 0 1\nH 0 0 0\nH 0 0 0.7\n*");
        let text = "FINAL SINGLE POINT ENERGY   -1.0\nTHE CP-SCF HAS CONVERGED\nVIBRATIONAL FREQUENCIES\n-----\n   0:   0.00 cm**-1\n   5:   -150.00 cm**-1 ***imaginary mode***\n   6:   -5.00 cm**-1\n   7:   3000.00 cm**-1\n\nNORMAL MODES\nTemperature         ... 298.15 K\nZero point energy                ...      0.01 Eh      13.21 kcal/mol\nTotal correction                          0.02 Eh      14.99 kcal/mol\nThermal Enthalpy correction       ...      0.00094421 Eh       0.59 kcal/mol\nFinal entropy term                ...      0.03 Eh     13.41 kcal/mol\nG-E(el)                           ...      0.04 Eh       2.17 kcal/mol\n****ORCA TERMINATED NORMALLY****\n";
        let r = evaluate("x", inp, parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Ok, "{:?}", r.messages);
        assert_eq!(r.character, Some(Character::TransitionState));
        assert_eq!(r.imaginary, vec![-150.0]);
        let t = r.thermo.unwrap();
        assert_eq!(t.zpve, 0.01);
        assert_eq!(t.du, 0.02);
        assert!((t.dh - 0.02094421).abs() < 1e-12);
        assert_eq!(t.tds, 0.03);
        assert_eq!(t.dg, 0.04);
        assert!(r.messages[0].contains("CHAR:   TS"));
        assert!(r.messages[0].contains("IMAGS: -150"));
    }

    #[test]
    fn freq_threshold_is_configurable() {
        let inp = parse_input_text("! r2scan-3c freq\n* xyz 0 1\nH 0 0 0\n*");
        let text = "FINAL SINGLE POINT ENERGY   -1.0\nVIBRATIONAL FREQUENCIES\n   6:   -30.00 cm**-1\n   7:   -25.00 cm**-1\n****ORCA TERMINATED NORMALLY****\n";
        let opts = AnalysisOptions { imaginary_threshold: -50.0, ..Default::default() };
        let r = evaluate("x", inp.clone(), parsed(text), QueueState::Unknown, &opts);
        assert_eq!(r.character, Some(Character::Minimum));
        let r = evaluate("x", inp, parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.character, Some(Character::Fail));
        assert_eq!(r.status, Status::Warning); // no CP-SCF, no thermo
    }

    #[test]
    fn thermo_falls_back_to_kt_at_temperature() {
        let text = "Temperature         ... 400.00 K\nZero point energy                ...      0.01 Eh      13.21 kcal/mol\nTotal correction                          0.02 Eh      14.99 kcal/mol\nFinal entropy term                ...      0.03 Eh     13.41 kcal/mol\nG-E(el)                           ...      0.04 Eh       2.17 kcal/mol\n";
        let t = thermo_result(&parse_output_text(text)).unwrap();
        assert!((t.dh - (0.02 + K_B_EH * 400.0)).abs() < 1e-12);
        assert!(thermo_result(&parse_output_text("Zero point energy ... 0.01 Eh 1 kcal/mol\n")).is_none());
    }

    #[test]
    fn nics_filters_ghost_atoms() {
        let inp = parse_input_text("! pbe0 def2-tzvp nmr\n* xyz 0 1\nC 0 0 0\nH: 0 0 1\nH: 0 0 2\n*");
        let text = "FINAL SINGLE POINT ENERGY   -1.0\nTHE CP-SCF HAS CONVERGED\nCHEMICAL SHIELDING SUMMARY (ppm)\n\n  Nucleus  Element    Isotropic     Anisotropy\n      0       C          55.1        120.5\n      1       H         -10.5          1.5\n      2       H          -9.0          1.0\n\n****ORCA TERMINATED NORMALLY****\n";
        let r = evaluate("x", inp, parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Ok, "{:?}", r.messages);
        assert_eq!(r.nics, vec![(1, -10.5), (2, -9.0)]);
    }

    #[test]
    fn include_failed_puts_errors_in_csv() {
        let opts = AnalysisOptions { include_failed: true, ..Default::default() };
        let r = evaluate("x", sp_input(), parsed("aborting the run\n"), QueueState::Unknown, &opts);
        assert_eq!(r.status, Status::Error);
        assert!(r.in_csv);
    }

    #[test]
    fn invalid_input_is_error() {
        let r = evaluate("x", parse_input_text("asdf"), parsed(GOOD_SP), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Error);
        assert_eq!(r.job_type, JobType::Invalid);
    }

    #[test]
    fn character_classification() {
        assert_eq!(character(0), Character::Minimum);
        assert_eq!(character(1), Character::TransitionState);
        assert_eq!(character(2), Character::Fail);
    }

    #[test]
    fn numerical_frequencies_without_cpscf_are_ok() {
        let inp = parse_input_text("! r2scan-3c numfreq\n* xyz 0 1\nH 0 0 0\nH 0 0 0.7\n*");
        let text = "FINAL SINGLE POINT ENERGY   -1.0\nVIBRATIONAL FREQUENCIES\n   5:   0.00 cm**-1\n   6:   4400.00 cm**-1\n\nTemperature         ... 298.15 K\nZero point energy                ...      0.01 Eh      13.21 kcal/mol\nTotal correction                          0.02 Eh      14.99 kcal/mol\nFinal entropy term                ...      0.03 Eh     13.41 kcal/mol\nG-E(el)                           ...      0.04 Eh       2.17 kcal/mol\n****ORCA TERMINATED NORMALLY****\n";
        let r = evaluate("x", inp, parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Ok, "{:?}", r.messages);
        assert_eq!(response_convergence(r.output.as_ref().unwrap(), JobType::Freq), None);
    }

    #[test]
    fn started_but_unconverged_cpscf_is_an_error() {
        let inp = parse_input_text("! pbe0 def2-tzvp nmr\n* xyz 0 1\nC 0 0 0\n*");
        let text = "FINAL SINGLE POINT ENERGY   -1.0\n     CP-SCF ITERATION   0:      0.0004\nCHEMICAL SHIELDING SUMMARY (ppm)\n      0       C          55.1        120.5\n\n****ORCA TERMINATED NORMALLY****\n";
        let r = evaluate("x", inp, parsed(text), QueueState::Unknown, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Error);
        assert!(r.messages.iter().any(|m| m.contains("CP-SCF was started but did not converge")));
    }

    #[test]
    fn response_convergence_combines_davidson_and_cpscf() {
        let d = |text: &str| parse_output_text(text);
        let conv = "DAVIDSON-DIAGONALIZATION\n*** CONVERGENCE OF RESIDUAL NORM REACHED ***\n";
        assert_eq!(response_convergence(&d(conv), JobType::Tddft), Some(true));
        assert_eq!(response_convergence(&d(conv), JobType::Sp), None, "Davidson only counts for TD-DFT jobs");
        assert_eq!(response_convergence(&d("DAVIDSON-DIAGONALIZATION\n"), JobType::Tddft), Some(false));
        let grad = format!("{conv}     CP-SCF ITERATION   0:      0.0004\n");
        assert_eq!(response_convergence(&d(&grad), JobType::Tddft), Some(false), "unconverged Z-vector CP-SCF");
        let grad_ok = format!("{grad} *** THE CP-SCF HAS CONVERGED ***\n");
        assert_eq!(response_convergence(&d(&grad_ok), JobType::Tddft), Some(true));
    }

    #[test]
    fn unavailable_queue_changes_messages_not_status() {
        let text = "FINAL SINGLE POINT ENERGY   -1.0\nSCF CONVERGED AFTER 10 CYCLES\n";
        let r = evaluate("x", sp_input(), parsed(text), QueueState::Unavailable, &AnalysisOptions::default());
        assert_eq!(r.status, Status::Error);
        assert!(r.messages[0].contains("queue status unknown"), "{:?}", r.messages);
        let r = evaluate("x", sp_input(), Observation::Missing, QueueState::Unavailable, &AnalysisOptions::default());
        assert_eq!(r.status, Status::NotSubmitted);
        assert!(r.messages[0].contains("queue status unknown"));
    }
}

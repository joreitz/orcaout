# orcaout

Summarises ORCA quantum-chemistry calculations into CSV files. A Rust
re-implementation of the `outorca` / `outorca_utils.py` scripts: it reads
every `.inp` in a directory, streams through the matching `.out`, decides
whether the job is fine (OK / WARNING / ERROR / RUNNING / PENDING / MISSING)
and writes

* `outorca.csv` – one row per successful job, one header per job type,
* `outorca.err` – the status lines for every job (uncoloured),
* `<name>.best.xyz` – final geometry of converged optimisations.

Output files are parsed **line by line** and all jobs of a directory are
parsed **in parallel** (rayon), so multi-GB outputs and folders with hundreds
of jobs are fine: memory stays at a few MB.

## Usage

```bash
cargo build --release
./target/release/orcaout                 # current directory
./target/release/orcaout -r projects/    # every sub-directory with .inp files
./target/release/orcaout job.out         # single .out (input taken from the echo in the .out)
```

| Option | Meaning |
|---|---|
| `-r`, `--recursive` | analyse all sub-directories that contain `.inp`/`.out` files |
| `-ncf`, `--noconvflag` | accept *Wavefunction not fully converged* results into the CSV (`SCFCONV=notfully`) |
| `-npolf`, `--nopolflag` | do not warn about spin contamination (deviation of `<S²>` ≥ 0.10) |
| `-imag`, `--imaginary <cm⁻¹>` | frequencies at or below this value count as imaginary (default −20) |
| `-a`, `--all` | also write failed jobs to the CSV (adds a `STATUS` column) |
| `-cwd`, `--workdir` | show the SLURM working directory of running jobs |
| `-of`, `--onefile <out>` | analyse one `.out` file |
| `-o`, `--output <name>` | CSV file name (default `outorca.csv`; `.err` is derived from it) |
| `--no-xyz` | do not write `.best.xyz` files |
| `--no-queue` | do not call `squeue` |
| `-j`, `--threads <n>` | number of parallel workers |
| `-q`, `--quiet` | print only warnings and errors |
| `--no-color` | plain output (`NO_COLOR` is honoured too) |

Exit code is 0 after a normal run (failed *jobs* are not program errors),
1 when nothing could be analysed or files could not be written, 2 on a
usage error.

## Job types and CSV columns

The job type is read from the `!` line(s) of the input (comments stripped,
case-insensitive). Each job type has its own header; a directory with several
job types gets one file per type (`outorca_opt.csv`, `outorca_freq.csv`, …).

| Job type | Detected by | Required columns |
|---|---|---|
| `sp` | nothing special | `NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV` |
| `ftdft` | `FOD` keyword or `SmearTemp` | `NAME;JOB;METHOD;ENERGY;CHRG;M;N_FOD;SCFCONV` |
| `opt` | `Opt`, `TightOpt`, … | `…;S^2;SCFCONV;GEOCONV` |
| `freq` | `Freq`, `NumFreq` | `…;S^2;SCFCONV;CPSCFCONV;ZPVE;dU;dH;TdS;dG;CHAR;IMAGS` |
| `optfreq` | opt + freq | `…;S^2;SCFCONV;GEOCONV;CPSCFCONV;ZPVE;dU;dH;TdS;dG;CHAR;IMAGS` |
| `nmr` | `NMR` | `…;S^2;SCFCONV;CPSCFCONV;NUM_GHST_ATMS;NICS[ID,ISO]` |
| `scan` | `%geom Scan` block | `…;S^2;SCFCONV;SCAN[COORD,E]` |
| `tddft` | `TDDFT` keyword or `%tddft` block | `…;S^2;SCFCONV;CPSCFCONV;EXC[STATE,EV]` |

Optional columns are appended only when at least one row has a value:
`BASIS`, `GEOCYCLES`, `DISP` (dispersion correction, Eh), `DIPOLE` (Debye),
`RUNTIME` (s), `SPEC[EV,FOSC]` (TD-DFT), `STATUS` (with `--all`).

Column semantics (as in the Python original):

* `ENERGY` – last `FINAL SINGLE POINT ENERGY` (Eh).
* `S^2` – last `<S**2>` (3 decimals). A deviation ≥ 0.10 from S(S+1) gives a warning.
* `SCFCONV` – `True`, `notfully` (only with `-ncf`) or `False`.
* `GEOCONV` – `THE OPTIMIZATION HAS CONVERGED` seen.
* `CPSCFCONV` – convergence of the response solver that actually ran, empty if none ran:
  * CP-SCF (analytic frequencies, NMR, TD-DFT gradients): started by `CP-SCF ITERATION   0`,
    converged by `THE CP-SCF HAS CONVERGED`. `NumFreq` runs no CP-SCF, so the cell stays empty
    and no warning is given.
  * TD-DFT/CIS excitation energies come from a Davidson diagonalisation, not from a CP-SCF.
    For `tddft` jobs the column reports the Davidson convergence
    (`CONVERGENCE OF RESIDUAL NORM REACHED` / `ORCA-CIS/TD-DFT FINISHED WITHOUT ERROR`),
    combined with the CP-SCF if one ran as well.
  * A solver that started but did not converge is an ERROR.
* `ZPVE`, `dU` (`Total correction`), `TdS` (`Final entropy term`), `dG` (`G-E(el)`) in Eh;
  `dH = dU + Thermal Enthalpy correction` (kT at the printed temperature when that line is missing).
* `CHAR` – `MIN` (no imaginary mode), `TS` (one), `FAIL` (more); `IMAGS` – the imaginary frequencies, `|`-separated.
* `NICS[ID,ISO]` – isotropic shieldings of the ghost atoms (`H:` syntax in the input), `[[id, iso], ...]`.
* `SCAN[COORD,E]` – `[(coordinate,energy),...]` from *The Calculated Surface using the 'Actual Energy'*, ready for plotting.
* `EXC[STATE,EV]` – `[(state,eV),...]` from the TD-DFT excited-state block (RKS singlet and triplet blocks are both kept).
* `SPEC[EV,FOSC]` – `[(eV,fosc),...]` from `ABSORPTION SPECTRUM VIA TRANSITION ELECTRIC DIPOLE MOMENTS`
  (ORCA 6 prints eV; for ORCA 5 the wavenumber is converted with 1 eV = 8065.543937 cm⁻¹).

## Status logic

| Status | When |
|---|---|
| `OK` | terminated normally, SCF (and geometry) converged, nothing suspicious |
| `WARNING` | spin contamination, `-ncf` accepted a not fully converged wave function, `.out` belongs to a different `.inp`, missing CP-SCF line, charge/multiplicity mismatch between `.inp` and `.out`, … |
| `ERROR` | no normal termination, SCF/geometry not converged, error termination, input error, empty or unreadable `.out`, no `!` line, … |
| `RUNNING` / `PENDING` | SLURM lists a job for this file stem in this directory (see below) |
| `MISSING` | no `.out` and nothing in the queue |

### SLURM queue

`squeue -u $USER -h -o "%i|%j|%t|%Z"` is called once per run (job id, name, state, working directory).

* A job belongs to the stem `mol_1` if its name is `mol_1` or starts with `mol_1.`
  (`mol_10.sp.sh` does not). If several stems match (`x.sp`, `x.sp.pbe0`), the longest wins.
* Running **and** pending jobs only count if their working directory is the analysed directory.
* If `squeue` is missing or fails, a `WARNING  squeue: ...` line goes to stderr and unfinished
  jobs are reported with "queue status unknown". `--no-queue` skips the lookup without warning.

Check once on your cluster that the format is understood (four `|`-separated fields per line):

```bash
squeue -u $USER -h -o "%i|%j|%t|%Z"
```

Recognised error lines include `ORCA finished by error termination in …`,
`aborting the run`, `SCF NOT CONVERGED`, `The optimization did not converge`,
`INPUT ERROR` / `UNRECOGNIZED OR DUPLICATED KEYWORD`, multiplicity errors,
memory errors, MPI aborts, segmentation faults and SLURM time-limit / cancel
messages. Only jobs with status OK or WARNING are written to the CSV unless
`--all` is given.

## Extending

The code is split into independent layers (`src/`):

| Module | Job |
|---|---|
| `input.rs` | job type, method, basis set, charge, multiplicity, atoms, ghost atoms from the `.inp` |
| `known.rs` | lists of known methods and basis sets (extend when a keyword is not recognised) |
| `output.rs` | streaming parser of the `.out` → `OutputData` (all fields optional) |
| `analysis.rs` | `evaluate()` turns input + output into a `JobReport` (status, messages, derived values) |
| `columns.rs` | CSV column registry and per-job-type headers |
| `queue.rs` | `squeue` lookup |
| `app.rs` | directory discovery, parallel execution, file writing, terminal output |
| `cli.rs` | argument parsing |

**Adding a quantity to the CSV** takes three steps:

1. `output.rs`: add a field to `OutputData`, a `Line` variant and a row in
   `LINE_PATTERNS` (regex), and an arm in `Parser::handle` that stores the value.
   Multi-line blocks get a `Section` variant instead.
2. `columns.rs`: add a `Column { name, value }` entry to `COLUMNS`. It is now an
   optional column: written whenever any job in the folder has the value.
3. If the column must always be present for some job types, add its name to the
   corresponding list in `required_columns`.

Add a unit test next to the parser (`parse_output_text("...")`) and, if the
value is job-type specific, a fixture in `tests/fixtures/synthetic`.

## Tests

```bash
cargo test
```

Unit tests live next to the code; `tests/integration.rs` runs the binary on
copies of the fixtures (`tests/fixtures/sp_uks` is a real ORCA 6.1 output,
`tests/fixtures/synthetic` are minimal hand-written outputs for every job
type and failure mode). `test.sh` regenerates `test_inputs/` (job-type
detection samples).

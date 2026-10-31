//! End-to-end tests: run the `orcaout` binary on copies of the fixtures and
//! check the files it writes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

/// Fresh temporary directory, unique per test.
fn temp_dir(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("orcaout-test-{}-{n}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn copy_fixture(src_dir: &Path, stem: &str, dst: &Path) {
    for ext in ["inp", "out"] {
        let src = src_dir.join(format!("{stem}.{ext}"));
        if src.exists() {
            fs::copy(&src, dst.join(format!("{stem}.{ext}"))).unwrap();
        }
    }
}

fn copy_synthetic(stems: &[&str], dst: &Path) {
    for s in stems {
        copy_fixture(&fixtures().join("synthetic"), s, dst);
    }
}

fn orcaout(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_orcaout"))
        .args(["--no-queue", "--no-color"])
        .args(args)
        .env_remove("USER")
        .output()
        .expect("binary runs")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn real_sp_output_gives_python_compatible_row() {
    let dir = temp_dir("sp");
    copy_fixture(&fixtures().join("sp_uks"), "cola3_n2_4.sp.wb97x-d4rev", &dir);
    let o = orcaout(&[dir.to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let csv = read(&dir.join("outorca.csv"));
    assert_eq!(
        csv,
        "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;BASIS;DISP;DIPOLE;RUNTIME\n\
         cola3_n2_4.sp.wb97x-d4rev;sp;wb97x-d4rev;-4128.719797087585;-2;4;3.768;True;def2-qzvppd;-0.27838099;7.734068328;151099.349\n"
    );
    let err = read(&dir.join("outorca.err"));
    assert!(err.starts_with("OK       cola3_n2_4.sp.wb97x-d4rev"), "{err}");
    assert!(err.contains("<S^2>: 3.768"));
    assert!(stdout(&o).contains("1 job(s): 1 OK"));
}

#[test]
fn out_without_inp_uses_embedded_input() {
    let dir = temp_dir("onefile");
    fs::copy(
        fixtures().join("sp_uks").join("cola3_n2_4.sp.wb97x-d4rev.out"),
        dir.join("cola3_n2_4.sp.wb97x-d4rev.out"),
    )
    .unwrap();
    let out_file = dir.join("cola3_n2_4.sp.wb97x-d4rev.out");
    let o = orcaout(&["-of", out_file.to_str().unwrap()]);
    assert!(o.status.success());
    let csv = read(&dir.join("outorca.csv"));
    assert!(csv.contains("cola3_n2_4.sp.wb97x-d4rev;sp;wb97x-d4rev;-4128.719797087585;-2;4;3.768;True"), "{csv}");
    assert!(stdout(&o).contains("using the input echoed in the .out"));
}

#[test]
fn every_job_type_gets_its_own_header() {
    let dir = temp_dir("mixed");
    copy_synthetic(&["opt_ok", "freq_ts", "numfreq", "optfreq_min", "nics", "fod", "scan", "tddft", "spincont"], &dir);
    let o = orcaout(&[dir.to_str().unwrap()]);
    assert!(o.status.success());

    let expect = [
        ("outorca_sp.csv", "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;BASIS\nspincont;sp;b3lyp;-75.7;0;2;1.250;True;def2-svp\n"),
        ("outorca_ftdft.csv", "NAME;JOB;METHOD;ENERGY;CHRG;M;N_FOD;SCFCONV\nfod;ftdft;fod;-1.1;0;1;0.123456;True\n"),
        (
            "outorca_opt.csv",
            "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;GEOCONV;GEOCYCLES;DIPOLE;RUNTIME\nopt_ok;opt;r2scan-3c;-1.165000000003;0;1;;True;True;2;0;12.345\n",
        ),
        (
            "outorca_freq.csv",
            "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;CPSCFCONV;ZPVE;dU;dH;TdS;dG;CHAR;IMAGS;RUNTIME\nfreq_ts;freq;r2scan-3c;-39.123456789;0;2;0.760;True;True;0.00964643;0.01248239;0.0134266;0.02136579;-0.00793919;TS;-512.34;120\n\
             numfreq;freq;r2scan-3c;-39.123456789;0;2;0.760;True;;0.00964643;0.01248239;0.0134266;0.02136579;-0.00793919;TS;-512.34;120\n",
        ),
        (
            "outorca_optfreq.csv",
            "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;GEOCONV;CPSCFCONV;ZPVE;dU;dH;TdS;dG;CHAR;IMAGS;GEOCYCLES\noptfreq_min;optfreq;r2scan-3c;-1.165000000003;0;1;;True;True;True;0.01002;0.01238;0.01332421;0.01482;-0.0015;MIN;;3\n",
        ),
        (
            "outorca_nmr.csv",
            "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;CPSCFCONV;NUM_GHST_ATMS;NICS[ID,ISO];BASIS\nnics;nmr;pbe0;-77;0;1;;True;True;2;[[2, -10.25], [3, -3.75]];def2-tzvp\n",
        ),
        (
            "outorca_scan.csv",
            "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;SCAN[COORD,E]\nscan;scan;r2scan-3c;-1.15;0;1;;True;[(0.7,-1.11675931),(0.8,-1.165),(0.9,-1.15)]\n",
        ),
        (
            "outorca_tddft.csv",
            "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;CPSCFCONV;EXC[STATE,EV];BASIS\ntddft;tddft;wb97x-d3;-1.17;0;1;;True;True;[(1,12.408),(2,16.327)];def2-tzvp\n",
        ),
    ];
    for (file, content) in expect {
        assert_eq!(read(&dir.join(file)), content, "{file}");
    }
    assert!(!dir.join("outorca.csv").exists(), "mixed folders get one file per job type");

    // converged optimisations produce .best.xyz
    let xyz = read(&dir.join("opt_ok.best.xyz"));
    assert_eq!(xyz.lines().next(), Some("2"));
    assert!(xyz.lines().nth(1).unwrap().starts_with("-1.165000000003   r2scan-3c, C: 0, M: 1"));
    assert_eq!(xyz.lines().count(), 4);
    assert!(dir.join("optfreq_min.best.xyz").exists());
    assert!(!dir.join("freq_ts.best.xyz").exists());

    let text = stdout(&o);
    assert!(text.contains("WARNING  spincont"), "{text}");
    assert!(text.contains("CHAR:   TS, IMAGS: -512.34"));
    assert!(text.contains("OK       numfreq"), "NumFreq without CP-SCF is not penalised:\n{text}");
    assert!(!text.contains("CP-SCF"), "{text}");
    assert!(text.contains("9 job(s): 8 OK, 1 WARNING"), "{text}");
}

#[test]
fn failed_jobs_are_reported_but_not_in_csv() {
    let dir = temp_dir("failed");
    copy_synthetic(&["opt_ok", "opt_fail", "scf_fail", "notfully", "incomplete", "empty", "missing", "garbage", "input_error"], &dir);
    let o = orcaout(&[dir.to_str().unwrap()]);
    assert!(o.status.success(), "failed jobs are not a program error");
    let text = stdout(&o);
    for (status, name, needle) in [
        ("ERROR", "opt_fail", "geometry optimization NOT converged (after 50 cycles)"),
        ("ERROR", "scf_fail", "SCF did not converge after 125 cycles"),
        ("ERROR", "scf_fail", "ORCA finished by error termination in SCF"),
        ("ERROR", "notfully", "wave function not fully converged (use -ncf to accept)"),
        ("ERROR", "incomplete", "ORCA did not terminate normally"),
        ("ERROR", "empty", ".out file is empty"),
        ("MISSING", "missing", "no .out file, job not submitted?"),
        ("ERROR", "garbage", ".out file is empty"),
        ("ERROR", "input_error", "UNRECOGNIZED OR DUPLICATED KEYWORD"),
        ("OK", "opt_ok", "#geom_cycles:   2"),
    ] {
        let head = format!("{status:<8} {name:<50}");
        assert!(text.contains(&head), "missing `{head}` in\n{text}");
        assert!(text.contains(needle), "missing `{needle}` in\n{text}");
    }
    assert!(!text.contains("SCF did not converge\n"), "input error must not be reported as SCF failure:\n{text}");

    // opt_ok is the only row; the failed opt jobs are left out
    let opt = read(&dir.join("outorca_opt.csv"));
    assert_eq!(opt.lines().count(), 2, "{opt}");
    assert!(opt.contains("\nopt_ok;opt;"));
    let sp = read(&dir.join("outorca_sp.csv"));
    assert_eq!(sp, "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV\n", "only the header for sp");

    // the .err file has every job, uncoloured
    let err = read(&dir.join("outorca.err"));
    assert_eq!(err.lines().filter(|l| !l.starts_with(' ')).count(), 9);
    assert!(!err.contains('\x1b'));
}

#[test]
fn ncf_accepts_not_fully_converged_and_all_includes_failed() {
    let dir = temp_dir("ncf");
    copy_synthetic(&["notfully", "scf_fail", "spincont"], &dir);
    let o = orcaout(&["-ncf", "-npolf", "--all", dir.to_str().unwrap()]);
    assert!(o.status.success());
    let csv = read(&dir.join("outorca.csv"));
    assert_eq!(
        csv,
        "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;BASIS;STATUS\n\
         notfully;sp;b3lyp;-150.123456;0;3;2.010;notfully;def2-svp;\n\
         scf_fail;sp;b3lyp;;0;3;;False;def2-svp;ERROR\n\
         spincont;sp;b3lyp;-75.7;0;2;1.250;True;def2-svp;\n"
    );
    let text = stdout(&o);
    assert!(text.contains("WARNING  notfully"), "{text}");
    assert!(text.contains("OK       spincont"), "-npolf silences the spin warning:\n{text}");
    assert!(text.contains("NOT checking for spin contamination"));
}

#[test]
fn imaginary_threshold_flag() {
    let dir = temp_dir("imag");
    copy_synthetic(&["freq_ts"], &dir);
    let o = orcaout(&["-imag", "-600", dir.to_str().unwrap()]);
    assert!(o.status.success());
    let csv = read(&dir.join("outorca.csv"));
    assert!(csv.contains(";MIN;;"), "{csv}");
    assert!(stdout(&o).contains("CHAR:  MIN"));
}

#[test]
fn recursive_mode_and_custom_output_name() {
    let root = temp_dir("recursive");
    let a = root.join("a");
    let b = root.join("b").join("deep");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    fs::create_dir_all(root.join(".hidden")).unwrap();
    copy_synthetic(&["opt_ok"], &a);
    copy_synthetic(&["fod"], &b);
    copy_synthetic(&["fod"], &root.join(".hidden"));
    let o = orcaout(&["-r", "-o", "summary.csv", root.to_str().unwrap()]);
    assert!(o.status.success());
    assert!(a.join("summary.csv").exists());
    assert!(a.join("summary.err").exists());
    assert!(b.join("summary.csv").exists());
    assert!(!root.join("summary.csv").exists(), "root has no inputs");
    assert!(!root.join(".hidden").join("summary.csv").exists(), "hidden directories are skipped");
    let text = stdout(&o);
    assert!(text.contains("Directory: "));
}

#[test]
fn explicit_inp_files_restrict_the_analysis() {
    let dir = temp_dir("explicit");
    copy_synthetic(&["opt_ok", "opt_fail", "fod"], &dir);
    let inp = dir.join("opt_ok.inp");
    let o = orcaout(&[inp.to_str().unwrap()]);
    assert!(o.status.success());
    let csv = read(&dir.join("outorca.csv"));
    assert_eq!(csv.lines().count(), 2);
    assert!(csv.contains("\nopt_ok;"));
    assert!(!stdout(&o).contains("opt_fail"));
}

#[test]
fn no_xyz_flag() {
    let dir = temp_dir("noxyz");
    copy_synthetic(&["opt_ok"], &dir);
    assert!(orcaout(&["--no-xyz", dir.to_str().unwrap()]).status.success());
    assert!(!dir.join("opt_ok.best.xyz").exists());
}

#[test]
fn quiet_hides_ok_lines() {
    let dir = temp_dir("quiet");
    copy_synthetic(&["opt_ok", "opt_fail"], &dir);
    let text = stdout(&orcaout(&["-q", dir.to_str().unwrap()]));
    assert!(!text.contains("OK       opt_ok"));
    assert!(text.contains("ERROR    opt_fail"));
}

#[test]
fn empty_directory_and_bad_paths() {
    let dir = temp_dir("nothing");
    let o = orcaout(&[dir.to_str().unwrap()]);
    assert!(o.status.success());
    assert!(stdout(&o).contains("No .inp/.out files in"));
    assert!(!dir.join("outorca.csv").exists());

    let o = orcaout(&["/definitely/not/here"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("no such file or directory"));

    let o = orcaout(&["--bogus"]);
    assert_eq!(o.status.code(), Some(2));

    let o = orcaout(&["--help"]);
    assert!(o.status.success());
    assert!(stdout(&o).contains("USAGE"));
}

#[test]
fn unreadable_out_is_an_error_not_a_crash() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("unreadable");
    copy_synthetic(&["opt_ok"], &dir);
    fs::set_permissions(dir.join("opt_ok.out"), fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(dir.join("opt_ok.out")).is_ok() {
        return; // running as root: every file is readable, nothing to test
    }
    let o = orcaout(&[dir.to_str().unwrap()]);
    assert!(o.status.success());
    assert!(stdout(&o).contains("could not read .out file"), "{}", stdout(&o));
    fs::set_permissions(dir.join("opt_ok.out"), fs::Permissions::from_mode(0o644)).unwrap();
}

#[test]
fn large_output_is_streamed() {
    // A 200 MB output built by repeating the SCF iteration block must parse
    // without problems; the parser keeps only a handful of values.
    let dir = temp_dir("large");
    let src = read(&fixtures().join("sp_uks").join("cola3_n2_4.sp.wb97x-d4rev.out"));
    let (head, tail) = src.split_once("                 **** Energy Check signals convergence ****").unwrap();
    let iteration = "   50    -4128.4372548561368603    -1.50e-05  4.03e-04  3.10e-02  7.86e-03  1504.5\n";
    let mut out = fs::File::create(dir.join("big.out")).unwrap();
    use std::io::Write;
    out.write_all(head.as_bytes()).unwrap();
    let block = iteration.repeat(10_000); // ~850 kB
    for _ in 0..240 {
        out.write_all(block.as_bytes()).unwrap();
    }
    out.write_all("                 **** Energy Check signals convergence ****".as_bytes()).unwrap();
    out.write_all(tail.as_bytes()).unwrap();
    drop(out);
    fs::copy(fixtures().join("sp_uks").join("cola3_n2_4.sp.wb97x-d4rev.inp"), dir.join("big.inp")).unwrap();
    let o = orcaout(&[dir.to_str().unwrap()]);
    assert!(o.status.success());
    let csv = read(&dir.join("outorca.csv"));
    assert!(csv.contains("big;sp;wb97x-d4rev;-4128.719797087585;-2;4;3.768;True"), "{csv}");
    assert!(stdout(&o).contains("WARNING  big"), "input name mismatch is reported:\n{}", stdout(&o));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn real_tddft_output_with_uks_hint_line() {
    // ORCA 6.0.1 prints "UHF/UKS reference: multiplicity estimated ..." between the
    // header and the first STATE line; the first parser version stopped there.
    let dir = temp_dir("tddft");
    copy_fixture(&fixtures().join("tddft_uks"), "cl_si20et19_n0_1.sp.pbe0", &dir);
    let o = orcaout(&[dir.to_str().unwrap()]);
    assert!(o.status.success());
    let text = stdout(&o);
    assert!(text.contains("OK       cl_si20et19_n0_1.sp.pbe0"), "{text}");
    assert!(text.contains("#states: 7"), "{text}");
    assert_eq!(
        read(&dir.join("outorca.csv")),
        "NAME;JOB;METHOD;ENERGY;CHRG;M;S^2;SCFCONV;CPSCFCONV;EXC[STATE,EV];BASIS;SPEC[EV,FOSC];DISP;DIPOLE;RUNTIME\n\
         cl_si20et19_n0_1.sp.pbe0;tddft;pbe0;-7752.36824903536;0;1;0.000;True;True;\
         [(1,2.952),(2,2.962),(3,3.201),(4,3.212),(5,3.351),(129,4.838),(130,4.847)];ma-def2-tzvp;\
         [(2.95163,0),(2.962001,0),(3.20077,0.010973602),(3.211695,0.012009639),(3.350784,0),(4.838284,0),(4.847203,0.002974608)];\
         -0.304521096;3.620416647;92506.46\n"
    );
}

fn orcaout_with_queue(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_orcaout"));
    cmd.arg("--no-color").args(args).env_remove("USER").env_remove("LOGNAME");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("binary runs")
}

#[test]
fn squeue_problems_are_reported_not_swallowed() {
    let dir = temp_dir("squeue");
    copy_synthetic(&["opt_ok", "incomplete", "missing"], &dir);
    let empty_path = temp_dir("empty-path");

    // no user name at all
    let o = orcaout_with_queue(&[dir.to_str().unwrap()], &[]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("WARNING  squeue: neither USER nor LOGNAME is set"), "{err}");

    // squeue not installed
    let o = orcaout_with_queue(
        &[dir.to_str().unwrap()],
        &[("USER", "tester"), ("PATH", empty_path.to_str().unwrap())],
    );
    assert!(o.status.success(), "a missing squeue is not a program error");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("WARNING  squeue: `squeue` not found"), "{err}");
    let text = stdout(&o);
    assert!(text.contains("OK       opt_ok"), "{text}");
    assert!(text.contains("queue status unknown (squeue failed), job may still be running"), "{text}");
    assert!(text.contains("no .out file; queue status unknown"), "{text}");

    // --no-queue: no warning
    let o = orcaout(&[dir.to_str().unwrap()]);
    assert!(!String::from_utf8_lossy(&o.stderr).contains("squeue"));
}

//! Lists of keywords that ORCA accepts on the `!` line, used to recognise
//! the electronic-structure method and the orbital basis set of an input.
//!
//! All entries are lower case because the input parser lower-cases the
//! keyword line before looking anything up.  Extend these lists whenever a
//! new method or basis set shows up in your inputs; nothing else has to
//! change.

use std::collections::HashSet;

pub fn known_basis_sets() -> HashSet<&'static str> {
    [
        // =========================================================================
        // 1. KARLSRUHE / AHLRICHS BASIS SETS
        // =========================================================================
        // Older generation
        "sv", "svp", "sv(p)", "dz", "dzp", "tzv", "tzvp", "tzv(p)", "tzvpp", "qzv", "qzvp", "qzvpp",
        // def2 family
        "def2-sv(p)", "def2-svp", "def2-tzvp", "def2-tzvp(-f)", "def2-tzvpp", "def2-qzvp", "def2-qzvpp",
        // Diffuse def2 sets (Rappoport/Furche) and minimally augmented (Truhlar)
        "def2-svpd", "def2-tzvpd", "def2-tzvppd", "def2-qzvpd", "def2-qzvppd",
        "ma-def2-svp", "ma-def2-sv(p)", "ma-def2-tzvp", "ma-def2-tzvp(-f)", "ma-def2-tzvpp",
        "ma-def2-qzvp", "ma-def2-qzvpp",
        // Composite / minimal sets
        "def2-minis", "def2-mzvpp", "minix", "def2-mtzvp", "def2-mtzvpp", "def2-mqzvp", "def2-msvp", "vdzp",
        // Relativistically recontracted def2 sets
        "zora-def2-svp", "zora-def2-sv(p)", "zora-def2-tzvp", "zora-def2-tzvp(-f)", "zora-def2-tzvpp", "zora-def2-qzvpp",
        "dkh-def2-svp", "dkh-def2-sv(p)", "dkh-def2-tzvp", "dkh-def2-tzvp(-f)", "dkh-def2-tzvpp", "dkh-def2-qzvpp",
        "ma-zora-def2-svp", "ma-zora-def2-tzvp", "ma-zora-def2-tzvpp", "ma-zora-def2-qzvpp",
        "ma-dkh-def2-svp", "ma-dkh-def2-tzvp", "ma-dkh-def2-tzvpp", "ma-dkh-def2-qzvpp",
        "x2c-svpall", "x2c-svpall-2c", "x2c-tzvpall", "x2c-tzvpall-2c", "x2c-tzvppall", "x2c-tzvppall-2c",
        "x2c-qzvpall", "x2c-qzvpall-2c", "x2c-qzvppall", "x2c-qzvppall-2c",
        "x2c-svpall-s", "x2c-tzvpall-s", "x2c-tzvppall-s", "x2c-qzvpall-s", "x2c-qzvppall-s",

        // =========================================================================
        // 2. DUNNING CORRELATION-CONSISTENT BASIS SETS
        // =========================================================================
        "cc-pvdz", "cc-pvtz", "cc-pvqz", "cc-pv5z", "cc-pv6z",
        "aug-cc-pvdz", "aug-cc-pvtz", "aug-cc-pvqz", "aug-cc-pv5z", "aug-cc-pv6z",
        "cc-pcvdz", "cc-pcvtz", "cc-pcvqz", "cc-pcv5z",
        "aug-cc-pcvdz", "aug-cc-pcvtz", "aug-cc-pcvqz", "aug-cc-pcv5z",
        "cc-pwcvdz", "cc-pwcvtz", "cc-pwcvqz", "cc-pwcv5z",
        "aug-cc-pwcvdz", "aug-cc-pwcvtz", "aug-cc-pwcvqz", "aug-cc-pwcv5z",
        "cc-pvdz-f12", "cc-pvtz-f12", "cc-pvqz-f12",
        "cc-pvdz-pp", "cc-pvtz-pp", "cc-pvqz-pp", "cc-pv5z-pp",
        "aug-cc-pvdz-pp", "aug-cc-pvtz-pp", "aug-cc-pvqz-pp", "aug-cc-pv5z-pp",
        "cc-pvdz-dk", "cc-pvtz-dk", "cc-pvqz-dk", "cc-pv5z-dk",
        "aug-cc-pvdz-dk", "aug-cc-pvtz-dk", "aug-cc-pvqz-dk", "aug-cc-pv5z-dk",

        // =========================================================================
        // 3. POPLE BASIS SETS
        // =========================================================================
        "sto-3g", "3-21g", "3-21gsp", "4-22gsp", "6-31g", "6-31g*", "6-31g**", "6-31g(d)", "6-31g(d,p)",
        "6-31+g*", "6-31+g**", "6-31++g*", "6-31++g**", "6-31+g(d)", "6-31+g(d,p)", "6-31++g(d,p)",
        "6-311g", "6-311g*", "6-311g**", "6-311g(d)", "6-311g(d,p)", "6-311+g*", "6-311+g**",
        "6-311++g*", "6-311++g**", "6-311+g(d)", "6-311+g(d,p)", "6-311++g(d,p)", "6-311++g(2d,2p)",
        "6-311++g(3df,3pd)", "6-311g(2d,2p)", "6-311g(3df,3pd)",

        // =========================================================================
        // 4. SPECIALISED & RELATIVISTIC BASIS SETS
        // =========================================================================
        "sarc-zora-svp", "sarc-zora-tzvp", "sarc-zora-tzvpp", "sarc-dkh-svp", "sarc-dkh-tzvp", "sarc-dkh-tzvpp",
        "sarc2-zora-qzv", "sarc2-zora-qzvp", "sarc2-dkh-qzv", "sarc2-dkh-qzvp",
        // Jensen polarisation-consistent sets (for DFT)
        "pc-0", "pc-1", "pc-2", "pc-3", "pc-4",
        "aug-pc-0", "aug-pc-1", "aug-pc-2", "aug-pc-3", "aug-pc-4",
        "pcseg-0", "pcseg-1", "pcseg-2", "pcseg-3", "pcseg-4",
        "aug-pcseg-0", "aug-pcseg-1", "aug-pcseg-2", "aug-pcseg-3", "aug-pcseg-4",
        "pcsseg-0", "pcsseg-1", "pcsseg-2", "pcsseg-3", "pcsseg-4",
        // ANO sets
        "ano-pvdz", "ano-pvtz", "ano-pvqz", "ano-pv5z",
        "saug-ano-pvdz", "saug-ano-pvtz", "saug-ano-pvqz", "aug-ano-pvdz", "aug-ano-pvtz", "aug-ano-pvqz",
        "ano-rcc-full", "ano-rcc-dzp", "ano-rcc-tzp", "ano-rcc-qzp",
        // Partially augmented Dunning sets
        "jul-cc-pvdz", "jul-cc-pvtz", "jul-cc-pvqz", "jun-cc-pvdz", "jun-cc-pvtz", "jun-cc-pvqz",
        "may-cc-pvtz", "may-cc-pvqz", "apr-cc-pvqz",
        // Weigend/Dolg ECP sets
        "lanl2dz", "lanl2tz", "lanl2tz(f)", "lanl08", "lanl08(f)", "def2-ecp", "sdd",
    ]
    .into_iter()
    .collect()
}

pub fn known_methods() -> HashSet<&'static str> {
    [
        // FOD analysis is treated as a method of its own (see the Python original)
        "fod",
        // =========================================================================
        // 1. WAVEFUNCTION THEORY (WFT) & SEMI-EMPIRICAL BASE METHODS
        // =========================================================================
        "hf", "rhf", "uhf", "rohf", "hf-3c",
        "semiemp", "am1", "pm3", "mndo", "zindo", "zindo/s", "zindo/1", "zindo/2", "ndo",
        "xtb", "xtb0", "xtb1", "xtb2", "gfn-xtb", "gfn0-xtb", "gfn1-xtb", "gfn2-xtb", "gfn-ff",

        // =========================================================================
        // 2. DENSITY FUNCTIONAL THEORY (DFT)  (ORCA 6 manual, section "Choice of functional")
        // =========================================================================
        // --- Local density approximations (LDA) ---
        "hfs", "lda", "lsd", "vwn", "vwn3", "vwn5", "pwlda",
        // --- Generalised gradient approximations (GGA) ---
        "bp86", "bp", "blyp", "olyp", "glyp", "xlyp", "pw91", "mpwpw", "mpwlyp", "pbe", "rpbe", "revpbe",
        "rpw86pbe", "pwp", "mpbe", "b97", "b97-d", "b97-d3", "b97-d4", "hcth", "hcth407", "b97m-v", "b97m-d3bj", "b97m-d4",
        // --- Meta-GGA ---
        "tpss", "revtpss", "m06l", "m06-l", "mn15l", "mn15-l", "scan", "scanfunc", "r2scan", "rscan",
        // --- Hybrids ---
        "b1lyp", "b3lyp", "b3lyp/g", "o3lyp", "x3lyp", "b1p", "b3p", "b3pw", "b3pw91", "pw1pw", "mpw1pw", "mpw1lyp",
        "pbe0", "revpbe0", "revpbe38", "mpbe0", "b1pw91", "bhandhlyp", "bhlyp", "tpssh", "tpss0", "revtpssh",
        "m06", "m062x", "m06-2x", "mn15", "pw6b95", "b97-1", "b97-2", "b97-3",
        "r2scanh", "r2scan0", "r2scan50",
        // --- Range-separated hybrids ---
        "wr2scan", "wb97", "wb97x", "wb97x-d3", "wb97x-d4", "wb97x-d4rev", "wb97x-v", "wb97x-d3bj",
        "wb97m-v", "wb97m-d3bj", "wb97m-d4", "wb97m-d4rev", "cam-b3lyp", "lc-blyp", "lc-pbe", "lc-wpbe", "wpbe",
        // --- Double hybrids ---
        "b2plyp", "mpw2plyp", "b2gp-plyp", "b2k-plyp", "b2t-plyp", "pwpb95", "pbe-qidh", "pbe0-dh", "pbe0-2",
        "dsd-blyp", "dsd-pbep86", "dsd-pbeb95", "revdsd-pbep86/2021", "revdsd-pbep86-d4/2021",
        "revdod-pbep86/2021", "revdod-pbep86-d4/2021", "pr2scan50", "pr2scan69", "kpr2scan50",
        "wb2plyp", "wb2gp-plyp", "rsx-qidh", "rsx-0dh", "wb88pp86", "wpbepp86", "wb97m(2)", "wpr2scan50",
        "wb97x-2", "scs/sos-b2plyp21", "scs-pbe-qidh", "sos-pbe-qidh", "scs-b2gp-plyp21", "sos-b2gp-plyp21",
        "scs/sos-wb2plyp", "scs-wb2gp-plyp", "sos-wb2gp-plyp", "scs-rsx-qidh", "sos-rsx-qidh",
        "scs-wb88pp86", "sos-wb88pp86", "scs-wpbepp86", "sos-wpbepp86",

        // =========================================================================
        // 3. LOW-COST COMPOSITE METHODS (-3c family)
        // =========================================================================
        "hg-3c", "pbeh-3c", "b97-3c", "r2scan-3c", "wb97x-3c",

        // =========================================================================
        // 4. CORRELATED SINGLE-REFERENCE WAVEFUNCTION METHODS
        // =========================================================================
        "mp2", "ri-mp2", "scs-mp2", "sos-mp2", "oomp2", "oo-ri-mp2", "sos-oomp2", "mp3", "mp4", "mp2-f12", "ri-mp2-f12",
        "dlpno-mp2", "dlpno-scs-mp2", "dlpno-sos-mp2", "dlpno-mp2-f12",
        "ccsd", "ccsd(t)", "ccsd-f12", "ccsd(t)-f12", "cc2", "cc3", "ccsdt",
        "dlpno-ccsd", "dlpno-ccsd(t)", "dlpno-ccsd(t1)", "dlpno-ccsd(t)-f12", "dlpno-ccsd-f12",
        "lpno-ccsd", "lpno-ccsd(t)", "qcisd", "qcisd(t)", "cepa/1", "cepa/2", "cepa/3", "ncepa/1", "cpf", "cpf/1",
        "ri-ccsd", "ri-ccsd(t)", "bccd", "bccd(t)",

        // =========================================================================
        // 5. MULTI-REFERENCE METHODS
        // =========================================================================
        "casscf", "nevpt2", "sc-nevpt2", "fic-nevpt2", "dlpno-nevpt2", "fic-mrci", "fic-mrcc", "caspt2", "mrci", "mrci+q",
        "sorci", "mrddci2", "mrddci3", "mrmp2", "ice-ci",
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_are_lower_case() {
        for k in known_methods().iter().chain(known_basis_sets().iter()) {
            assert_eq!(*k, k.to_lowercase(), "keyword `{k}` must be lower case");
        }
    }

    #[test]
    fn methods_and_basis_sets_do_not_overlap() {
        let (m, b) = (known_methods(), known_basis_sets());
        let overlap: Vec<_> = m.intersection(&b).collect();
        assert!(overlap.is_empty(), "overlapping keywords: {overlap:?}");
    }

    #[test]
    fn contains_functionals_from_python_reference() {
        let m = known_methods();
        for f in ["wb97x-d4rev", "wb97x-v", "r2scan-3c", "b3lyp", "pbe0", "dlpno-ccsd(t)", "fod"] {
            assert!(m.contains(f), "missing method {f}");
        }
        let b = known_basis_sets();
        for s in ["def2-qzvppd", "def2-tzvp", "ma-def2-tzvp", "cc-pvtz", "6-311++g(d,p)"] {
            assert!(b.contains(s), "missing basis set {s}");
        }
    }
}

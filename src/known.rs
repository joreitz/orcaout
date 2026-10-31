use std::collections::HashSet;

pub fn known_basis_sets() -> HashSet<&'static str>{
    [
        // =========================================================================
        // 1. KARLSRUHER / AHLRICHS BASIS SETS
        // =========================================================================
        // Older Generation
        "sv", "svp", "sv(p)", "dz", "dzp", "tzv", "tzvp", "tzv(p)", "tzvpp", "qzv", "qzvpp",
        // Modern Generation
        "def2-sv(p)", "def2-svp", "def2-tzvp", "def2-tzvpp", "def2-qzvp", "def2-qzvpp",
        // Diffuse Sets
        "def2-minis", "def2-svpd", "def2-tzvpd", "def2-tzvppd", "def2-qzvpd", "def2-qzvppd",

        // =========================================================================
        // 2. DUNNING CORRELATION-CONSISTENT BASISSÄTZE
        // =========================================================================
        // Standard (cc-pVXZ)
        "cc-pvdz", "cc-pvtz", "cc-pvqz", "cc-pv5z", "cc-pv6z",
        // Augmented (aug-cc-pVXZ)
        "aug-cc-pvdz", "aug-cc-pvtz", "aug-cc-pvqz", "aug-cc-pv5z", "aug-cc-pv6z",
        // Core-Valence Varianten
        "cc-pcvdz", "cc-pcvtz", "cc-pcvqz", "cc-pcv5z",
        "aug-cc-pcvdz", "aug-cc-pcvtz", "aug-cc-pcvqz", "aug-cc-pcv5z",
        // tight-d Effects
        "cc-pvdz-f12", "cc-pvtz-f12", "cc-pvqz-f12",

        // =========================================================================
        // 3. POPLE BASIS SETS
        // =========================================================================
        "3-21g", "6-31g", "6-31g*", "6-31g**", "6-31+g*", "6-31+g**", "6-311g", 
        "6-311g*", "6-311g**", "6-311+g*", "6-311+g**",

        // =========================================================================
        // 4. SPEZIALISIERTE & RELATIVISTISCHE BASISSÄTZE
        // =========================================================================
        // Relativistische Sätze (DKH / ZORA)
        "sarc-zora-j", "sarc-zora-tzvp", "sarc-dkh-tzvp",
        // Composite / minimally augmented
        "def2-mzvpp", "minix", "def2-mtzvp", "def2-mqzvp", "def2-msvp",
        // Jensen Polarisation-Konsistente Sätze (für DFT)
        "pc-0", "pc-1", "pc-2", "pc-3", "pc-4",
        "aug-pc-0", "aug-pc-1", "aug-pc-2", "aug-pc-3", "aug-pc-4"
    ].into_iter().collect()
}

pub fn known_methods() -> HashSet<&'static str>{
    [
        // Only Exception FOD 
        "fod",
        // =========================================================================
        // 1. WAVEFUNCTION THEORY (WFT) & SEMI-EMPIRICAL BASE METHODS
        // =========================================================================
        "hf",         // Hartree-Fock (RHF/UHF/ROHF)
        "semiemp",    // Semi-empirical flag
        "am1",        // Austin Model 1
        "pm3",        // Parameterized Model 3
        "zindo",      // Zerner's Intermediate Neglect of Differential Overlap

        // =========================================================================
        // 2. DENSITY FUNCTIONAL THEORY (DFT)
        // =========================================================================
        
        // --- Local Density Approximations (LDA) ---
        "hfs",        // Hartree-Fock Slater
        "lsd",        // Local Spin Density
        "vwn3",       // Vosko, Wilk, and Nusair (Variant 3)
        "vwn5",       // Vosko, Wilk, and Nusair (Variant 5)
        "pwlda",      // Perdew-Wang LDA

        // --- Generalized Gradient Approximations (GGA) ---
        "bp86",       // Becke-Perdew 86
        "blyp",       // Becke-Lee-Yang-Parr
        "pbe",        // Perdew-Burke-Ernzerhof
        "mpbe",       // Modified PBE
        "pw91",       // Perdew-Wang 91
        "olyp",       // Handy-Cohen exchange with LYP correlation
        "xlyp",       // Xu-Goddard exchange with LYP correlation
        "rpbe",       // Revised PBE (Hammer-Hansen-Norskov)
        "revpbe",     // Revised PBE (Zhang-Yang)

        // --- Meta-GGA Functionals ---
        "tpss",       // Tao-Perdew-Staroverov-Scuseria
        "revtpss",    // Revised TPSS
        "m06l",       // Minnesota 2006 local functional
        "mn15l",      // Minnesota 2015 local functional
        "scan",       // Strongly Constrained and Appropriately Normed
        "r2scan",     // Regularized Restored SCAN

        // --- Hybrid Functionals ---
        "b3lyp",      // Becke 3-parameter Hybrid (with VWN5)
        "b3lyp/g",    // Becke 3-parameter Hybrid (with VWN1-RPA)
        "b3pw91",     // Becke 3-parameter Hybrid with PW91
        "pbe0",       // PBE Hybrid (25% HF exchange)
        "mpbe0",      // Modified PBE Hybrid
        "b1lyp",      // Becke 1-parameter Hybrid with LYP
        "b1pw91",     // Becke 1-parameter Hybrid with PW91
        "o3lyp",      // Cohen-Handy 3-parameter Hybrid with LYP
        "x3lyp",      // Extended 3-parameter Hybrid
        "tpssh",      // TPSS Hybrid (10% HF exchange)
        "revtpssh",   // Revised TPSS Hybrid
        "pw6b95",     // Zhao-Truhlar hybrid meta-GGA
        "m06",        // Minnesota 2006 hybrid (27% HF exchange)
        "m062x",      // Minnesota 2006 hybrid (54% HF exchange)
        "mn15",       // Minnesota 2015 hybrid

        // --- Range-Separated Hybrids ---
        "cam-b3lyp",  // Coulomb Attenuating Method B3LYP
        "wb97",       // Chai-Head-Gordon range-separated functional
        "wb97x",      // wB97 with extra Hartree-Fock exchange
        "wb97x-d3",   // wB97X with Grimme's D3 dispersion
        "wb97m-v",    // Range-separated hybrid meta-GGA with VV10 dispersion
        "lc-blyp",    // Long-range Corrected BLYP

        // --- Double-Hybrid Functionals (DH-DFT) ---
        "b2plyp",     // Becke 2-parameter Hybrid with MP2 correlation
        "mpw2plyp",   // Modified PW91 2-parameter Hybrid with MP2
        "b2gp-plyp",  // General Purpose B2PLYP variant
        "pbe0-dh",    // PBE0 Double Hybrid
        "dsd-pbep86", // Dispersion-corrected Spin-Component-Scaled Double Hybrid
        "wb97x-2",    // Range-separated Double Hybrid

        // =========================================================================
        // 3. LOW-COST COMPOSITE METHODS (-3c family)
        // =========================================================================
        "hg-3c",      // Hartree-Fock with 3 corrections (gCP, D3, SRB)
        "pbeh-3c",    // Hybrid-PBE composite scheme
        "b97-3c",     // B97-based composite scheme
        "r2scan-3c",  // r2SCAN-based low-cost composite scheme
        "wb97x-3c",   // Range-separated hybrid composite scheme

        // =========================================================================
        // 4. CORRELATED SINGLE-REFERENCE WAVEFUNCTION METHODS
        // =========================================================================
        
        // --- Møller–Plesset Perturbation Theory ---
        "mp2",           // 2nd-order Møller-Plesset perturbation theory
        "ri-mp2",        // MP2 with Resolution of the Identity
        "scs-mp2",       // Spin-Component-Scaled MP2
        "sos-mp2",       // Scaled Opposite-Spin MP2
        "oomp2",         // Orbital-Optimized MP2
        "sos-oomp2",     // Scaled Opposite-Spin Orbital-Optimized MP2
        "mp3",           // 3rd-order Møller-Plesset perturbation theory
        "mp4",           // 4th-order Møller-Plesset perturbation theory (SDQ/SDTQ)

        // --- Coupled Cluster & Single Reference Methods ---
        "ccsd",          // Coupled Cluster Singles and Doubles
        "ccsd(t)",       // CCSD with perturbative Triples
        "dlpno-mp2",     // Domain-based Local Pair Natural Orbital MP2
        "dlpno-ccsd",    // Domain-based Local Pair Natural Orbital CCSD
        "dlpno-ccsd(t)", // DLPNO-CCSD with perturbative Triples (Gold Standard)
        "qcisd",         // Quadratic Configuration Interaction Singles and Doubles
        "qcisd(t)",      // QCISD with perturbative Triples
        "cepa/1",        // Coupled Electron Pair Approximation variant 1
        "cpf",           // Coupled Pair Functional

        // =========================================================================
        // 5. MULTI-REFERENCE METHODS
        // =========================================================================
        "casscf",        // Complete Active Space Self-Consistent Field
        "nevpt2",        // n-Electron Valence State Perturbation Theory (2nd order)
        "fic-mrci",      // Internally Contracted Multireference Configuration Interaction
        "caspt2",        // Complete Active Space Perturbation Theory (2nd order)
    ].into_iter().collect()
}

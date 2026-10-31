#!/usr/bin/env bash
# Creates a folder test_inputs/ with small ORCA inputs for testing job type detection.
# The first line of each file states the expected result (as an ORCA comment).
set -euo pipefail

dir="test_inputs"
mkdir -p "$dir"
cd "$dir"

geom='* xyz 0 1
H 0.0 0.0 0.0
H 0.0 0.0 0.74
*'

# --- plain cases -------------------------------------------------------------
cat > sp.inp <<EOF
# expected: Sp
! r2scan-3c
$geom
EOF

cat > opt_end_of_line.inp <<EOF
# expected: Opt   (opt is the last token; the Python version detected Sp here)
! wb97x-3c opt
$geom
EOF

cat > tightopt.inp <<EOF
# expected: Opt   (synonym)
! r2scan-3c TightOpt
$geom
EOF

cat > freq.inp <<EOF
# expected: Freq
! r2scan-3c freq
$geom
EOF

# --- combinations and formatting ----------------------------------------------
cat > optfreq_two_bang_lines.inp <<EOF
# expected: Optfreq   (keywords spread over two ! lines)
! wB97X-3c TightOpt
! Freq
$geom
EOF

cat > optfreq_no_space_after_bang.inp <<EOF
# expected: Optfreq   (no space after !)
!r2scan-3c opt freq
$geom
EOF

cat > optfreq_uppercase.inp <<EOF
# expected: Optfreq   (upper case)
! R2SCAN-3C OPT FREQ
$geom
EOF

cat > freq_after_blocks.inp <<EOF
# expected: Freq   (! line is not the first line)
%pal nprocs 4 end
%maxcore 2000
! r2scan-3c freq
$geom
EOF

cat > opt_in_comment.inp <<EOF
# expected: Sp   (opt only appears in a comment)
! r2scan-3c   # opt freq later
$geom
EOF

cat > scan_in_comment.inp <<EOF
# expected: Opt   (scan only appears in a comment)
! r2scan-3c opt
# scan maybe later
$geom
EOF

# --- block-based detection -----------------------------------------------------
cat > scan.inp <<EOF
# expected: Scan   (contains opt, but the scan takes precedence)
! r2scan-3c opt
%geom
  Scan
    B 0 1 = 0.7, 1.5, 9
  end
end
$geom
EOF

cat > tddft.inp <<EOF
# expected: Tddft
! wb97x-d3 def2-tzvp
%tddft
  nroots 10
end
$geom
EOF

cat > smeartemp.inp <<EOF
# expected: Ftdft   (Fermi smearing via block)
! r2scan-3c
%scf
  SmearTemp 5000
end
$geom
EOF

cat > fod.inp <<EOF
# expected: Ftdft   (FOD keyword)
! fod
$geom
EOF

cat > nmr.inp <<EOF
# expected: Nmr
! pbe0 def2-tzvp nmr
$geom
EOF

# --- edge cases -----------------------------------------------------------------
cat > no_bang_line.inp <<EOF
# expected: Invalid   (no ! line at all)
%method
  method dft
  functional pbe0
end
$geom
EOF

cat > irc.inp <<EOF
# expected: not supported   (currently falls through to Sp)
! r2scan-3c irc
$geom
EOF

cat > xyzfile.inp <<EOF
# expected: Opt   (coordinates from an external file)
! r2scan-3c opt
* xyzfile 0 1 h2.xyz
EOF

# --- entries that must be ignored ---------------------------------------------
mkdir -p folder_named_like.inp   # directory with .inp extension
touch README                      # file without extension
touch sp.out                      # other extension

echo "Created test inputs in $(pwd)"

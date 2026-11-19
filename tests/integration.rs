use std::io::Write;
use std::process::{Command, Output, Stdio};

fn seqcol() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_seqcol"));
    // Ensure consistent color output regardless of terminal environment
    cmd.env("COLORTERM", "truecolor").env_remove("TERM");
    cmd
}

#[test]
fn test_short_help() {
    let help = |flag| String::from_utf8(seqcol().arg(flag).env_remove("COLUMNS").output().unwrap().stdout).unwrap();
    let (short, long) = (help("-h"), help("--help"));
    assert!(short.lines().count() < long.lines().count());
    // Each argument entry starts with "  -" or "  [", so a line starting with more indentation continues an entry.
    let entries = short.lines().skip_while(|line| *line != "Arguments:").filter(|line| line.starts_with("  "));
    for line in entries {
        assert!(!line.starts_with("   "), "help of an argument continues on the next line: {line:?}");
    }
}

#[test]
fn test_custom_colorscheme() {
    let output = seqcol()
        .args(["-s", "tests/data/custom.csv", "tests/data/custom.fa"])
        .output()
        .expect("Failed to execute seqcol");

    assert!(output.status.success(), "seqcol failed: {:?}", output);

    let expected = std::fs::read("tests/expected/custom_colorscheme.txt")
        .expect("Failed to read expected output");
    assert_eq!(output.stdout, expected);
}

#[test]
fn test_unreadable_file_continues_and_fails() {
    // Open error and read error, each streaming and non-streaming (-T).
    for flags in [vec!["-p", "never"], vec!["-p", "never", "-T"]] {
        let existing_only = seqcol()
            .args(&flags)
            .arg("tests/data/custom.fa")
            .output()
            .unwrap();
        assert!(!existing_only.stdout.is_empty());

        for unreadable in ["missing.fa", "tests/data"] {
            let output = seqcol()
                .args(&flags)
                .args([unreadable, "tests/data/custom.fa"])
                .output()
                .expect("Failed to execute seqcol");

            assert_eq!(output.status.code(), Some(1), "{unreadable}: {output:?}");
            assert!(String::from_utf8_lossy(&output.stderr).contains(unreadable));
            assert_eq!(output.stdout, existing_only.stdout);
        }
    }
}

#[test]
fn test_missing_file_fails_after_closed_pipe() {
    let mut child = seqcol()
        .args(["-p", "never", "missing.fa", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child.stdin.take().unwrap().write_all(b"ACGT\n").unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(1));
}

#[test]
fn test_transpose_helix_propensity() {
    let output = seqcol()
        .args(["tests/data/aln_picorna.fa1", "-Ts", "helix_propensity"])
        .output()
        .expect("Failed to execute seqcol");

    assert!(output.status.success(), "seqcol failed: {:?}", output);

    let expected = std::fs::read("tests/expected/transpose_helix_propensity.txt")
        .expect("Failed to read expected output");
    assert_eq!(output.stdout, expected);
}

#[test]
fn test_foreground_consensus() {
    let output = seqcol()
        .args([
            "tests/data/ebola_virus_reduced_align.fa1",
            "-s", "",
            "-S", "hydrophobicity",
            "-c", "128 128 128",
        ])
        .output()
        .expect("Failed to execute seqcol");

    assert!(output.status.success(), "seqcol failed: {:?}", output);

    let expected = std::fs::read("tests/expected/foreground_consensus.txt")
        .expect("Failed to read expected output");
    assert_eq!(output.stdout, expected);
}

#[test]
fn test_foreground_mutations() {
    let output = seqcol()
        .args([
            "tests/data/ebola_virus_reduced_align.fa1",
            "-s", "",
            "-S", "hydrophobicity",
            "-C", "128 128 128",
        ])
        .output()
        .expect("Failed to execute seqcol");

    assert!(output.status.success(), "seqcol failed: {:?}", output);

    let expected = std::fs::read("tests/expected/foreground_mutations.txt")
        .expect("Failed to read expected output");
    assert_eq!(output.stdout, expected);
}

#[test]
fn test_consensus_color_depth() {
    for depth in ["16", "256"] {
        for flag in ["-c", "-C"] {
            let output = seqcol()
                .args(["-p", "never", "--colors", depth, flag, "128 128 128", "tests/data/ebola_virus_reduced_align.fa1"])
                .output()
                .expect("Failed to execute seqcol");
            assert!(output.status.success(), "seqcol failed: {:?}", output);
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(!stdout.contains("38;2;") && !stdout.contains("48;2;"), "--colors {depth} {flag}: RGB code in output");
        }
    }
}

/// Output size limits keep pager scrolling fast. Lower a limit when the output gets smaller.
#[test]
fn test_output_size() {
    let cases: [(&[&str], usize); 8] = [
        (&["-s", "hydrophobicity"], 36612),
        (&["--colors", "256", "-s", "hydrophobicity"], 22811),
        (&["--colors", "16", "-s", "hydrophobicity"], 11854),
        (&["-S", "hydrophobicity", "-c", "gray"], 41243),
        (&["-S", "hydrophobicity", "-c", "bold"], 39038),
        (&["-s", "hydrophobicity", "-c", "bold"], 40597),
        (&["--colors", "16", "-s", "hydrophobicity", "-c", "bold"], 16052),
        (&["-T", "-s", "hydrophobicity"], 59413),
    ];
    for (flags, max_bytes) in cases {
        let output = seqcol()
            .args(["-p", "never", "tests/data/ebola_virus_reduced_align.fa1"])
            .args(flags)
            .output()
            .expect("Failed to execute seqcol");
        assert!(output.status.success(), "{flags:?} failed: {output:?}");
        let n_bytes = output.stdout.len();
        assert!(n_bytes <= max_bytes, "{flags:?}: {n_bytes} bytes, limit {max_bytes}");
    }
}

#[test]
fn test_paging_never() {
    // With -p never, output should go directly to stdout (same as default in non-TTY)
    let output = seqcol()
        .args(["-s", "tests/data/custom.csv", "-p", "never", "tests/data/custom.fa"])
        .output()
        .expect("Failed to execute seqcol");

    assert!(output.status.success(), "seqcol failed: {:?}", output);

    let expected = std::fs::read("tests/expected/custom_colorscheme.txt")
        .expect("Failed to read expected output");
    assert_eq!(output.stdout, expected);
}

#[test]
fn test_paging_env_var() {
    // Test that SEQCOL_PAGING env var works
    let output = seqcol()
        .env("SEQCOL_PAGING", "never")
        .args(["-s", "tests/data/custom.csv", "tests/data/custom.fa"])
        .output()
        .expect("Failed to execute seqcol");

    assert!(output.status.success(), "seqcol failed: {:?}", output);

    let expected = std::fs::read("tests/expected/custom_colorscheme.txt")
        .expect("Failed to read expected output");
    assert_eq!(output.stdout, expected);
}

#[test]
fn test_paging_invalid_value() {
    // Invalid paging value should produce an error
    let output = seqcol()
        .args(["-s", "tests/data/custom.csv", "-p", "invalid", "tests/data/custom.fa"])
        .output()
        .expect("Failed to execute seqcol");

    assert!(!output.status.success(), "seqcol should have failed with invalid paging value");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value"),"Error message should mention invalid paging value: {}", stderr);
}

// Sequence type tests

/// Background code of A in the taylor colors of nucleotides and of amino acids.
const TAYLOR_NUCL_A: &str = "48;2;102;255;0;";
const TAYLOR_AA_A: &str = "48;2;204;255;0;";

/// Run seqcol without a pager, assert success, and return stdout.
fn seqcol_ok(args: &[&str]) -> String {
    let output = seqcol().args(["-p", "never"]).args(args).output().expect("Failed to execute seqcol");
    assert!(output.status.success(), "{args:?} failed: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn test_scheme_colors_protein_with_aa_colors() {
    let file = "tests/data/ebola_virus_reduced_align.fa1";
    assert_eq!(seqcol_ok(&["-s", "taylor", file]), seqcol_ok(&["-s", "taylor", "-a", "aa", file]));
}

#[test]
fn test_scheme_colors_dna_with_nucl_colors() {
    for file in ["tests/data/custom.fa", "tests/data/custom.fa.gz"] {
        assert_eq!(seqcol_ok(&["-s", "taylor", file]), seqcol_ok(&["-s", "taylor", "-a", "nucl", file]), "{file}");
    }
}

#[test]
fn test_scheme_for_other_type_leaves_text_uncolored() {
    let stdout = seqcol_ok(&["-s", "hydrophobicity", "tests/data/custom.fa"]);
    assert!(!stdout.contains("48;"), "{stdout:?}");
}

#[test]
fn test_short_first_line_takes_type_of_later_lines() {
    // The first sequence line has one letter, A, and the header must not end the read-ahead.
    let stdout = seqcol_ok(&["-s", "taylor", "tests/data/wrapped_aa.fa"]);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines[1].contains(TAYLOR_AA_A), "{stdout:?}");
    assert_eq!(stdout, seqcol_ok(&["-s", "taylor", "-a", "aa", "tests/data/wrapped_aa.fa"]));
}

#[test]
fn test_alphabet_flag_forces_type() {
    let stdout = seqcol_ok(&["-s", "taylor", "-a", "aa", "tests/data/custom.fa"]);
    assert!(stdout.contains(TAYLOR_AA_A) && !stdout.contains(TAYLOR_NUCL_A), "{stdout:?}");
}

#[test]
fn test_each_file_has_its_own_type() {
    let (dna, prot) = ("tests/data/custom.fa", "tests/data/wrapped_aa.fa");
    let expected = seqcol_ok(&["-s", "taylor", "-a", "nucl", dna]) + &seqcol_ok(&["-s", "taylor", "-a", "aa", prot]);
    assert_eq!(seqcol_ok(&["-s", "taylor", dna, prot]), expected);
}

#[test]
fn test_table_columns_have_their_own_type() {
    for (scheme, protein_colored) in [("taylor", true), ("jalview", false)] {
        let stdout = seqcol_ok(&["-s", scheme, "-m", "8", "tests/data/mixed.tsv"]);
        for row in stdout.lines().skip(1) {
            let cells: Vec<&str> = row.split('\t').collect();
            assert!(cells[1].contains("48;2;"), "{scheme}: {row:?}");
            assert_eq!(cells[2].contains("48;2;"), protein_colored, "{scheme}: {row:?}");
            if scheme == "taylor" {
                assert!(cells[1].contains(TAYLOR_NUCL_A) && cells[2].contains(TAYLOR_AA_A), "{row:?}");
            }
        }
    }
}

#[test]
fn test_schemes_of_one_type_combine() {
    let stdout = seqcol_ok(&["-s", "jalview", "-s", "clustal", "-m", "8", "tests/data/mixed.tsv"]);
    let jalview_a = "48;2;100;247;63;";
    let clustal_a = "48;2;128;160;240;";
    for row in stdout.lines().skip(1) {
        let cells: Vec<&str> = row.split('\t').collect();
        assert!(cells[1].contains(jalview_a) && cells[2].contains(clustal_a), "{row:?}");
    }
}

#[test]
fn test_fastq_sequence_lines_are_nucleotides() {
    let stdout = seqcol_ok(&["-s", "taylor", "tests/data/reads.fq"]);
    for sequence_line in stdout.lines().skip(1).step_by(4) {
        assert!(sequence_line.contains(TAYLOR_NUCL_A), "{stdout:?}");
    }
}

#[test]
fn test_scheme_on_stdin() {
    let output = seqcol_stdin(&["-p", "never", "-s", "taylor"], b">a\nACGT\n");
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8(output.stdout).unwrap().contains(TAYLOR_NUCL_A));
}

/// Run `script` in bash with `$0` set to the seqcol binary, assert success, and return stdout.
fn bash_ok(script: &str) -> String {
    let output = Command::new("bash")
        .args(["-c", script, env!("CARGO_BIN_EXE_seqcol")])
        .env("COLORTERM", "truecolor")
        .env_remove("TERM")
        .output()
        .expect("Failed to execute bash");
    assert!(output.status.success(), "{script}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn test_fifo_input_and_scheme_are_read() {
    let stdout = bash_ok(r#""$0" -p never -s taylor <(printf '>a\nACGT\n')"#);
    assert!(stdout.contains(TAYLOR_NUCL_A), "{stdout:?}");
    let stdout = bash_ok(r#""$0" -p never -s <(printf 'A\t#ff0000\n') <(printf 'ACGT\n')"#);
    assert!(stdout.contains("48;2;255;0;0;"), "{stdout:?}");
}

/// Run seqcol with `args` and `input` on stdin, and capture stdout and stderr.
fn seqcol_stdin(args: &[&str], input: &[u8]) -> Output {
    seqcol()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().unwrap().write_all(input)?;
            child.wait_with_output()
        })
        .expect("Failed to execute seqcol")
}

#[test]
fn test_gzip_input() {
    let expected = std::fs::read("tests/expected/custom_colorscheme.txt")
        .expect("Failed to read expected output");
    let args = ["-s", "tests/data/custom.csv", "-p", "never"];
    let gz = std::fs::read("tests/data/custom.fa.gz").expect("Failed to read gzip fixture");

    let file_output = seqcol()
        .args(args)
        .arg("tests/data/custom.fa.gz")
        .output()
        .expect("Failed to execute seqcol");
    let stdin_output = seqcol_stdin(&args, &gz);

    for output in [file_output, stdin_output] {
        assert!(output.status.success(), "seqcol failed: {:?}", output);
        assert_eq!(output.stdout, expected);
    }
}

#[test]
fn test_highlight_attributes() {
    for (flags, code) in [(["-c", "bold"], "\x1b[1;"), (["-C", "underline"], "\x1b[4;")] {
        let output = seqcol_stdin(
            &[&["-p", "never", "-s", "taylor"], &flags[..]].concat(),
            b"ACGT\nACGA\nACGA\n",
        );
        assert!(output.status.success(), "seqcol failed: {:?}", output);
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains(code), "{flags:?} must write {code:?}: {stdout:?}");
    }
}

#[test]
fn test_invalid_highlight_style() {
    let output = seqcol_stdin(&["-p", "never", "-c", "nocolor"], b"ACGT\n");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unknown color name."), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn test_invalid_utf8_is_kept() {
    let output = seqcol_stdin(&["-p", "never", "-s", "taylor"], b">caf\xe9\nAC\xffGT\n");
    assert!(output.status.success(), "seqcol failed: {:?}", output);
    let header = b">caf\xe9\n";
    assert!(output.stdout.windows(header.len()).any(|w| w == header), "{:?}", output.stdout);
    let colored_after_byte = b"\xff\x1b[";
    assert!(output.stdout.windows(3).any(|w| w == colored_after_byte), "{:?}", output.stdout);
}

#[test]
fn test_min_length_colors_only_long_sequence_runs() {
    // -m must measure a run of sequence letters, not the line.
    let output = seqcol()
        .args(["-p", "never", "-s", "taylor", "-m", "8", "tests/data/table.tsv"])
        .output()
        .expect("Failed to execute seqcol");

    assert!(output.status.success(), "seqcol failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "\x1b[0mid\tname\tseq");
    assert!(lines[1].starts_with("\x1b[0m1\tsample_one\t\x1b["), "{stdout:?}");
    assert_eq!(lines[2], "\x1b[0m2\tfoo\tTTGACCA");
    assert!(lines[3].starts_with("\x1b[0m3\tbar\t\x1b["), "N must not split the run: {stdout:?}");

    let dna_output = seqcol()
        .args(["-p", "never", "-s", "taylor", "-a", "dna", "-m", "8", "tests/data/table.tsv"])
        .output()
        .expect("Failed to execute seqcol");
    assert_eq!(stdout.as_bytes(), dna_output.stdout, "must color DNA, and N must be in the dna alphabet");
}

#[test]
fn test_regex_and_min_length_with_offset_match() {
    let output = seqcol_stdin(
        &["-p", "never", "-s", "taylor", "-r", "ACGT.*", "-m", "3"],
        b"xxxxxxxxACGTACGT\n",
    );

    assert!(output.status.success(), "seqcol failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.starts_with("\x1b[0mxxxxxxxx\x1b["),
        "styling must start at the match: {stdout:?}"
    );
}

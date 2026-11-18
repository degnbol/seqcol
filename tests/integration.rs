use std::io::Write;
use std::process::{Command, Output, Stdio};

fn seqcol() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_seqcol"));
    // Ensure consistent color output regardless of terminal environment
    cmd.env("COLORTERM", "truecolor").env_remove("TERM");
    cmd
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
fn test_missing_file_continues_and_fails() {
    // Streaming, and non-streaming (-T).
    for flags in [vec!["-p", "never"], vec!["-p", "never", "-T"]] {
        let output = seqcol()
            .args(&flags)
            .args(["missing.fa", "tests/data/custom.fa"])
            .output()
            .expect("Failed to execute seqcol");

        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("missing.fa"));
        let existing_only = seqcol()
            .args(&flags)
            .arg("tests/data/custom.fa")
            .output()
            .unwrap();
        assert!(!existing_only.stdout.is_empty());
        assert_eq!(output.stdout, existing_only.stdout);
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
        .args(["tests/data/aln_picorna.fa1", "-Ts", "helix_propensity_aa"])
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
            "-S", "hydrophobicity_aa",
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
            "-S", "hydrophobicity_aa",
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
        (&["-s", "hydrophobicity_aa"], 48193),
        (&["--colors", "256", "-s", "hydrophobicity_aa"], 48928),
        (&["--colors", "16", "-s", "hydrophobicity_aa"], 23326),
        (&["-S", "hydrophobicity_aa", "-c", "gray"], 41243),
        (&["-S", "hydrophobicity_aa", "-c", "bold"], 39038),
        (&["-s", "hydrophobicity_aa", "-c", "bold"], 40597),
        (&["--colors", "16", "-s", "hydrophobicity_aa", "-c", "bold"], 16052),
        (&["-T", "-s", "hydrophobicity_aa"], 59413),
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

// Auto-detection tests

#[test]
fn test_auto_detect_aa_with_base_name() {
    // Using base name 'taylor' with protein input should auto-select taylor_aa
    // Compare output with explicit taylor_aa to verify auto-detection works
    let auto_output = seqcol()
        .args(["-s", "taylor", "tests/data/ebola_virus_reduced_align.fa1"])
        .output()
        .expect("Failed to execute seqcol with auto-detect");

    let explicit_output = seqcol()
        .args(["-s", "taylor_aa", "tests/data/ebola_virus_reduced_align.fa1"])
        .output()
        .expect("Failed to execute seqcol with explicit scheme");

    assert!(auto_output.status.success(), "seqcol auto-detect failed: {:?}", auto_output);
    assert!(explicit_output.status.success(), "seqcol explicit failed: {:?}", explicit_output);
    assert_eq!(auto_output.stdout, explicit_output.stdout,
        "Auto-detected taylor should produce same output as explicit taylor_aa");
}

#[test]
fn test_auto_detect_dna_with_base_name() {
    // custom.fa contains DNA sequences (ACGT), should auto-select taylor_nucl
    let auto_output = seqcol()
        .args(["-s", "taylor", "tests/data/custom.fa"])
        .output()
        .expect("Failed to execute seqcol with auto-detect");

    let explicit_output = seqcol()
        .args(["-s", "taylor_nucl", "tests/data/custom.fa"])
        .output()
        .expect("Failed to execute seqcol with explicit scheme");

    assert!(auto_output.status.success(), "seqcol auto-detect failed: {:?}", auto_output);
    assert!(explicit_output.status.success(), "seqcol explicit failed: {:?}", explicit_output);
    assert_eq!(auto_output.stdout, explicit_output.stdout,
        "Auto-detected taylor should produce same output as explicit taylor_nucl for DNA");
}

#[test]
fn test_auto_detect_error_no_matching_suffix() {
    // hydrophobicity only has _aa suffix, using with DNA should error with suggestion
    let output = seqcol()
        .args(["-s", "hydrophobicity", "tests/data/custom.fa"])
        .output()
        .expect("Failed to execute seqcol");

    assert!(!output.status.success(), "seqcol should have failed for missing nucleotide scheme");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("hydrophobicity_nucl"),
        "Error should mention the attempted scheme name: {}", stderr);
    assert!(stderr.contains("Available:"),
        "Error should mention available alternatives: {}", stderr);
    assert!(stderr.contains("hydrophobicity_aa"),
        "Error should suggest hydrophobicity_aa: {}", stderr);
}

#[test]
fn test_alphabet_flag_overrides_auto_detect() {
    // With -a aa flag, should use _aa suffix even though we're not reading a file
    // This tests that -a flag takes priority over auto-detection
    let output = seqcol()
        .args(["-s", "taylor", "-a", "aa", "tests/data/custom.fa"])
        .output()
        .expect("Failed to execute seqcol");

    let explicit_output = seqcol()
        .args(["-s", "taylor_aa", "tests/data/custom.fa"])
        .output()
        .expect("Failed to execute seqcol with explicit scheme");

    assert!(output.status.success(), "seqcol with -a aa failed: {:?}", output);
    assert_eq!(output.stdout, explicit_output.stdout,
        "-a aa should override auto-detection and use taylor_aa");
}

#[test]
fn test_explicit_suffix_still_works() {
    // Explicit full name should still work (backward compatibility)
    let output = seqcol()
        .args(["-s", "hydrophobicity_aa", "tests/data/ebola_virus_reduced_align.fa1"])
        .output()
        .expect("Failed to execute seqcol");

    assert!(output.status.success(), "Explicit colorscheme name should work: {:?}", output);
    assert!(!output.stdout.is_empty(), "Should produce colored output");
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
fn test_highlight_attributes() {
    for (flags, code) in [(["-c", "bold"], "\x1b[1;"), (["-C", "underline"], "\x1b[4;")] {
        let output = seqcol_stdin(
            &[&["-p", "never", "-s", "taylor_nucl"], &flags[..]].concat(),
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
    let output = seqcol_stdin(&["-p", "never", "-s", "taylor_nucl"], b">caf\xe9\nAC\xffGT\n");
    assert!(output.status.success(), "seqcol failed: {:?}", output);
    let header = b">caf\xe9\n";
    assert!(output.stdout.windows(header.len()).any(|w| w == header), "{:?}", output.stdout);
    let colored_after_byte = b"\xff\x1b[";
    assert!(output.stdout.windows(3).any(|w| w == colored_after_byte), "{:?}", output.stdout);
}

#[test]
fn test_regex_and_min_length_with_offset_match() {
    let output = seqcol_stdin(
        &["-p", "never", "-s", "taylor_nucl", "-r", "ACGT.*", "-m", "3"],
        b"xxxxxxxxACGTACGT\n",
    );

    assert!(output.status.success(), "seqcol failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.starts_with("\x1b[0mxxxxxxxx\x1b["),
        "styling must start at the match: {stdout:?}"
    );
}

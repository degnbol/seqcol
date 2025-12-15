use std::process::Command;

fn seqcol() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_seqcol"));
    // Ensure consistent color output regardless of terminal environment
    cmd.env("COLORTERM", "truecolor");
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
    let output = Command::new(env!("CARGO_BIN_EXE_seqcol"))
        .env("COLORTERM", "truecolor")
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
    assert!(stderr.contains("Invalid paging value"), "Error message should mention invalid paging value: {}", stderr);
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

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn seqcol() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_seqcol"));
    // Ensure consistent color output regardless of terminal environment
    cmd.env("COLORTERM", "truecolor").env_remove("TERM").env_remove("NO_COLOR");
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
        (&["-s", "hydrophobicity"], 36584),
        (&["--colors", "256", "-s", "hydrophobicity"], 22783),
        (&["--colors", "16", "-s", "hydrophobicity"], 11826),
        (&["-S", "hydrophobicity", "-c", "gray"], 41215),
        (&["-S", "hydrophobicity", "-c", "bold"], 39010),
        (&["-s", "hydrophobicity", "-c", "bold"], 40569),
        (&["--colors", "16", "-s", "hydrophobicity", "-c", "bold"], 16024),
        (&["-T", "-s", "hydrophobicity"], 20574),
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

#[test]
fn test_coloring_colored_output_is_unchanged() {
    let colored = seqcol_ok(&["-s", "taylor", "tests/data/ebola_virus_reduced_align.fa1"]);
    let recolor = |flags: &[&str]| {
        let output = seqcol_stdin(&[&["-p", "never", "-s", "taylor"], flags].concat(), colored.as_bytes());
        assert!(output.status.success(), "{flags:?}: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    };
    assert_eq!(recolor(&[]), colored);
    assert_eq!(recolor(&["-T"]), seqcol_ok(&["-s", "taylor", "-T", "tests/data/ebola_virus_reduced_align.fa1"]));
}

#[test]
fn test_colours_is_colors() {
    let file = "tests/data/ebola_virus_reduced_align.fa1";
    assert_eq!(seqcol_ok(&["--colours", "16", "-s", "taylor", file]), seqcol_ok(&["--colors", "16", "-s", "taylor", file]));
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

const FASTQ_RECORDS: &[u8] = b"@r1\nAC\n+\nFF\n@r2\nAG\n+\nFF\n";

#[test]
fn test_fastq_quality_lines_are_not_sequence() {
    let output = seqcol_stdin(&["-p", "never", "-s", "taylor"], FASTQ_RECORDS);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    for record in lines.chunks(4) {
        assert!(record[1].contains(TAYLOR_NUCL_A), "{stdout:?}");
        assert_eq!(record[3], "FF", "{stdout:?}");
    }
}

#[test]
fn test_fastq_quality_lines_are_not_colored_with_one_alphabet() {
    let output = seqcol_stdin(&["-p", "never", "-s", "taylor", "-a", "aa"], FASTQ_RECORDS);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    for record in lines.chunks(4) {
        assert_eq!(record[3], "FF", "{stdout:?}");
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
        .env_remove("NO_COLOR")
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

#[test]
fn test_alphabet_file_ignores_whitespace() {
    let stdout = bash_ok(r#""$0" -p never -s taylor -f <(printf 'A C\n') -m 3 <(printf 'A CA\n')"#);
    assert!(!stdout.contains("48;2;"), "{stdout:?}");
}

#[test]
fn test_alphabet_edits() {
    let colored = |alphabet, input: &[u8]| {
        let output = seqcol_stdin(&["-p", "never", "-s", "taylor", "-m", "3", "-a", alphabet], input);
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap().contains("48;2;")
    };
    assert!(colored("dna", b"AC-GT\n"));
    assert!(!colored("dna^-", b"AC-GT\n"));
    assert!(!colored("aa", b"ARBND\n"));
    assert!(colored("aa+B", b"ARBND\n"));
}

/// Run seqcol without a pager with an environment variable, assert success, and return stdout.
fn seqcol_env_ok(key: &str, value: &str, args: &[&str]) -> Vec<u8> {
    let output = seqcol().env(key, value).args(["-p", "never"]).args(args).output().expect("Failed to execute seqcol");
    assert!(output.status.success(), "{key}={value} {args:?} failed: {output:?}");
    output.stdout
}

/// Parameters of each SGR escape sequence, e.g. "1" for "\x1b[1m".
fn sgr_params(text: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(text)
        .split("\x1b[")
        .skip(1)
        .map(|rest| rest.split('m').next().unwrap().to_string())
        .collect()
}

#[test]
fn test_term_dumb_writes_input_unchanged() {
    let file = "tests/data/ebola_virus_reduced_align.fa";
    let input = std::fs::read(file).unwrap();
    assert_eq!(seqcol_env_ok("TERM", "dumb", &["-s", "taylor", "-c", "bold", file]), input);
    let transposed = seqcol_env_ok("TERM", "dumb", &["-T", "-s", "taylor", file]);
    assert!(!transposed.contains(&0x1b), "{:?}", String::from_utf8_lossy(&transposed));
    assert_ne!(transposed, input, "rows must be transposed");
    assert!(!sgr_params(&seqcol_env_ok("TERM", "dumb", &["-k", "16", "-s", "taylor", file])).is_empty());
}

#[test]
fn test_no_color_keeps_attributes() {
    let file = "tests/data/ebola_virus_reduced_align.fa";
    assert_eq!(seqcol_env_ok("NO_COLOR", "1", &["-s", "taylor", file]), std::fs::read(file).unwrap());
    let params = sgr_params(&seqcol_env_ok("NO_COLOR", "1", &["-s", "taylor", "-c", "bold red", file]));
    assert!(params.contains(&"1".to_string()), "{params:?}");
    assert!(params.iter().all(|p| p == "0" || p == "1"), "{params:?}");
    let params = sgr_params(&seqcol_env_ok("NO_COLOR", "1", &["-T", "-s", "taylor", "-c", "bold red", file]));
    assert!(params.contains(&"1".to_string()) && params.iter().all(|p| p == "0" || p == "1"), "{params:?}");
    let colored = seqcol_ok(&["-s", "taylor", file]);
    assert_eq!(String::from_utf8(seqcol_env_ok("NO_COLOR", "", &["-s", "taylor", file])).unwrap(), colored);
    assert_eq!(String::from_utf8(seqcol_env_ok("NO_COLOR", "1", &["-k", "true", "-s", "taylor", file])).unwrap(), colored);
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
fn test_invisible_caret_hides_all() {
    let output = seqcol_stdin(&["-p", "never", "-s", "taylor", "-i", "^"], b"AC\n");
    assert!(output.status.success(), "seqcol failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    for rgb in ["102;255;0", "255;255;0"] {
        assert!(stdout.contains(&format!("48;2;{rgb};38;2;{rgb}m")), "{rgb}: {stdout:?}");
    }
}

#[test]
fn test_user_errors_fail_without_panic() {
    for (args, message) in [
        (&["-c", "nocolor"][..], "Unknown color name."),
        (&["-s", "noscheme"], "noscheme: No such file"),
        (&["-s", "taylor", "-i", "!"], "-i '!': character has no background color."),
        (&["-f", "/dev/null", "-m", "2"], "/dev/null: no characters other than whitespace."),
        (&["-f", "/"], "/: Is a directory"),
        (&["-a", "dna^ACGTN-."], "-a/-f: empty alphabet"),
        (&["-r", "("], "-r: regex parse error"),
    ] {
        let output = seqcol_stdin(&[&["-p", "never"], args].concat(), b"ACGT\n");
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(message), "{args:?}: {stderr}");
        assert!(!stderr.contains("panicked"), "{args:?}: {stderr}");
    }
    let output = seqcol_stdin(&["-p", "never", "-a", "noalphabet"], b"ACGT\n");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("not an alphabet keyword"), "{output:?}");
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
    assert_eq!(lines[0], "id\tname\tseq");
    assert!(lines[1].starts_with("\x1b[0m1\tsample_one\t\x1b["), "{stdout:?}");
    assert_eq!(lines[2], "2\tfoo\tTTGACCA");
    assert!(lines[3].starts_with("\x1b[0m3\tbar\t\x1b["), "N must not split the run: {stdout:?}");
    assert!(lines[4].starts_with("\x1b[0m4\tbaz\t\x1b["), ". must not split the run: {stdout:?}");
    assert!(lines[4].contains("38;2;128;128;128m."), ". must be a gray gap: {stdout:?}");

    let dna_output = seqcol()
        .args(["-p", "never", "-s", "taylor", "-a", "dna", "-m", "8", "tests/data/table.tsv"])
        .output()
        .expect("Failed to execute seqcol");
    assert_eq!(stdout.as_bytes(), dna_output.stdout, "must color DNA, and N and . must be in the dna alphabet");

    let no_gap_output = seqcol()
        .args(["-p", "never", "-s", "taylor", "-a", "dna^-.", "-m", "8", "tests/data/table.tsv"])
        .output()
        .expect("Failed to execute seqcol");
    assert!(no_gap_output.status.success(), "seqcol failed: {:?}", no_gap_output);
    let no_gap_stdout = String::from_utf8(no_gap_output.stdout).unwrap();
    assert_eq!(no_gap_stdout.lines().nth(4), Some("4\tbaz\tACG.TAC..GTA"), "{no_gap_stdout:?}");
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

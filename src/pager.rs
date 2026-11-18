use std::env;
use std::process::{Child, Command, Stdio};

/// Spawn the pager from `$PAGER` (default `less`), with a piped stdin.
///
/// # Returns
/// The pager process, or `None` if it does not start.
pub fn spawn_pager() -> Option<Child> {
    let pager = env::var("PAGER").unwrap_or_else(|_| "less".to_string());

    // Parse pager command (may include arguments like "less -R")
    let mut parts = pager.split_whitespace();
    let cmd = parts.next()?;
    let args: Vec<&str> = parts.collect();

    let mut command = Command::new(cmd);
    command.args(&args);

    // If using less, ensure essential flags are set
    if cmd == "less" {
        if !args.iter().any(|a| a.contains("-R") || a.contains("--RAW-CONTROL-CHARS")) {
            command.arg("-R"); // Interpret ANSI color sequences
        }
        if !args.iter().any(|a| a.contains("--mouse")) {
            command.arg("--mouse"); // Enable mouse scrolling (required for mouse wheel to work)
        }
        if !args.iter().any(|a| a.contains("--wheel-lines")) {
            command.arg("--wheel-lines=5"); // Scroll 5 lines per wheel tick by default
        }
        if !args.iter().any(|a| a.contains("-F") || a.contains("--quit-if-one-screen")) {
            command.arg("-F"); // Quit pager if there are only a few lines to display
        }
        if !args.iter().any(|a| a.contains("-X") || a.contains("--no-init")) {
            command.arg("-X"); // Small files will be shown without clearing the entire window
        }
        if !args.iter().any(|a| a.contains("-L") || a.contains("--no-lessopen")) {
            command.arg("-L"); // Ignore LESSOPEN preprocessor (content already processed)
        }
        // Add other defaults only if no arguments were provided
        if args.is_empty() {
            command.arg("-S"); // Chop long lines (horizontal scroll instead of wrap)
            command.arg("-K"); // Quit on Ctrl-C
            command.arg("-~"); // Don't show tildes for lines past EOF
            command.arg("-#8"); // Horizontal scroll 8 chars at a time (better for sequences)
            // See: https://github.com/sharkdp/bat/issues/193
        }
    }

    // Set LESSCHARSET for proper UTF-8 handling
    command.env("LESSCHARSET", "UTF-8");

    command
        .stdin(Stdio::piped())
        .spawn()
        .ok()
}

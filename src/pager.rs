use std::env;
use std::process::{Child, Command, Stdio};

/// Spawn the pager from `$PAGER` (default `less`), with a piped stdin.
///
/// # Arguments
/// * `quit_if_one_screen` - Add `-F` to `less`, so it quits when the content fits on one screen.
///
/// # Returns
/// The pager process, or `None` if it does not start.
pub fn spawn_pager(quit_if_one_screen: bool) -> Option<Child> {
    let pager = env::var("PAGER").unwrap_or_else(|_| "less".to_string());
    pager_command(&pager, quit_if_one_screen)?
        .stdin(Stdio::piped())
        .spawn()
        .ok()
}

/// Build the command for a pager command line. Adds default flags to `less`.
///
/// # Arguments
/// * `pager` - Program and arguments, separated by whitespace, e.g. "less -R".
/// * `quit_if_one_screen` - Add `-F` to `less`, so it quits when the content fits on one screen.
///
/// # Returns
/// The command, or `None` if `pager` is empty.
fn pager_command(pager: &str, quit_if_one_screen: bool) -> Option<Command> {
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
        if quit_if_one_screen && !args.iter().any(|a| a.contains("-F") || a.contains("--quit-if-one-screen")) {
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
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_arg(command: &Command, arg: &str) -> bool {
        command.get_args().any(|a| a == arg)
    }

    #[test]
    fn less_quits_if_one_screen_only_when_asked() {
        assert!(has_arg(&pager_command("less", true).unwrap(), "-F"));
        assert!(!has_arg(&pager_command("less", false).unwrap(), "-F"));
        assert!(has_arg(&pager_command("less -S", true).unwrap(), "-F"));
    }

    #[test]
    fn user_args_are_kept() {
        assert!(has_arg(&pager_command("less -F", false).unwrap(), "-F"));
        assert!(has_arg(&pager_command("more -x", true).unwrap(), "-x"));
        assert!(!has_arg(&pager_command("more", true).unwrap(), "-F"));
    }
}

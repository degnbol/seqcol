use std::env;
use std::process::{Child, Command, Stdio};

/// Spawn the pager from `$PAGER` (default `less`), with a piped stdin.
///
/// # Arguments
/// * `quit_if_one_screen` - Whether `less` quits when the content fits on one screen. See `pager_command`.
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

/// Build the command for a pager command line. Adds default flags to `less`,
/// before the given arguments, so the given arguments override them.
///
/// # Arguments
/// * `pager` - Program and arguments, separated by whitespace, e.g. "less -R".
/// * `quit_if_one_screen` - Whether `less` quits when the content fits on one screen.
///   `true` adds `-F` as a default. `false` adds `-+F` after the given arguments,
///   which overrides `-F` from the given arguments and `$LESS`.
///
/// # Returns
/// The command, or `None` if `pager` is empty.
fn pager_command(pager: &str, quit_if_one_screen: bool) -> Option<Command> {
    let mut parts = pager.split_whitespace();
    let cmd = parts.next()?;
    let args: Vec<&str> = parts.collect();

    let mut command = Command::new(cmd);
    if cmd == "less" {
        command.args([
            "-R", // Interpret ANSI color sequences
            "--mouse", // Enable mouse scrolling (required for mouse wheel to work)
            "--wheel-lines=5", // Scroll 5 lines per wheel tick by default
            "-X", // Small files will be shown without clearing the entire window
            "-L", // Ignore LESSOPEN preprocessor (content already processed)
        ]);
        if quit_if_one_screen {
            command.arg("-F"); // Quit pager if there are only a few lines to display
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
    command.args(&args);
    if cmd == "less" && !quit_if_one_screen {
        command.arg("-+F"); // Reset -F from the given arguments and $LESS
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

    fn position(command: &Command, arg: &str) -> Option<usize> {
        command.get_args().position(|a| a == arg)
    }

    #[test]
    fn less_quits_if_one_screen_only_when_asked() {
        assert!(has_arg(&pager_command("less", true).unwrap(), "-F"));
        assert!(!has_arg(&pager_command("less", false).unwrap(), "-F"));
        assert!(has_arg(&pager_command("less -S", true).unwrap(), "-F"));
    }

    #[test]
    fn no_quit_if_one_screen_overrides_user_args() {
        let command = pager_command("less -RF", false).unwrap();
        assert!(position(&command, "-RF").unwrap() < position(&command, "-+F").unwrap());
        assert!(!has_arg(&pager_command("more", false).unwrap(), "-+F"));
    }

    #[test]
    fn user_args_follow_defaults() {
        let command = pager_command("less --wheel-lines=3", true).unwrap();
        assert!(position(&command, "--wheel-lines=5").unwrap() < position(&command, "--wheel-lines=3").unwrap());
    }

    #[test]
    fn long_user_args_keep_short_defaults() {
        assert!(has_arg(&pager_command("less --LINE-NUMBERS", true).unwrap(), "-L"));
    }

    #[test]
    fn user_args_are_kept() {
        assert!(has_arg(&pager_command("less -F", false).unwrap(), "-F"));
        assert!(has_arg(&pager_command("more -x", true).unwrap(), "-x"));
        assert!(!has_arg(&pager_command("more", true).unwrap(), "-F"));
    }
}

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
    let less_options = env::var("LESS").ok();
    pager_command(&pager, less_options.as_deref(), quit_if_one_screen)?
        .stdin(Stdio::piped())
        .spawn()
        .ok()
}

/// Default options for `less`.
const LESS_OPTIONS: [&str; 9] = [
    "-S", // Chop long lines (horizontal scroll instead of wrap)
    "-K", // Quit on Ctrl-C
    "-~", // Don't show tildes for lines past EOF
    "-#8", // Horizontal scroll 8 chars at a time (better for sequences)
    // See: https://github.com/sharkdp/bat/issues/193
    "-R", // Interpret ANSI color sequences
    "-X", // Small files will be shown without clearing the entire window
    "-L", // Ignore LESSOPEN preprocessor (content already processed)
    "--mouse", // Enable mouse scrolling (required for mouse wheel to work)
    "--wheel-lines=5", // Scroll 5 lines per wheel tick by default
];

/// Build the command for a pager command line.
/// For `less`, sets `$LESS` to the default options followed by `less_options`.
/// `less` applies `$LESS` before its arguments, and later options override earlier ones,
/// so `less_options` override the defaults, and the arguments in `pager` override both.
///
/// # Arguments
/// * `pager` - Program and arguments, separated by whitespace, e.g. "less -R".
/// * `less_options` - The user's `$LESS`, e.g. "-i" or "FRX".
/// * `quit_if_one_screen` - Whether `less` quits when the content fits on one screen.
///   `true` adds `-F` to the default options. `false` adds `-+F` after the arguments,
///   which overrides `-F` from all other sources.
///
/// # Returns
/// The command, or `None` if `pager` is empty.
fn pager_command(pager: &str, less_options: Option<&str>, quit_if_one_screen: bool) -> Option<Command> {
    let mut parts = pager.split_whitespace();
    let cmd = parts.next()?;

    let mut command = Command::new(cmd);
    command.args(parts);
    if cmd == "less" {
        let mut options = LESS_OPTIONS.join(" ");
        if quit_if_one_screen {
            options.push_str(" -F"); // Quit pager if there are only a few lines to display
        }
        if let Some(user_options) = less_options.map(str::trim_start).filter(|o| !o.is_empty()) {
            // less ignores options without a leading dash after other options.
            // A leading `+` is a start command, e.g. "+G".
            let dash = if user_options.starts_with(['-', '+']) { "" } else { "-" };
            options = format!("{options} {dash}{user_options}");
        }
        command.env("LESS", options);
        if !quit_if_one_screen {
            command.arg("-+F");
        }
    }

    // Set LESSCHARSET for proper UTF-8 handling
    command.env("LESSCHARSET", "UTF-8");
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command: &Command) -> Vec<&str> {
        command.get_args().map(|a| a.to_str().unwrap()).collect()
    }

    fn less_env(command: &Command) -> Option<&str> {
        command.get_envs()
            .find(|(key, _)| *key == "LESS")
            .and_then(|(_, value)| value?.to_str())
    }

    #[test]
    fn less_options_precede_user_less_options() {
        let command = pager_command("less", Some("-+S"), false).unwrap();
        let options = less_env(&command).unwrap();
        assert!(options.starts_with(&LESS_OPTIONS.join(" ")));
        assert!(options.ends_with(" -+S"));
    }

    #[test]
    fn user_less_options_get_a_dash() {
        let command = pager_command("less", Some("FRX"), false).unwrap();
        assert!(less_env(&command).unwrap().ends_with(" -FRX"));
        let command = pager_command("less", Some("+G"), false).unwrap();
        assert!(less_env(&command).unwrap().ends_with(" +G"));
    }

    #[test]
    fn no_quit_if_one_screen_follows_user_less_options() {
        let command = pager_command("less", Some("-F"), false).unwrap();
        assert_eq!(args(&command), ["-+F"]);
    }

    #[test]
    fn less_quits_if_one_screen_only_when_asked() {
        let quitting = pager_command("less -N", None, true).unwrap();
        assert!(less_env(&quitting).unwrap().ends_with(" -F"));
        assert_eq!(args(&quitting), ["-N"]);

        let staying = pager_command("less -N", None, false).unwrap();
        assert!(!less_env(&staying).unwrap().contains(" -F"));
        assert_eq!(args(&staying), ["-N", "-+F"]);
    }

    #[test]
    fn other_pagers_are_unchanged() {
        let command = pager_command("more -x", Some("-S"), false).unwrap();
        assert_eq!(args(&command), ["-x"]);
        assert_eq!(less_env(&command), None);
    }
}

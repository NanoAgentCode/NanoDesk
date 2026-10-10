use std::env;
use std::io::{self, IsTerminal};

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD_CYAN: &str = "\x1b[1;36m";
const ANSI_BOLD_GREEN: &str = "\x1b[1;32m";
const ANSI_BOLD_RED: &str = "\x1b[1;31m";
const ANSI_BLUE: &str = "\x1b[34m";
const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_DIM: &str = "\x1b[2m";

#[derive(Clone, Copy)]
pub(super) struct CliTheme {
    pub(super) enabled: bool,
}

impl CliTheme {
    pub(super) fn stdout() -> Self {
        Self::new(io::stdout().is_terminal())
    }

    pub(super) fn stderr() -> Self {
        Self::new(io::stderr().is_terminal())
    }

    pub(super) fn new(is_terminal: bool) -> Self {
        Self {
            enabled: is_terminal
                && env::var_os("NO_COLOR").is_none()
                && env::var("TERM").map_or(true, |term| term != "dumb"),
        }
    }

    pub(super) fn paint(self, text: impl AsRef<str>, color: &str) -> String {
        if self.enabled {
            format!("{color}{}{ANSI_RESET}", text.as_ref())
        } else {
            text.as_ref().to_string()
        }
    }

    pub(super) fn brand(self, text: impl AsRef<str>) -> String {
        self.paint(text, ANSI_BOLD_CYAN)
    }

    pub(super) fn prompt(self, text: impl AsRef<str>) -> String {
        self.paint(text, ANSI_BOLD_GREEN)
    }

    pub(super) fn error(self, text: impl AsRef<str>) -> String {
        self.paint(text, ANSI_BOLD_RED)
    }

    pub(super) fn label(self, text: impl AsRef<str>) -> String {
        self.paint(text, ANSI_BLUE)
    }

    pub(super) fn success(self, text: impl AsRef<str>) -> String {
        self.paint(text, ANSI_GREEN)
    }

    pub(super) fn command(self, text: impl AsRef<str>) -> String {
        self.paint(text, ANSI_YELLOW)
    }

    pub(super) fn accent(self, text: impl AsRef<str>) -> String {
        self.paint(text, ANSI_CYAN)
    }

    pub(super) fn muted(self, text: impl AsRef<str>) -> String {
        self.paint(text, ANSI_DIM)
    }
}

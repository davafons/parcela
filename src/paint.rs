//! Terminal colors, shown only when the stream is a terminal and NO_COLOR is unset.
use std::io::IsTerminal;

#[derive(Clone, Copy)]
pub struct Paint {
    color: bool,
}

impl Paint {
    pub fn stdout() -> Self {
        Self::new(std::io::stdout().is_terminal())
    }
    pub fn stderr() -> Self {
        Self::new(std::io::stderr().is_terminal())
    }
    fn new(terminal: bool) -> Self {
        Self {
            color: terminal && std::env::var_os("NO_COLOR").is_none(),
        }
    }
    pub fn paint(self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }
    /// `name | ` before a log line. `index` is the process's place in the list, so neighbours
    /// get different colors.
    pub fn log_prefix(self, index: usize, name: &str, width: usize) -> String {
        const COLORS: [&str; 6] = ["36", "33", "32", "35", "34", "31"];
        self.paint(COLORS[index % COLORS.len()], &format!("{name:width$} |"))
    }
}

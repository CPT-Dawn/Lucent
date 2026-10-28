use std::fmt;

/// A warning or error about a config file, pointing at a line and column
/// (both counted from 1) with a plain-language way to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub column: usize,
    pub message: String,
    /// How to fix it, when there is something to suggest.
    pub help: Option<String>,
}

impl Diagnostic {
    /// A diagnostic at byte `offset` into `text`.
    pub(super) fn at(text: &str, offset: usize, message: String, help: Option<String>) -> Self {
        let mut offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        let before = &text[..offset];
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        Self {
            line: before.matches('\n').count() + 1,
            column: before[line_start..].chars().count() + 1,
            message,
            help,
        }
    }
}

impl fmt::Display for Diagnostic {
    /// `line:column: message`, then the fix on its own line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)?;
        if let Some(help) = &self.help {
            write!(f, "\n  help: {help}")?;
        }
        Ok(())
    }
}

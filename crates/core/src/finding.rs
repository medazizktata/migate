//! What every check returns.

/// How bad a finding is. Ordered, so the worst finding decides the exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Note,
    Warning,
    Error,
}

/// One problem, tied to a permanent rule ID (`docs/research/architecture.md`, section 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: &'static str,
    pub severity: Severity,
    /// The migration or object the finding is about.
    pub object: String,
    pub message: String,
    /// Where in the project it comes from: a file relative to the project, and a line (1-based).
    pub at: Option<(String, u32)>,
}

impl Finding {
    pub fn new(
        rule: &'static str,
        severity: Severity,
        object: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            rule,
            severity,
            object: object.into(),
            message: message.into(),
            at: None,
        }
    }

    /// The same finding, placed at `file:line`.
    pub fn at(mut self, file: impl Into<String>, line: u32) -> Self {
        self.at = Some((file.into(), line));
        self
    }
}

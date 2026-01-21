//! Error types for the templating module.

use thiserror::Error;

/// Errors that can occur during template rendering.
#[derive(Debug, Error)]
pub enum TemplateError {
    /// Template syntax is invalid (e.g., unclosed tags, invalid expressions).
    #[error("template syntax error: {0}")]
    SyntaxError(String),

    /// A variable referenced in the template is undefined and has no default.
    #[error("undefined variable: {0}")]
    UndefinedVariable(String),

    /// A macro or function call failed during rendering.
    #[error("macro error: {0}")]
    MacroError(String),

    /// Template rendering failed for an unexpected reason.
    #[error("render error: {0}")]
    RenderError(String),
}

#[cfg(feature = "templating")]
impl From<minijinja::Error> for TemplateError {
    fn from(err: minijinja::Error) -> Self {
        use minijinja::ErrorKind;

        match err.kind() {
            ErrorKind::SyntaxError => Self::SyntaxError(err.to_string()),
            ErrorKind::UndefinedError => Self::UndefinedVariable(err.to_string()),
            _ => Self::RenderError(err.to_string()),
        }
    }
}

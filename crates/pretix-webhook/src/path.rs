use std::fmt::{Display, Formatter};

/// An invalid webhook URL path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookPathError {
    message: String,
}

impl WebhookPathError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for WebhookPathError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for WebhookPathError {}

/// Validates an absolute, exact webhook path.
///
/// # Errors
///
/// Returns [`WebhookPathError`] when `path` is not `/` or an absolute path
/// made of static URL-unreserved ASCII segments.
pub fn validate_absolute_webhook_path(path: &str) -> Result<(), WebhookPathError> {
    validate_absolute(path, "absolute webhook path")
}

fn validate_absolute(path: &str, subject: &str) -> Result<(), WebhookPathError> {
    if !path.starts_with('/') {
        return Err(WebhookPathError::new(format!(
            "invalid {subject} {path:?}: it must start with '/'"
        )));
    }
    if path == "/" {
        return Ok(());
    }
    if path.ends_with('/') {
        return Err(WebhookPathError::new(format!(
            "invalid {subject} {path:?}: trailing slashes are not allowed"
        )));
    }

    validate_segments(&path[1..], subject, path)
}

/// Validates a global webhook prefix.
///
/// # Errors
///
/// Returns [`WebhookPathError`] when `prefix` is not `/` or an absolute path
/// made of static URL-unreserved ASCII segments.
pub fn validate_webhook_prefix(prefix: &str) -> Result<(), WebhookPathError> {
    validate_absolute(prefix, "webhook prefix")
}

/// Validates a relative webhook registration path.
///
/// # Errors
///
/// Returns [`WebhookPathError`] when `path` is empty, has a leading or trailing
/// slash, or contains anything other than static URL-unreserved ASCII segments.
pub fn validate_relative_webhook_path(path: &str) -> Result<(), WebhookPathError> {
    let subject = "relative webhook path";
    if path.is_empty() {
        return Err(WebhookPathError::new(format!(
            "invalid {subject} {path:?}: it must contain at least one segment"
        )));
    }
    if path.starts_with('/') {
        return Err(WebhookPathError::new(format!(
            "invalid {subject} {path:?}: leading slashes are not allowed"
        )));
    }
    if path.ends_with('/') {
        return Err(WebhookPathError::new(format!(
            "invalid {subject} {path:?}: trailing slashes are not allowed"
        )));
    }

    validate_segments(path, subject, path)
}

fn validate_segments(
    segments: &str,
    subject: &str,
    full_path: &str,
) -> Result<(), WebhookPathError> {
    for segment in segments.split('/') {
        if segment.is_empty() {
            return Err(WebhookPathError::new(format!(
                "invalid {subject} {full_path:?}: empty path segments are not allowed"
            )));
        }
        if matches!(segment, "." | "..") {
            return Err(WebhookPathError::new(format!(
                "invalid {subject} {full_path:?}: '.' and '..' segments are not allowed"
            )));
        }
        if let Some(character) = segment
            .chars()
            .find(|character| !is_url_unreserved_ascii(*character))
        {
            return Err(WebhookPathError::new(format!(
                "invalid {subject} {full_path:?}: character {character:?} is not allowed; segments may contain only ASCII letters, digits, '-', '.', '_', and '~'"
            )));
        }
    }
    Ok(())
}

fn is_url_unreserved_ascii(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '.' | '_' | '~')
}

/// Validates and joins a global prefix and relative registration path.
///
/// # Errors
///
/// Returns [`WebhookPathError`] when either argument does not follow its path
/// grammar.
pub fn resolve_webhook_path(prefix: &str, relative_path: &str) -> Result<String, WebhookPathError> {
    validate_webhook_prefix(prefix)?;
    validate_relative_webhook_path(relative_path)?;

    if prefix == "/" {
        Ok(format!("/{relative_path}"))
    } else {
        Ok(format!("{prefix}/{relative_path}"))
    }
}

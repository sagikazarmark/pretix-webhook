use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WebhookPathError {
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

pub(crate) fn validate_absolute_webhook_path(path: &str) -> Result<(), WebhookPathError> {
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

pub(crate) fn validate_webhook_prefix(prefix: &str) -> Result<(), WebhookPathError> {
    validate_absolute(prefix, "webhook prefix")
}

pub(crate) fn validate_relative_webhook_path(path: &str) -> Result<(), WebhookPathError> {
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

pub(crate) fn resolve_webhook_path(
    prefix: &str,
    relative_path: &str,
) -> Result<String, WebhookPathError> {
    validate_webhook_prefix(prefix)?;
    validate_relative_webhook_path(relative_path)?;

    if prefix == "/" {
        Ok(format!("/{relative_path}"))
    } else {
        Ok(format!("{prefix}/{relative_path}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_static_paths_are_accepted_and_resolved_canonically() {
        for path in ["/", "/hooks", "/hooks/AZaz09-._~"] {
            validate_absolute_webhook_path(path).unwrap();
            validate_webhook_prefix(path).unwrap();
        }

        for path in ["pretix", "integrations/pretix/AZaz09-._~"] {
            validate_relative_webhook_path(path).unwrap();
        }

        assert_eq!(
            resolve_webhook_path("/hooks", "integrations/pretix").unwrap(),
            "/hooks/integrations/pretix"
        );
        assert_eq!(
            resolve_webhook_path("/", "integrations/pretix").unwrap(),
            "/integrations/pretix"
        );
    }

    #[test]
    fn invalid_absolute_paths_report_the_rejected_form() {
        for path in [
            "hooks",
            "/hooks/",
            "/hooks//pretix",
            "/hooks/./pretix",
            "/hooks/../pretix",
            "/hooks/{id}",
            "/hooks/:id",
            "/hooks/*rest",
            "/hooks?enabled=true",
            "/hooks#pretix",
            "/hooks%2Fpretix",
            "/hooks pretix",
            "/hooks/é",
        ] {
            let error = validate_absolute_webhook_path(path).unwrap_err();
            assert!(
                error.to_string().contains(path),
                "error did not identify {path:?}: {error}"
            );
        }
    }

    #[test]
    fn invalid_relative_paths_and_prefixes_are_rejected_before_resolution() {
        for path in [
            "",
            "/hooks",
            "hooks/",
            "hooks//pretix",
            "hooks/./pretix",
            "hooks/../pretix",
            "hooks/{organizer}",
            "hooks?enabled=true",
            "hooks%2Fpretix",
            "hooks pretix",
        ] {
            let error = validate_relative_webhook_path(path).unwrap_err();
            assert!(
                error.to_string().contains(path),
                "error did not identify {path:?}: {error}"
            );
        }

        let prefix_error = resolve_webhook_path("/hooks/", "pretix").unwrap_err();
        assert!(prefix_error.to_string().contains("webhook prefix"));

        let relative_error = resolve_webhook_path("/hooks", "/pretix").unwrap_err();
        assert!(relative_error.to_string().contains("relative webhook path"));
    }
}

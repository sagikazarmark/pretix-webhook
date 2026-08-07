use pretix_webhook::{
    NoopHandler, WebhookConfig, resolve_webhook_path, validate_absolute_webhook_path,
    validate_relative_webhook_path, validate_webhook_prefix, webhook_router_at,
};

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

#[test]
fn exact_router_construction_returns_invalid_paths_as_errors() {
    let result = webhook_router_at("/hooks/{organizer}", NoopHandler, WebhookConfig::new());

    let Err(error) = result else {
        panic!("dynamic route was accepted");
    };
    assert!(error.to_string().contains("/hooks/{organizer}"));
}

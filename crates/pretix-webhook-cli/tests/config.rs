use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use clap::Parser;
use pretix_webhook::{NoopHandler, webhook_router};
use pretix_webhook_cli::Config;
use tower::ServiceExt;

#[tokio::test]
async fn reads_server_policy_and_credentials_from_environment() {
    let config = temp_env::with_vars(
        [
            ("PRETIX_WEBHOOK_BIND", Some("0.0.0.0:8787")),
            ("PRETIX_WEBHOOK_PATH", Some("/hooks/pretix")),
            ("PRETIX_WEBHOOK_ALLOW_ORGANIZERS", Some("acmecorp;other")),
            ("PRETIX_WEBHOOK_ALLOW_EVENTS", Some("democon;conference")),
            (
                "PRETIX_WEBHOOK_CREDENTIALS",
                Some("old:secret;current:new-secret"),
            ),
        ],
        || Config::try_parse_from(["pretix-webhook"]).unwrap(),
    );

    assert_eq!(
        config.bind(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8787)
    );
    assert_eq!(config.path(), "/hooks/pretix");
    assert_eq!(config.allowed_organizers(), ["acmecorp", "other"]);
    assert_eq!(config.allowed_events(), ["democon", "conference"]);

    let app = webhook_router(NoopHandler, config.webhook_config().unwrap());
    let payload = r#"{
        "notification_id": 1,
        "organizer": "acmecorp",
        "event": "democon",
        "action": "pretix.event.changed"
    }"#;
    let request = Request::post("/").body(Body::from(payload)).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    for authorization in ["Basic b2xkOnNlY3JldA==", "Basic Y3VycmVudDpuZXctc2VjcmV0"] {
        let request = Request::post("/")
            .header("authorization", authorization)
            .body(Body::from(payload))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}

#[tokio::test]
async fn filters_are_optional_and_default_to_unrestricted() {
    let config = temp_env::with_vars(
        [
            ("PRETIX_WEBHOOK_ALLOW_ORGANIZERS", None::<&str>),
            ("PRETIX_WEBHOOK_ALLOW_EVENTS", None::<&str>),
            ("PRETIX_WEBHOOK_CREDENTIALS", None::<&str>),
        ],
        || Config::try_parse_from(["pretix-webhook"]).unwrap(),
    );

    assert!(config.is_unrestricted());
    assert!(config.allowed_organizers().is_empty());
    assert!(config.allowed_events().is_empty());

    let app = webhook_router(NoopHandler, config.webhook_config().unwrap());
    let request = Request::post("/")
        .body(Body::from(
            r#"{
                "notification_id": 1,
                "organizer": "any-organizer",
                "event": "any-event",
                "action": "pretix.event.changed"
            }"#,
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[test]
fn organizer_and_event_flags_are_independently_repeatable() {
    let config = Config::try_parse_from([
        "pretix-webhook",
        "--allow-organizer",
        "acmecorp",
        "--allow-event",
        "democon",
        "--allow-organizer",
        "other",
        "--allow-event",
        "conference",
    ])
    .unwrap();

    assert_eq!(config.allowed_organizers(), ["acmecorp", "other"]);
    assert_eq!(config.allowed_events(), ["democon", "conference"]);
}

#[test]
fn combined_allowlist_option_and_environment_variable_are_removed() {
    let error =
        Config::try_parse_from(["pretix-webhook", "--allow", "acmecorp/democon"]).unwrap_err();
    assert!(error.to_string().contains("--allow"));

    let config = temp_env::with_var("PRETIX_WEBHOOK_ALLOW", Some("acmecorp/democon"), || {
        Config::try_parse_from(["pretix-webhook"]).unwrap()
    });
    assert!(config.is_unrestricted());
}

#[test]
fn empty_and_whitespace_padded_filter_values_are_rejected() {
    for (option, value) in [
        ("--allow-organizer", ""),
        ("--allow-organizer", " padded"),
        ("--allow-event", ""),
        ("--allow-event", "padded "),
    ] {
        let config = Config::try_parse_from(["pretix-webhook", option, value]).unwrap();
        assert!(config.webhook_config().is_err());
    }
}

#[test]
fn command_line_paths_use_the_library_static_path_grammar() {
    for path in ["/", "/hooks/AZaz09-._~"] {
        let config = Config::try_parse_from(["pretix-webhook", "--path", path]).unwrap();
        assert_eq!(config.path(), path);
    }

    for path in [
        "hooks",
        "/hooks/",
        "/hooks//pretix",
        "/hooks/./pretix",
        "/hooks/{organizer}",
        "/hooks?enabled=true",
        "/hooks%2Fpretix",
        "/hooks pretix",
    ] {
        let error = Config::try_parse_from(["pretix-webhook", "--path", path]).unwrap_err();
        assert!(
            error.to_string().contains(path),
            "error did not identify {path:?}: {error}"
        );
    }
}

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Mutex, MutexGuard},
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use clap::Parser;
use pretix_webhook::{NoopHandler, webhook_router, webhook_router_at};
use pretix_webhook_cli::Config;
use tower::ServiceExt;

static ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn explicit_toml_config_builds_independently_filtered_public_routes() {
    let environment = lock_environment();
    let config_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/multi.toml");
    let config = Config::try_parse_from(["pretix-webhook", "--config", config_path])
        .unwrap()
        .into_effective()
        .unwrap();
    drop(environment);

    assert_eq!(
        config
            .endpoints()
            .iter()
            .map(pretix_webhook_cli::EffectiveEndpoint::path)
            .collect::<Vec<_>>(),
        ["/incoming/sales/orders", "/incoming/operations/checkins"]
    );
    assert!(
        config
            .endpoints()
            .iter()
            .all(pretix_webhook_cli::EffectiveEndpoint::is_unauthenticated)
    );

    let (bind, endpoints) = config.into_parts();
    assert_eq!(bind, "127.0.0.1:3000".parse().unwrap());
    let mut app = axum::Router::new();
    for endpoint in endpoints {
        let (path, webhook_config) = endpoint.into_parts();
        app = app.merge(webhook_router_at(&path, NoopHandler, webhook_config).unwrap());
    }

    let accepted = Request::post("/incoming/sales/orders")
        .body(Body::from(event_payload("acmecorp", "democon")))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(accepted).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let filtered = Request::post("/incoming/operations/checkins")
        .body(Body::from(event_payload("acmecorp", "democon")))
        .unwrap();
    assert_eq!(
        app.oneshot(filtered).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn multi_prefix_precedence_is_cli_environment_toml_then_default() {
    let _environment = lock_environment();
    let minimal = fixture("minimal-multi.toml");
    let configured = fixture("multi.toml");

    temp_env::with_var("PRETIX_WEBHOOK_PREFIX", None::<&str>, || {
        let defaulted = parse_multi(&minimal, &[]);
        assert_eq!(defaulted.endpoints()[0].path(), "/webhook/nested/route");

        let from_toml = parse_multi(&configured, &[]);
        assert_eq!(from_toml.endpoints()[0].path(), "/incoming/sales/orders");
    });

    temp_env::with_var("PRETIX_WEBHOOK_PREFIX", Some("/environment"), || {
        let from_environment = parse_multi(&configured, &[]);
        assert_eq!(
            from_environment.endpoints()[0].path(),
            "/environment/sales/orders"
        );

        let from_cli = parse_multi(&configured, &["--prefix", "/command-line"]);
        assert_eq!(from_cli.endpoints()[0].path(), "/command-line/sales/orders");
    });

    let rooted = temp_env::with_var("PRETIX_WEBHOOK_PREFIX", None::<&str>, || {
        parse_multi(&fixture("root-prefix.toml"), &[])
    });
    assert_eq!(rooted.endpoints()[0].path(), "/top/level");
}

#[test]
fn multi_mode_is_selected_only_by_an_explicit_flag() {
    let _environment = lock_environment();
    let config = temp_env::with_vars(
        [
            (
                "PRETIX_WEBHOOK_CONFIG",
                Some(fixture("multi.toml").as_str()),
            ),
            ("PRETIX_WEBHOOK_PREFIX", None),
            ("PRETIX_WEBHOOK_PATH", None),
        ],
        || Config::try_parse_from(["pretix-webhook"]).unwrap(),
    );

    assert_eq!(config.config_path_input(), None);
    assert_eq!(
        config.into_effective().unwrap().endpoints()[0].path(),
        "/webhook"
    );
}

#[test]
fn simple_and_multi_endpoint_inputs_are_mutually_exclusive() {
    let _environment = lock_environment();
    let config_path = fixture("minimal-multi.toml");
    for arguments in [
        vec!["--path", "/simple"],
        vec!["--allow-organizer", "acmecorp"],
        vec!["--allow-event", "democon"],
        vec!["--credential", "user:password"],
    ] {
        let mut command = vec!["pretix-webhook", "--config", config_path.as_str()];
        command.extend(arguments);
        let error = Config::try_parse_from(command)
            .unwrap()
            .into_effective()
            .unwrap_err();
        assert!(error.to_string().contains("cannot be combined"));
    }

    for (variable, value) in [
        ("PRETIX_WEBHOOK_PATH", "/simple"),
        ("PRETIX_WEBHOOK_ALLOW_ORGANIZERS", "acmecorp"),
        ("PRETIX_WEBHOOK_ALLOW_EVENTS", "democon"),
        ("PRETIX_WEBHOOK_CREDENTIALS", "user:password"),
    ] {
        let from_environment = temp_env::with_var(variable, Some(value), || {
            Config::try_parse_from(["pretix-webhook", "--config", config_path.as_str()])
                .unwrap()
                .into_effective()
                .unwrap_err()
        });
        assert!(from_environment.to_string().contains("cannot be combined"));
    }

    for prefix_source in [
        Config::try_parse_from(["pretix-webhook", "--prefix", "/multi"])
            .unwrap()
            .into_effective()
            .unwrap_err(),
        temp_env::with_var("PRETIX_WEBHOOK_PREFIX", Some("/multi"), || {
            Config::try_parse_from(["pretix-webhook"])
                .unwrap()
                .into_effective()
                .unwrap_err()
        }),
    ] {
        assert!(prefix_source.to_string().contains("require --config"));
    }
}

#[test]
fn strict_toml_errors_identify_the_source_file() {
    let _environment = lock_environment();
    for (name, expected) in [
        ("unknown-root.toml", "unknown field `unexpected`"),
        ("unknown-webhook.toml", "unknown field `allow_organizer`"),
        ("malformed-type.toml", "invalid type"),
        (
            "empty-multi.toml",
            "at least one [[webhooks]] entry is required",
        ),
    ] {
        let path = fixture(name);
        let error = Config::try_parse_from(["pretix-webhook", "--config", path.as_str()])
            .unwrap()
            .into_effective()
            .unwrap_err();
        let diagnostic = error.to_string();
        assert!(
            diagnostic.contains(&path),
            "missing source in {diagnostic:?}"
        );
        assert!(
            diagnostic.contains(expected),
            "missing {expected:?} in {diagnostic:?}"
        );
    }
}

#[test]
fn bind_configuration_remains_available_in_multi_mode() {
    let _environment = lock_environment();
    let config = parse_multi(&fixture("minimal-multi.toml"), &["--bind", "0.0.0.0:8787"]);
    assert_eq!(config.bind(), "0.0.0.0:8787".parse().unwrap());
}

#[tokio::test]
async fn reads_server_policy_and_credentials_from_environment() {
    let environment = lock_environment();
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
        || {
            Config::try_parse_from(["pretix-webhook"])
                .unwrap()
                .into_effective()
                .unwrap()
        },
    );
    drop(environment);

    assert_eq!(
        config.bind(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8787)
    );
    assert_eq!(config.endpoints()[0].path(), "/hooks/pretix");

    let (_, mut endpoints) = config.into_parts();
    let endpoint = endpoints.pop().unwrap();
    let (_, webhook_config) = endpoint.into_parts();
    let app = webhook_router(NoopHandler, webhook_config);
    let payload = r#"{
        "notification_id": 1,
        "organizer": "acmecorp",
        "event": "democon",
        "action": "pretix.event.changed"
    }"#;
    let request = Request::post("/").body(Body::from(payload)).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    assert_credentials_accepted(app, payload).await;
}

#[tokio::test]
async fn filters_are_optional_and_default_to_unrestricted() {
    let environment = lock_environment();
    let config = temp_env::with_vars(
        [
            ("PRETIX_WEBHOOK_ALLOW_ORGANIZERS", None::<&str>),
            ("PRETIX_WEBHOOK_ALLOW_EVENTS", None::<&str>),
            ("PRETIX_WEBHOOK_CREDENTIALS", None::<&str>),
        ],
        || {
            Config::try_parse_from(["pretix-webhook"])
                .unwrap()
                .into_effective()
                .unwrap()
        },
    );
    drop(environment);

    assert!(config.endpoints()[0].is_unrestricted());
    assert!(config.endpoints()[0].is_unauthenticated());

    let (_, mut endpoints) = config.into_parts();
    let endpoint = endpoints.pop().unwrap();
    let (_, webhook_config) = endpoint.into_parts();
    let app = webhook_router(NoopHandler, webhook_config);
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
fn omitted_and_explicit_default_paths_have_the_same_effective_endpoint() {
    let _environment = lock_environment();
    temp_env::with_var("PRETIX_WEBHOOK_PATH", None::<&str>, || {
        let defaulted = Config::try_parse_from(["pretix-webhook"]).unwrap();
        assert_eq!(defaulted.path_input(), None);
        assert_eq!(
            defaulted.into_effective().unwrap().endpoints()[0].path(),
            "/webhook"
        );

        let explicit = Config::try_parse_from(["pretix-webhook", "--path", "/webhook"]).unwrap();
        assert_eq!(explicit.path_input(), Some("/webhook"));
        assert_eq!(
            explicit.into_effective().unwrap().endpoints()[0].path(),
            "/webhook"
        );
    });
}

#[tokio::test]
async fn organizer_event_and_credential_flags_are_independently_repeatable() {
    let environment = lock_environment();
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
        "--credential",
        "old:secret",
        "--credential",
        "current:new-secret",
    ])
    .unwrap()
    .into_effective()
    .unwrap();
    drop(environment);

    let (_, mut endpoints) = config.into_parts();
    let endpoint = endpoints.pop().unwrap();
    let (_, webhook_config) = endpoint.into_parts();
    let app = webhook_router(NoopHandler, webhook_config);
    let payload = r#"{
        "notification_id": 1,
        "organizer": "other",
        "event": "conference",
        "action": "pretix.event.changed"
    }"#;
    assert_credentials_accepted(app, payload).await;
}

#[test]
fn combined_allowlist_option_and_environment_variable_are_removed() {
    let _environment = lock_environment();
    let error =
        Config::try_parse_from(["pretix-webhook", "--allow", "acmecorp/democon"]).unwrap_err();
    assert!(error.to_string().contains("--allow"));

    let config = temp_env::with_var("PRETIX_WEBHOOK_ALLOW", Some("acmecorp/democon"), || {
        Config::try_parse_from(["pretix-webhook"])
            .unwrap()
            .into_effective()
            .unwrap()
    });
    assert!(config.endpoints()[0].is_unrestricted());
}

#[test]
fn empty_and_whitespace_padded_filter_values_are_rejected() {
    let _environment = lock_environment();
    for (option, value) in [
        ("--allow-organizer", ""),
        ("--allow-organizer", " padded"),
        ("--allow-event", ""),
        ("--allow-event", "padded "),
    ] {
        let config = Config::try_parse_from(["pretix-webhook", option, value]).unwrap();
        assert!(config.into_effective().is_err());
    }
}

#[test]
fn empty_and_malformed_credentials_are_rejected() {
    let _environment = lock_environment();
    for credential in ["", "username", ":password", "username:"] {
        let error =
            Config::try_parse_from(["pretix-webhook", "--credential", credential]).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("expected USERNAME:PASSWORD")
                || message.contains("username and password must be non-empty"),
            "unexpected error for {credential:?}: {message}"
        );
    }
}

#[test]
fn command_line_paths_use_the_library_static_path_grammar() {
    let _environment = lock_environment();
    for path in ["/", "/hooks/AZaz09-._~"] {
        let config = Config::try_parse_from(["pretix-webhook", "--path", path]).unwrap();
        assert_eq!(config.path_input(), Some(path));
        assert_eq!(config.into_effective().unwrap().endpoints()[0].path(), path);
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

async fn assert_credentials_accepted(app: axum::Router, payload: &str) {
    for authorization in ["Basic b2xkOnNlY3JldA==", "Basic Y3VycmVudDpuZXctc2VjcmV0"] {
        let request = Request::post("/")
            .header("authorization", authorization)
            .body(Body::from(payload.to_owned()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}

fn event_payload(organizer: &str, event: &str) -> String {
    format!(
        r#"{{
            "notification_id": 1,
            "organizer": "{organizer}",
            "event": "{event}",
            "action": "pretix.event.changed"
        }}"#
    )
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn parse_multi(path: &str, arguments: &[&str]) -> pretix_webhook_cli::EffectiveConfig {
    let mut command = vec!["pretix-webhook", "--config", path];
    command.extend_from_slice(arguments);
    Config::try_parse_from(command)
        .unwrap()
        .into_effective()
        .unwrap()
}

fn lock_environment() -> MutexGuard<'static, ()> {
    ENVIRONMENT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

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
async fn toml_routes_resolve_independent_rotatable_credentials() {
    let environment = lock_environment();
    let config = temp_env::with_vars(
        [
            ("FIRST_OLD_CREDENTIAL", Some("old:secret")),
            ("FIRST_CURRENT_CREDENTIAL", Some("current:new:secret")),
            (
                "SECOND_CREDENTIAL",
                Some("route-two-user:route-two-password"),
            ),
        ],
        || parse_multi(&fixture("credential-multi.toml"), &[]),
    );
    drop(environment);

    let debug = format!("{config:?}");
    for secret_part in [
        "old",
        "secret",
        "current",
        "new",
        "route-two-user",
        "route-two-password",
    ] {
        assert!(!debug.contains(secret_part), "secret leaked in {debug:?}");
    }
    assert_eq!(
        config
            .endpoints()
            .iter()
            .map(pretix_webhook_cli::EffectiveEndpoint::is_unauthenticated)
            .collect::<Vec<_>>(),
        [false, false, true, true]
    );

    let mut app = axum::Router::new();
    for endpoint in config.into_parts().1 {
        let (path, webhook_config) = endpoint.into_parts();
        app = app.merge(webhook_router_at(&path, NoopHandler, webhook_config).unwrap());
    }

    for authorization in ["Basic b2xkOnNlY3JldA==", "Basic Y3VycmVudDpuZXc6c2VjcmV0"] {
        let response = app
            .clone()
            .oneshot(authorized_request(
                "/incoming/first",
                authorization,
                event_payload("acmecorp", "democon"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    let wrong_route = app
        .clone()
        .oneshot(authorized_request(
            "/incoming/second",
            "Basic b2xkOnNlY3JldA==",
            event_payload("acmecorp", "democon"),
        ))
        .await
        .unwrap();
    assert_eq!(wrong_route.status(), StatusCode::UNAUTHORIZED);

    let second_route = app
        .clone()
        .oneshot(authorized_request(
            "/incoming/second",
            "Basic cm91dGUtdHdvLXVzZXI6cm91dGUtdHdvLXBhc3N3b3Jk",
            event_payload("acmecorp", "democon"),
        ))
        .await
        .unwrap();
    assert_eq!(second_route.status(), StatusCode::NO_CONTENT);

    for path in ["/incoming/empty-references", "/incoming/omitted-references"] {
        let request = Request::post(path)
            .body(Body::from(event_payload("acmecorp", "democon")))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    let malformed = app
        .oneshot(authorized_request(
            "/incoming/second",
            "Basic b2xkOnNlY3JldA==",
            "not json".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        malformed.headers().get("www-authenticate").unwrap(),
        "Basic realm=\"pretix-webhook\""
    );
}

#[test]
fn invalid_credential_references_fail_closed_without_disclosing_values() {
    let _environment = lock_environment();
    let config_path = fixture("credential-multi.toml");

    for invalid_value in [
        None,
        Some(""),
        Some("leaked-username"),
        Some(":leaked-password"),
        Some("leaked-user:"),
    ] {
        let error = temp_env::with_vars(
            [
                ("FIRST_OLD_CREDENTIAL", invalid_value),
                ("FIRST_CURRENT_CREDENTIAL", Some("valid:credential")),
                ("SECOND_CREDENTIAL", Some("valid:credential")),
            ],
            || {
                Config::try_parse_from(["pretix-webhook", "--config", config_path.as_str()])
                    .unwrap()
                    .into_effective()
                    .unwrap_err()
            },
        );

        for diagnostic in [error.to_string(), format!("{error:?}")] {
            assert!(diagnostic.contains("webhooks entry 1"), "{diagnostic:?}");
            assert!(diagnostic.contains("/incoming/first"), "{diagnostic:?}");
            assert!(
                diagnostic.contains("FIRST_OLD_CREDENTIAL"),
                "{diagnostic:?}"
            );
            if let Some(value) = invalid_value.filter(|value| !value.is_empty()) {
                assert!(
                    !diagnostic.contains(value),
                    "value leaked in {diagnostic:?}"
                );
            }
            for secret_part in ["leaked-username", "leaked-password", "leaked-user"] {
                assert!(
                    !diagnostic.contains(secret_part),
                    "secret leaked in {diagnostic:?}"
                );
            }
        }
    }
}

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
fn toml_semantic_errors_are_aggregated_deterministically_without_sensitive_values() {
    let _environment = lock_environment();
    let path = fixture("invalid-semantic-multi.toml");
    let diagnostic = temp_env::with_vars(
        [
            (
                "DUPLICATE_CREDENTIAL_REFERENCE",
                Some("private-user:private-secret"),
            ),
            ("MISSING_CREDENTIAL_REFERENCE", None),
        ],
        || {
            Config::try_parse_from(["pretix-webhook", "--config", path.as_str()])
                .unwrap()
                .into_effective()
                .unwrap_err()
                .to_string()
        },
    );

    let expected_in_order = [
        "webhooks entry 1: duplicate organizer slug",
        "webhooks entry 1: duplicate event slug",
        "webhooks entry 1: duplicate credential environment-variable name",
        "webhooks entry 1 (\"/incoming/duplicate\"): invalid organizer slug: it must not be empty",
        "webhooks entry 1 (\"/incoming/duplicate\"): invalid organizer slug: leading and trailing whitespace are not allowed",
        "webhooks entry 1 (\"/incoming/duplicate\"): invalid event slug: leading and trailing whitespace are not allowed",
        "webhooks entry 1 (\"/incoming/duplicate\"): credential environment variable \"MISSING_CREDENTIAL_REFERENCE\" is missing or not valid Unicode",
        "webhooks entry 2: duplicate resolved webhook path \"/incoming/duplicate\" (first used by entry 1)",
        "webhooks entry 3: invalid relative webhook path \"\"",
        "webhooks entry 4: invalid relative webhook path \"/leading\"",
        "webhooks entry 5: invalid relative webhook path \"trailing/\"",
        "webhooks entry 6: invalid relative webhook path \"empty//segment\"",
        "webhooks entry 7: invalid relative webhook path \"dot/./segment\"",
        "webhooks entry 8: invalid relative webhook path \"dot-dot/../segment\"",
        "webhooks entry 9: invalid relative webhook path \"dynamic/{route}\"",
        "webhooks entry 10: invalid relative webhook path \"query?enabled=true\"",
        "webhooks entry 11: invalid relative webhook path \"fragment#value\"",
        "webhooks entry 12: invalid relative webhook path \"encoded%2Froute\"",
        "webhooks entry 13: invalid relative webhook path \"white space\"",
        "webhooks entry 14: invalid relative webhook path \"non-ascii-é\"",
    ];
    let mut previous = 0;
    for expected in expected_in_order {
        let position = diagnostic[previous..]
            .find(expected)
            .unwrap_or_else(|| panic!("missing {expected:?} in {diagnostic:?}"));
        previous += position + expected.len();
    }

    for sensitive in [
        "private-organizer",
        "private-event",
        "DUPLICATE_CREDENTIAL_REFERENCE",
        "private-user",
        "private-secret",
    ] {
        assert!(
            !diagnostic.contains(sensitive),
            "value leaked in {diagnostic:?}"
        );
    }
}

#[test]
fn multi_mode_source_conflicts_empty_routes_and_invalid_prefix_are_aggregated() {
    let _environment = lock_environment();
    let path = fixture("empty-multi.toml");
    let diagnostic = temp_env::with_vars(
        [
            ("PRETIX_WEBHOOK_PATH", Some("/private-path")),
            ("PRETIX_WEBHOOK_ALLOW_ORGANIZERS", Some("private-organizer")),
            ("PRETIX_WEBHOOK_ALLOW_EVENTS", Some("private-event")),
            (
                "PRETIX_WEBHOOK_CREDENTIALS",
                Some("private-user:private-secret"),
            ),
        ],
        || {
            Config::try_parse_from([
                "pretix-webhook",
                "--config",
                path.as_str(),
                "--prefix",
                "invalid-prefix",
            ])
            .unwrap()
            .into_effective()
            .unwrap_err()
            .to_string()
        },
    );

    let expected_in_order = [
        "simple webhook path input cannot be combined with --config",
        "simple organizer filter inputs cannot be combined with --config",
        "simple event filter inputs cannot be combined with --config",
        "simple credential inputs cannot be combined with --config",
        "at least one [[webhooks]] entry is required",
        "invalid webhook prefix \"invalid-prefix\"",
    ];
    let mut previous = 0;
    for expected in expected_in_order {
        let position = diagnostic[previous..]
            .find(expected)
            .unwrap_or_else(|| panic!("missing {expected:?} in {diagnostic:?}"));
        previous += position + expected.len();
    }

    for sensitive in [
        "private-path",
        "private-organizer",
        "private-event",
        "private-user",
        "private-secret",
    ] {
        assert!(
            !diagnostic.contains(sensitive),
            "value leaked in {diagnostic:?}"
        );
    }
}

#[test]
fn every_invalid_multi_prefix_form_is_semantically_rejected() {
    let _environment = lock_environment();
    let path = fixture("minimal-multi.toml");
    for prefix in [
        "incoming",
        "/incoming/",
        "/incoming//route",
        "/incoming/./route",
        "/incoming/../route",
        "/incoming/{route}",
        "/incoming?enabled=true",
        "/incoming#fragment",
        "/incoming%2Froute",
        "/incoming route",
        "/incoming/é",
    ] {
        let config = Config::try_parse_from([
            "pretix-webhook",
            "--config",
            path.as_str(),
            "--prefix",
            prefix,
        ])
        .unwrap();
        let diagnostic = config.into_effective().unwrap_err().to_string();
        assert!(
            diagnostic.contains(prefix),
            "error did not identify {prefix:?}: {diagnostic:?}"
        );
    }
}

#[test]
fn invalid_prefix_does_not_hide_independent_route_errors() {
    let _environment = lock_environment();
    let path = fixture("invalid-semantic-multi.toml");
    let diagnostic = temp_env::with_vars(
        [
            ("DUPLICATE_CREDENTIAL_REFERENCE", Some("valid:credential")),
            ("MISSING_CREDENTIAL_REFERENCE", None),
        ],
        || {
            Config::try_parse_from([
                "pretix-webhook",
                "--config",
                path.as_str(),
                "--prefix",
                "invalid-prefix",
            ])
            .unwrap()
            .into_effective()
            .unwrap_err()
            .to_string()
        },
    );

    for expected in [
        "invalid webhook prefix",
        "duplicate organizer slug",
        "duplicate webhook route (first used by entry 1; resolved path unavailable because the prefix is invalid)",
        "invalid relative webhook path",
        "MISSING_CREDENTIAL_REFERENCE",
    ] {
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

fn authorized_request(path: &str, authorization: &str, body: String) -> Request<Body> {
    Request::post(path)
        .header("authorization", authorization)
        .body(Body::from(body))
        .unwrap()
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

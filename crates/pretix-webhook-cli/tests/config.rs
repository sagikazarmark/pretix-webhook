use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use clap::Parser;
use pretix_webhook::{NoopHandler, webhook_router};
use pretix_webhook_cli::{AllowedTarget, Config};
use tower::ServiceExt;

#[tokio::test]
async fn reads_server_policy_and_credentials_from_environment() {
    let config = temp_env::with_vars(
        [
            ("PRETIX_WEBHOOK_BIND", Some("0.0.0.0:8787")),
            ("PRETIX_WEBHOOK_PATH", Some("/hooks/pretix")),
            ("PRETIX_WEBHOOK_ALLOW", Some("acmecorp/democon;other/*")),
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
    assert_eq!(
        config.allowed_targets(),
        [
            AllowedTarget::Event {
                organizer: "acmecorp".into(),
                event: "democon".into(),
            },
            AllowedTarget::AllEvents {
                organizer: "other".into(),
            },
        ]
    );

    let app = webhook_router(NoopHandler, config.webhook_config());
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
async fn allowlist_is_optional_and_defaults_to_unrestricted() {
    let config = temp_env::with_vars(
        [
            ("PRETIX_WEBHOOK_ALLOW", None::<&str>),
            ("PRETIX_WEBHOOK_CREDENTIALS", None::<&str>),
        ],
        || Config::try_parse_from(["pretix-webhook"]).unwrap(),
    );

    assert!(config.is_unrestricted());
    assert!(config.allowed_targets().is_empty());

    let app = webhook_router(NoopHandler, config.webhook_config());
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

use std::error::Error;

use clap::Parser;
use pretix_webhook::webhook_router_at;
use pretix_webhook_cli::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_observability();
    let config = Config::parse().into_effective()?;
    let (bind, endpoints) = config.into_parts();
    let mut app = axum::Router::new();
    let mut diagnostics = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints {
        diagnostics.push(StartupRoute {
            path: endpoint.path().to_owned(),
            unrestricted: endpoint.is_unrestricted(),
            unauthenticated: endpoint.is_unauthenticated(),
        });
        let (path, webhook_config) = endpoint.into_parts();
        app = app.merge(webhook_router_at(
            &path,
            selected_handler(&path)?,
            webhook_config,
        )?);
    }
    let listener = tokio::net::TcpListener::bind(bind).await?;

    report_startup(listener.local_addr()?, &diagnostics);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

#[cfg(feature = "tracing")]
fn selected_handler(
    path: &str,
) -> Result<impl pretix_webhook::WebhookHandler, pretix_webhook::WebhookPathError> {
    pretix_webhook::TracingHandler::with_route(path)
}

#[cfg(all(not(feature = "tracing"), feature = "log"))]
fn selected_handler(
    path: &str,
) -> Result<impl pretix_webhook::WebhookHandler, pretix_webhook::WebhookPathError> {
    pretix_webhook::LogHandler::with_route(path)
}

#[cfg(not(any(feature = "tracing", feature = "log")))]
fn selected_handler(
    _path: &str,
) -> Result<impl pretix_webhook::WebhookHandler, pretix_webhook::WebhookPathError> {
    Ok(pretix_webhook::NoopHandler)
}

#[cfg(feature = "tracing")]
fn init_observability() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("pretix_webhook=info,pretix_webhook_cli=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

#[cfg(all(not(feature = "tracing"), feature = "log"))]
fn init_observability() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
}

#[cfg(not(any(feature = "tracing", feature = "log")))]
fn init_observability() {}

#[cfg(feature = "tracing")]
fn announce_listener(bind: std::net::SocketAddr, route_count: usize) {
    tracing::info!(target: "pretix_webhook_cli", %bind, route_count, "pretix webhook receiver listening on http://{bind} with {route_count} route(s)");
}

#[cfg(feature = "tracing")]
fn announce_route(route: &str) {
    tracing::info!(target: "pretix_webhook_cli", %route, "pretix webhook route configured at {route}");
}

#[cfg(feature = "tracing")]
fn warn_unrestricted(route: &str) {
    tracing::warn!(target: "pretix_webhook_cli", %route, "warning: no filters configured for {route}; accepting all events from all organizers");
}

#[cfg(feature = "tracing")]
fn warn_unauthenticated(route: &str) {
    tracing::warn!(target: "pretix_webhook_cli", %route, "warning: no HTTP Basic credentials configured for {route}; accepting unauthenticated requests");
}

#[cfg(all(not(feature = "tracing"), feature = "log"))]
fn announce_listener(bind: std::net::SocketAddr, route_count: usize) {
    log::info!(
        target: "pretix_webhook_cli",
        "pretix webhook receiver listening on http://{bind} with {} route(s)",
        route_count
    );
}

#[cfg(all(not(feature = "tracing"), feature = "log"))]
fn announce_route(route: &str) {
    log::info!(target: "pretix_webhook_cli", "pretix webhook route configured at {route}");
}

#[cfg(all(not(feature = "tracing"), feature = "log"))]
fn warn_unrestricted(route: &str) {
    log::warn!(
        target: "pretix_webhook_cli",
        "warning: no filters configured for {route}; accepting all events from all organizers"
    );
}

#[cfg(all(not(feature = "tracing"), feature = "log"))]
fn warn_unauthenticated(route: &str) {
    log::warn!(
        target: "pretix_webhook_cli",
        "warning: no HTTP Basic credentials configured for {route}; accepting unauthenticated requests"
    );
}

#[cfg(not(any(feature = "tracing", feature = "log")))]
fn announce_listener(bind: std::net::SocketAddr, route_count: usize) {
    eprintln!(
        "pretix webhook receiver listening on http://{bind} with {} route(s)",
        route_count
    );
}

#[cfg(not(any(feature = "tracing", feature = "log")))]
fn announce_route(route: &str) {
    eprintln!("pretix webhook route configured at {route}");
}

#[cfg(not(any(feature = "tracing", feature = "log")))]
fn warn_unrestricted(route: &str) {
    eprintln!(
        "warning: no filters configured for {route}; accepting all events from all organizers"
    );
}

#[cfg(not(any(feature = "tracing", feature = "log")))]
fn warn_unauthenticated(route: &str) {
    eprintln!(
        "warning: no HTTP Basic credentials configured for {route}; accepting unauthenticated requests"
    );
}

struct StartupRoute {
    path: String,
    unrestricted: bool,
    unauthenticated: bool,
}

fn report_startup(bind: std::net::SocketAddr, endpoints: &[StartupRoute]) {
    announce_listener(bind, endpoints.len());
    for endpoint in endpoints {
        announce_route(&endpoint.path);
    }
    for endpoint in endpoints {
        if endpoint.unrestricted {
            warn_unrestricted(&endpoint.path);
        }
        if endpoint.unauthenticated {
            warn_unauthenticated(&endpoint.path);
        }
    }
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("failed to install shutdown signal handler: {error}");
    }
}

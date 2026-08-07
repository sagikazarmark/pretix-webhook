use std::error::Error;

use clap::Parser;
use pretix_webhook::webhook_router_at;
use pretix_webhook_cli::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_observability();
    let config = Config::parse().into_effective()?;
    for endpoint in config.endpoints() {
        if endpoint.is_unrestricted() {
            warn_unrestricted(endpoint.path());
        }
    }
    let (bind, endpoints) = config.into_parts();
    let route_count = endpoints.len();
    let mut app = axum::Router::new();
    for endpoint in endpoints {
        let (path, webhook_config) = endpoint.into_parts();
        app = app.merge(webhook_router_at(
            &path,
            selected_handler(&path)?,
            webhook_config,
        )?);
    }
    let listener = tokio::net::TcpListener::bind(bind).await?;

    announce_listener(bind, route_count);
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
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(all(not(feature = "tracing"), feature = "log"))]
fn init_observability() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
}

#[cfg(not(any(feature = "tracing", feature = "log")))]
fn init_observability() {}

#[cfg(feature = "tracing")]
fn announce_listener(bind: std::net::SocketAddr, route_count: usize) {
    tracing::info!(%bind, route_count, "pretix webhook receiver listening");
}

fn warn_unrestricted(path: &str) {
    eprintln!(
        "warning: no filters configured for {path}; accepting all events from all organizers"
    );
}

#[cfg(all(not(feature = "tracing"), feature = "log"))]
fn announce_listener(bind: std::net::SocketAddr, route_count: usize) {
    log::info!(
        "pretix webhook receiver listening on http://{bind} with {} route(s)",
        route_count
    );
}

#[cfg(not(any(feature = "tracing", feature = "log")))]
fn announce_listener(bind: std::net::SocketAddr, route_count: usize) {
    eprintln!(
        "pretix webhook receiver listening on http://{bind} with {} route(s)",
        route_count
    );
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("failed to install shutdown signal handler: {error}");
    }
}

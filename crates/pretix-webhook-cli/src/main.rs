use std::error::Error;

use clap::Parser;
use pretix_webhook::webhook_router_at;
use pretix_webhook_cli::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_observability();
    let config = Config::parse().into_effective()?;
    if config.endpoint().is_unrestricted() {
        warn_unrestricted();
    }
    let (bind, endpoint) = config.into_parts();
    let (path, webhook_config) = endpoint.into_parts();
    let app = webhook_router_at(&path, selected_handler(&path)?, webhook_config)?;
    let listener = tokio::net::TcpListener::bind(bind).await?;

    announce_listener(bind, &path);
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
fn announce_listener(bind: std::net::SocketAddr, path: &str) {
    tracing::info!(%bind, %path, "pretix webhook receiver listening");
}

fn warn_unrestricted() {
    eprintln!("warning: no filters configured; accepting all events from all organizers");
}

#[cfg(all(not(feature = "tracing"), feature = "log"))]
fn announce_listener(bind: std::net::SocketAddr, path: &str) {
    log::info!("pretix webhook receiver listening on http://{bind}{path}");
}

#[cfg(not(any(feature = "tracing", feature = "log")))]
fn announce_listener(bind: std::net::SocketAddr, path: &str) {
    eprintln!("pretix webhook receiver listening on http://{bind}{path}");
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("failed to install shutdown signal handler: {error}");
    }
}

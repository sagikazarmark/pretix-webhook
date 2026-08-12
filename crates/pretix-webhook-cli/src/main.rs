use std::{convert::Infallible, error::Error, process::ExitCode};

use axum::{Router, routing::post_service};
use clap::Parser;
use pretix_webhook_cli::Config;
use pretix_webhook_events::WebhookEvent;

#[tokio::main]
async fn main() -> ExitCode {
    init_observability();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Report through `Display` so multi-line startup validation reports
            // stay readable; the runtime's default `Debug` rendering escapes them
            // onto one line.
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    let config = Config::parse().into_effective()?;
    let (bind, endpoints) = config.into_parts();
    let mut app = Router::new();
    let mut diagnostics = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints {
        diagnostics.push(StartupRoute {
            path: endpoint.path().to_owned(),
            unrestricted: endpoint.is_unrestricted(),
            unauthenticated: endpoint.is_unauthenticated(),
        });
        let (path, webhook_builder) = endpoint.into_parts();
        let service =
            webhook_builder.build(|_event: WebhookEvent| async { Ok::<(), Infallible>(()) });
        app = app.route(&path, post_service(service));
    }
    let listener = tokio::net::TcpListener::bind(bind).await?;

    report_startup(listener.local_addr()?, &diagnostics);
    let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_sender
                .send(tokio::signal::ctrl_c().await)
                .expect("shutdown receiver remains alive while the server runs");
        })
        .await?;
    shutdown_receiver.await??;

    Ok(())
}

fn init_observability() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("pretix_webhook=info,pretix_webhook_cli=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

fn announce_listener(bind: std::net::SocketAddr, route_count: usize) {
    tracing::info!(target: "pretix_webhook_cli", %bind, route_count, "pretix webhook receiver listening on http://{bind} with {route_count} route(s)");
}

fn announce_route(route: &str) {
    tracing::info!(target: "pretix_webhook_cli", %route, "pretix webhook route configured at {route}");
}

fn warn_unrestricted(route: &str) {
    tracing::warn!(target: "pretix_webhook_cli", %route, "warning: no filters configured for {route}; accepting all events from all organizers");
}

fn warn_unauthenticated(route: &str) {
    tracing::warn!(target: "pretix_webhook_cli", %route, "warning: no HTTP Basic credentials configured for {route}; accepting unauthenticated requests");
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

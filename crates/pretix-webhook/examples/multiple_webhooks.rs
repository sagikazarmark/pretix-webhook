use axum::{Router, routing::get};
use pretix_webhook::{
    BasicAuthCredential, MultiWebhookRouter, NoopHandler, WebhookConfig, handler_fn,
};
use pretix_webhook_events::WebhookEvent;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sales_password = std::env::var("PRETIX_SALES_WEBHOOK_PASSWORD")?;
    let operations_password = std::env::var("PRETIX_OPERATIONS_WEBHOOK_PASSWORD")?;
    let sales = WebhookConfig::new()
        .allow_organizer("acmecorp")?
        .allow_event("democon")?
        .require_basic_auth([BasicAuthCredential::new("sales-webhook", sales_password)]);
    let operations = WebhookConfig::new()
        .allow_organizer("acmecorp")?
        .require_basic_auth([BasicAuthCredential::new(
            "operations-webhook",
            operations_password,
        )]);

    let webhooks = MultiWebhookRouter::new("/hooks")?
        .register(
            "sales/orders",
            handler_fn(|event: WebhookEvent| async move {
                println!("sales event: {}", event.action());
                Ok::<_, std::convert::Infallible>(())
            }),
            sales,
        )?
        .register("operations/checkins", NoopHandler, operations)?
        .finish();

    let _app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(webhooks);

    Ok(())
}

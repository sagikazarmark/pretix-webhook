use std::{convert::Infallible, error::Error};

use axum::{
    Router,
    routing::{get, post_service},
};
use pretix_webhook::{BasicAuthCredential, WebhookServiceBuilder};
use pretix_webhook_events::WebhookEvent;

fn main() -> Result<(), Box<dyn Error>> {
    let sales_password = std::env::var("PRETIX_SALES_WEBHOOK_PASSWORD")?;
    let operations_password = std::env::var("PRETIX_OPERATIONS_WEBHOOK_PASSWORD")?;
    let sales = WebhookServiceBuilder::new()
        .allow_organizer("acmecorp")?
        .allow_event("democon")?
        .require_basic_auth([BasicAuthCredential::new("sales-webhook", sales_password)]);
    let operations = WebhookServiceBuilder::new()
        .allow_organizer("acmecorp")?
        .require_basic_auth([BasicAuthCredential::new(
            "operations-webhook",
            operations_password,
        )]);

    let sales = sales.build(|event: WebhookEvent| async move {
        println!("sales event: {}", event.action());
        Ok::<_, Infallible>(())
    });
    let operations =
        operations.build(|_event: WebhookEvent| async move { Ok::<_, Infallible>(()) });

    let _app = Router::<()>::new()
        .route("/health", get(|| async { "ok" }))
        .route("/hooks/sales/orders", post_service(sales))
        .route("/hooks/operations/checkins", post_service(operations));

    Ok(())
}

use std::time::Duration;

use campuslingo_server::recovery::{DeliveryConfig, DeliveryOutcome, deliver_once, http_client};
use sqlx::postgres::PgPoolOptions;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = DeliveryConfig::from_env()?;
    let once = std::env::args().any(|argument| argument == "--once");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    let client = http_client()?;

    loop {
        match deliver_once(&pool, &config.key, &config.endpoint, &client).await {
            Ok(DeliveryOutcome::Delivered { outbox_id }) => {
                println!("{{\"event\":\"recovery_delivered\",\"outbox_id\":\"{outbox_id}\"}}");
            }
            Ok(DeliveryOutcome::Released {
                outbox_id,
                attempts,
            }) => {
                println!(
                    "{{\"event\":\"recovery_deferred\",\"outbox_id\":\"{outbox_id}\",\"attempts\":{attempts}}}"
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Ok(DeliveryOutcome::Idle) => {
                if once {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            Err(error) => {
                eprintln!("{{\"event\":\"recovery_delivery_failed\",\"error\":\"{error}\"}}");
                if once {
                    return Err(error.into());
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
    Ok(())
}

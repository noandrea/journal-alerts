use anyhow::Result;
use flume::Receiver;
use log::{debug, error, info};

#[derive(Clone)]
pub struct Slack {
    webhook_url: String,
    client: reqwest::Client,
}

impl Slack {
    pub fn new(webhook_url: String) -> Self {
        Slack {
            webhook_url,
            client: reqwest::Client::new(),
        }
    }

    pub async fn start(&self, rx: Receiver<String>) -> Result<()> {
        info!("Slack notifier started.");
        while let Ok(message) = rx.recv() {
            debug!("Received alert message: {}", message);
            if let Err(e) = self.send_alert(&message).await {
                error!("Error sending alert to Slack: {}", e);
            }
        }
        Ok(())
    }

    pub async fn send_alert(&self, message: &str) -> Result<()> {
        if self.webhook_url.is_empty() {
            info!("{message}");
            return Ok(());
        }

        let payload = serde_json::json!({ "text": message });
        let res = self
            .client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .inspect_err(|e| error!("HTTP client error {}", e))?;

        if !res.status().is_success() {
            error!("Failed to send alert to Slack. Status: {}", res.status());
        }

        Ok(())
    }
}

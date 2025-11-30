use anyhow::Result;
use log::{error, info};

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
            .await?;

        if !res.status().is_success() {
            error!("Failed to send alert to Slack. Status: {}", res.status());
        }

        Ok(())
    }
}

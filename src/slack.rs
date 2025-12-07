use std::time::Instant;

use anyhow::Result;
use dashmap::DashMap;
use flume::Receiver;
use log::{debug, error, info, warn};

#[derive(Clone)]
pub struct Slack {
    webhook_url: String,
    client: reqwest::Client,
    repeats: DashMap<String, (usize, Instant)>,
}

impl Slack {
    pub fn new(webhook_url: String) -> Self {
        Slack {
            webhook_url,
            client: reqwest::Client::new(),
            repeats: DashMap::new(),
        }
    }

    pub async fn start(&self, rx: Receiver<String>) -> Result<()> {
        info!("Slack notifier started.");
        while let Ok(message) = rx.recv_async().await {
            debug!("Received alert message: {}", message);

            // to avoid spamming, check for duplicates
            if let Some(mut entry) = self.repeats.get_mut(&message) {
                let (count, _) = entry.value_mut();
                *count += 1usize;
                warn!(
                    "Suppressing duplicate alert detected, count: {}: {}",
                    *count, message
                );
                continue;
            }

            if let Err(e) = self.send_alert(&message).await {
                error!("Error sending alert to Slack: {}", e);
                continue;
            }

            // insert into repeats map with count 1 and current instant
            self.repeats
                .insert(message.clone(), (1usize, Instant::now()));
        }

        // remove suppression entries older than 1 hour
        // TODO: make this configurable
        let cutoff = Instant::now() - std::time::Duration::from_secs(3600);
        self.repeats
            .retain(|_, &mut (_, timestamp)| timestamp >= cutoff);

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

use std::sync::Arc;
use std::thread::spawn;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::matcher::Matcher;
use anyhow::{Context, Result};
use dashmap::DashMap;
use flume::Sender;
use log::{debug, error, info, warn};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub struct JournalProcessor {
    config: Config,
    heartbeat_states: Arc<DashMap<usize, (std::time::Instant, String)>>,
    matcher_alerts: Matcher,
    matcher_heartbeats: Matcher,
}

impl JournalProcessor {
    pub fn new(config: &Config) -> Result<Self> {
        // Initialize heartbeat states with current time
        let heartbeat_states = Arc::new(
            config
                .heartbeats
                .iter()
                .enumerate()
                .map(|(i, heartbeat)| (i, (Instant::now(), heartbeat.pattern.clone())))
                .collect::<DashMap<usize, (Instant, String)>>(),
        );

        // Compile matchers for alerts
        let matcher_alerts = Matcher::new(
            config
                .alerts
                .iter()
                .map(|r| r.pattern.clone())
                .collect::<Vec<String>>()
                .as_slice(),
        )?;
        // Compile matchers for heartbeats
        let matcher_heartbeats = Matcher::new(
            config
                .heartbeats
                .iter()
                .map(|r| r.pattern.clone())
                .collect::<Vec<String>>()
                .as_slice(),
        )?;

        let jp = JournalProcessor {
            config: config.clone(),
            heartbeat_states,
            matcher_alerts,
            matcher_heartbeats,
        };
        info!("Loaded {} matching rules for alerts.", config.alerts.len());
        info!(
            "Loaded {} matching rules for heartbeats.",
            config.heartbeats.len()
        );

        Ok(jp)
    }

    pub async fn start(&self, tx: Sender<String>) -> Result<()> {
        info!("Journal processor started.");
        // Start the heartbeat monitoring thread
        let heartbeat_states = self.heartbeat_states.clone();
        let heartbeats = self.config.heartbeats.clone();
        let heartbeat_interval = self.config.heartbeat_interval;
        let heartbeat_tx = tx.clone();

        spawn(move || {
            info!("Heartbeat monitoring thread started.");
            loop {
                let now = std::time::Instant::now();
                for entry in heartbeat_states.iter() {
                    let (i, (last_seen, msg)) = entry.pair();
                    info!(
                        "Heartbeat state for index {}: pattern '{}', last seen {:?} ago",
                        i,
                        msg,
                        last_seen.elapsed()
                    );
                    // retrieve the tolerance for this heartbeat
                    let tolerance = Duration::from_secs(heartbeats[*i].tolerance);
                    // if the heartbeat is overdue
                    if now.saturating_duration_since(*last_seen) > tolerance {
                        heartbeat_tx
                            .send(format!(
                                "Heartbeat missed. Last seen {:?} ago, last message: {}",
                                last_seen.elapsed(),
                                msg,
                            ))
                            .inspect_err(|e| {
                                error!("Failed to send heartbeat missed alert: {}", e);
                            })
                            .ok();
                    } else {
                        debug!(
                            "Heartbeat for index {} is within tolerance (last seen {:?} ago).",
                            i,
                            last_seen.elapsed()
                        );
                    }
                }
                std::thread::sleep(Duration::from_secs(heartbeat_interval));
            }
        });

        // Start processing the journal

        let unit = self.config.systemd_unit.clone();
        let alerts_matcher = &self.matcher_alerts;
        let heartbeats_matcher = &self.matcher_heartbeats;

        let mut args = vec!["-f", "-n", "0", "--output=cat"];

        if self.config.systemd_unit.is_empty() {
            warn!("No systemd unit specified, monitoring all logs.");
        } else {
            info!("Filtering logs for systemd unit: {}", unit);
            args.extend_from_slice(&["--unit", &unit]);
        }

        let mut child = Command::new("journalctl")
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn journalctl process")?;

        let stdout = child
            .stdout
            .take()
            .context("Failed to capture stdout of journalctl")?;

        let mut lines = BufReader::new(stdout).lines();

        while let Ok(Some(message)) = lines.next_line().await {
            // alerts matching
            match alerts_matcher.find_match(&message) {
                Some((_, msg)) => {
                    debug!("Matched alert log message: {}", message);
                    tx.send(msg.clone()).context("tx.send() failed")?;
                }
                None => {
                    debug!("No matching rule for log message: {}", message);
                }
            }

            // heartbeats matching, if matched, update the last seen time
            if let Some((i, msg)) = heartbeats_matcher.find_match(&message) {
                debug!("Matched heartbeat log message: {}", message);
                self.heartbeat_states
                    .insert(i, (std::time::Instant::now(), msg));
            } else {
                debug!("No matching rule for log message: {}", message);
            }
        }

        Ok(())
    }
}

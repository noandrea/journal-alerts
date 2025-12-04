use std::process::Stdio;
use std::sync::Arc;
use std::thread::spawn;
use std::time::{Duration, Instant};

use super::matcher::Matcher;
use crate::config::{Config, HeartbeatRule};
use anyhow::{Context, Result};
use dashmap::DashMap;
use flume::Sender;
use log::{debug, error, info, warn};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub struct JournalProcessor {
    config: Config,
    // Map of heartbeat index to (last seen time, message)
    heartbeat_updates: Arc<DashMap<usize, (Instant, String)>>,
    // Map of heartbeat index to (last seen time, missed count)
    heartbeat_misses: Arc<DashMap<usize, (Instant, usize)>>,
    // Compiled matchers
    matcher_alerts: Matcher,
    matcher_heartbeats: Matcher,
}

impl JournalProcessor {
    pub fn new(config: &Config) -> Result<Self> {
        // Initialize heartbeat states with current time
        let heartbeat_updates = Arc::new(
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
            heartbeat_updates,
            heartbeat_misses: Arc::new(DashMap::new()),
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
        let heartbeat_updates = self.heartbeat_updates.clone();
        let heartbeat_misses = self.heartbeat_misses.clone();
        let heartbeats = self.config.heartbeats.clone();
        let heartbeat_interval = self.config.heartbeat_interval;
        let heartbeat_tx = tx.clone();

        spawn(move || {
            info!("Heartbeat monitoring thread started.");
            loop {
                let now = std::time::Instant::now();
                for entry in heartbeat_updates.iter() {
                    let (i, (last_seen, msg)) = entry.pair();
                    // TODO: make this a debug log
                    info!(
                        "Heartbeat state for index {}: pattern '{}', last seen {:?} ago",
                        i,
                        msg,
                        last_seen.elapsed()
                    );
                    // retrieve the tolerance for this heartbeat
                    let HeartbeatRule {
                        tolerance,
                        prefix,
                        pattern,
                    } = heartbeats[*i].clone();
                    let tolerance = Duration::from_secs(tolerance);
                    // if the heartbeat is overdue
                    let msg = if now.saturating_duration_since(*last_seen) > tolerance {
                        let message = format!(
                            "{} Heartbeat missed for pattern '{}'. Last seen {:?} ago.",
                            prefix,
                            msg,
                            last_seen.elapsed()
                        );
                        Some(message)
                    } else {
                        None
                    };
                    // now decide if to update or not
                    let mut entry = heartbeat_misses.entry(*i).or_insert((now, 0));
                    let (missed_at, missed_count) = entry.value_mut();

                    match (msg, *missed_count) {
                        (Some(msg), 0) => {
                            // first time missed, will send alert below
                            *missed_at = now;
                            *missed_count += 1;
                            heartbeat_tx
                                .send(msg)
                                .inspect_err(|e| {
                                    error!("Failed to send heartbeat missed alert: {}", e);
                                })
                                .ok();
                        }
                        (None, n) if n > 0 => {
                            // recovery
                            let recovery_time = now.saturating_duration_since(*missed_at);
                            let recovery_message = format!(
                                "🩹 Heartbeat recovered in {}s for pattern '{}'.",
                                recovery_time.as_secs(),
                                pattern,
                            );
                            // send recovery alert
                            heartbeat_tx
                                .send(recovery_message)
                                .inspect_err(|e| {
                                    error!("Failed to send heartbeat recovery alert: {}", e);
                                })
                                .ok();
                            // reset the missed count
                            heartbeat_misses.remove(i);
                        }
                        _ => {
                            // (None, 0) => heartbeat is fine, do nothing
                            // (Some(_), n) if n > 0 => already alerted, do nothing
                        }
                    }
                }
                std::thread::sleep(Duration::from_secs(heartbeat_interval));
            }
        });

        // Start processing the journal
        info!("Starting journalctl process...");
        let unit = self.config.systemd_unit.clone();
        let alerts_matcher = &self.matcher_alerts;
        let heartbeats_matcher = &self.matcher_heartbeats;

        let mut args = vec![
            "-oL", // flush output line by line
            "journalctl",
            "--follow",
            "--lines",
            "0",
            "--output=cat",
            "--no-pager",
        ];

        if self.config.systemd_unit.is_empty() {
            warn!("No systemd unit specified, monitoring all logs.");
        } else {
            info!("Filtering logs for systemd unit: {}", unit);
            args.extend_from_slice(&["--unit", &unit]);
        }

        let mut child = Command::new("stdbuf")
            .args(&args)
            .stdout(Stdio::piped())
            .spawn()
            .context("Failed to spawn journalctl process")?;

        let stdout = child
            .stdout
            .take()
            .context("Failed to capture stdout of journalctl")?;

        // use a large buffer (8MB) instead of the default 8KB
        // this will not help if the logs are generated faster than we can process them,
        // at a sustained rate, but it will help to smooth out short bursts
        let buffer_size = 8 * 1024 * 1024;
        let mut lines = BufReader::with_capacity(buffer_size, stdout).lines();

        while let Ok(Some(message)) = lines.next_line().await {
            // alerts matching
            match alerts_matcher.find_match(&message) {
                Some((i, msg)) => {
                    debug!("Matched alert log message: {}", message);
                    // get the prefix for this alerts
                    let prefix = &self.config.alerts[i].prefix;
                    let msg = format!("{}{}", prefix, msg);
                    tx.send(msg.clone()).context("tx.send() failed")?;
                }
                None => {
                    debug!("No matching rule for log message: {}", message);
                }
            }

            // heartbeats matching, if matched, update the last seen time
            if let Some((i, msg)) = heartbeats_matcher.find_match(&message) {
                debug!("Matched heartbeat log message: {}", message);
                self.heartbeat_updates.insert(i, (Instant::now(), msg));
            } else {
                debug!("No matching rule for log message: {}", message);
            }
        }

        Ok(())
    }
}

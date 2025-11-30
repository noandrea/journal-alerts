use std::sync::Arc;
use std::thread::spawn;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::matcher::Matcher;
use anyhow::Result;
use dashmap::DashMap;
use flume::Sender;
use log::{debug, error, info, warn};
use systemd::JournalSeek;
use systemd::journal::OpenOptions;

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
        // Start the heartbeat monitoring thread
        let heartbeat_states = self.heartbeat_states.clone();
        let heartbeats = self.config.heartbeats.clone();
        let heartbeat_tx = tx.clone();

        spawn(move || {
            loop {
                let now = std::time::Instant::now();
                for entry in heartbeat_states.iter() {
                    let (i, (last_seen, msg)) = entry.pair();
                    debug!(
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
                    }
                }
                std::thread::sleep(Duration::from_secs(60));
            }
        });

        // Start processing the journal
        let mut journal = OpenOptions::default()
            .open()
            .expect("Failed to open systemd journal");

        let unit = self.config.systemd_unit.clone();
        let alerts_matcher = &self.matcher_alerts;
        let heartbeats_matcher = &self.matcher_heartbeats;

        if self.config.systemd_unit.is_empty() {
            warn!("No systemd unit specified, monitoring all logs.");
        } else {
            info!("Filtering logs for systemd unit: {}", unit);
            journal
                .match_add("_SYSTEMD_UNIT", unit.clone())
                .inspect_err(|e| error!("failed to fiOpenOptionslter {} unit: {}", unit, e))?;
        }
        journal
            .seek(JournalSeek::Tail)
            .expect("Failed to seek to end of journal");

        journal.previous()?;

        loop {
            if journal.next()? == 0 {
                journal.wait(None)?; // wait for new entries (blocking)
            }

            let Some(message) = journal.get_data("MESSAGE")?.and_then(|v| {
                v.value()
                    .map(String::from_utf8_lossy)
                    .map(|v| v.into_owned())
            }) else {
                info!("Journal entry has no MESSAGE field.");
                continue;
            };

            // alerts matching
            match alerts_matcher.find_match(&message) {
                Some((_, msg)) => {
                    debug!("Matched alert log message: {}", message);
                    tx.send(msg.clone())?;
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
    }
}

mod config;
mod matcher;
mod slack;

use anyhow::Result;
use config::*;
use log::{debug, error, info, warn};
use matcher::*;
use systemd::journal::{JournalSeek, OpenOptions};

use self::slack::Slack;

#[tokio::main]
async fn main() -> Result<()> {
    let binary_name = env!("CARGO_BIN_NAME");
    let version = env!("CARGO_PKG_VERSION");
    let git_hash = env!("GIT_HASH");

    // Handle version flag
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && (args[1] == "-v" || args[1] == "--version") {
        println!(
            "{} version {} (git commit {})",
            binary_name, version, git_hash
        );
        return Ok(());
    }

    env_logger::init();
    info!("Starting {binary_name}...");
    info!(
        "{} version {} (git commit {})",
        binary_name, version, git_hash
    );

    let config_path = std::env::var("LOG_ALERT_CONFIG").ok();
    let config = Config::load(config_path)?;

    let matcher = Matcher::new(&config.rules)?;
    info!("Loaded {} matching rules.", config.rules.len());

    let slack = Slack::new(config.slack_webhook_url.clone());

    let mut journal = OpenOptions::default()
        .open()
        .expect("Failed to open systemd journal");

    if config.systemd_unit.is_empty() {
        warn!("No systemd unit specified, monitoring all logs.");
    } else {
        info!("Filtering logs for systemd unit: {}", config.systemd_unit);
        journal
            .match_add("_SYSTEMD_UNIT", config.systemd_unit.clone())
            .inspect_err(|e| {
                error!(
                    "failed to fiOpenOptionslter {} unit: {}",
                    config.systemd_unit, e
                )
            })?;
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

        let Some(msg) = matcher.find_match(&message) else {
            debug!("No matching rule for log message: {}", message);
            continue;
        };

        debug!("Matched log message: {}", message);

        slack.send_alert(&msg).await?;
    }
}

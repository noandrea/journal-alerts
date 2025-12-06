mod config;
mod processor;
mod slack;

use anyhow::Result;
use config::*;
use log::info;
use tokio::select;

use self::processor::JournalProcessor;
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
    // load configuration
    let config_path = std::env::var("LOG_ALERT_CONFIG").ok();
    let config = Config::load(config_path)?;

    // prepare communication channel
    let (tx, rx) = flume::unbounded::<String>();
    // setup notifier and journal processor
    let slack = Slack::new(config.slack_webhook_url.clone());
    let processor = JournalProcessor::new(&config)?;

    // signal startup complete
    tx.send(format!("{binary_name} has started"))?;

    // start both tasks
    select! {
        res = slack.start(rx) => res?,
        res = processor.start(tx) => res?,
    }
    Ok(())
}

# Journal Alerts

A robust and configurable service that monitors `systemd` journal logs in real-time, sending alerts to a Slack channel when specific conditions are met.

## Overview

`journal-alerts` is a Rust application designed to run as a background service. It tails the logs of a specified `systemd` unit using `journalctl` and matches log lines against user-defined rules. It supports two main types of monitoring:

1.  **Stateless Alerts:** Simple regex-based pattern matching. If a log line matches a defined pattern, an alert is immediately sent to Slack. This is useful for capturing specific error or warning messages.
2.  **Stateful Heartbeats:** Monitors for the *absence* of expected log messages. If a specific log message (a "heartbeat") doesn't appear within a configured time tolerance, a "missed heartbeat" alert is triggered. When the heartbeat message reappears, a recovery alert is sent. This is ideal for ensuring that periodic tasks or services are still running correctly.

## Features

-   **Real-time Monitoring:** Streams logs directly from `journalctl --follow`.
-   **Stateless Pattern Matching:** Trigger alerts on specific log messages (e.g., "error", "panic", "timeout").
-   **Stateful Heartbeat Monitoring:** Get notified when a recurring event *stops* happening.
-   **Slack Integration:** Sends well-formatted alerts to a configured Slack webhook.
-   **Alert Suppression:** Automatically groups and silences repeated alerts to keep channels clean.
-   **Resilient:** Designed to be run as a `systemd` service itself, with robust error handling.

## Prerequisites

-   Rust and Cargo (for building the project).
-   A Linux system with `systemd`.

## Configuration

The application is configured using a `config.toml` file. An example configuration is provided in `deploy/config.example.toml`.

```toml
# Log Alert Configuration

# Systemd service to monitor
systemd_unit = "myservice.service"

# Slack webhook URL for sending notifications
# Get this from: https://api.slack.com/messaging/webhooks
slack_webhook_url = "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"

# (Optional) Interval for checking heartbeats. Defaults to 10 seconds.
# heartbeat_interval = 10 # in seconds

# --- Alert Rules ---
# Each [[alerts]] rule defines a regex pattern to match in the logs.
# When a log line matches, an alert is sent to Slack.

[[alerts]]
pattern = "(?i)error" # Case-insensitive regex for "error"
prefix = "🔴 "

[[alerts]]
pattern = "(?i)warn"
prefix = "🟠 "

# --- Heartbeat Rules ---
# Each [[heartbeats]] rule monitors for an expected periodic log message.
# An alert is sent if the message is NOT seen within the 'tolerance' period.

[[heartbeats]]
pattern = "(?i)health_check_ok" # The expected heartbeat message
prefix = "Missing "             # Prefix for the alert message
tolerance = 300                 # Time in seconds to wait before alerting
```

### Rule Ordering

The stateless `[[alerts]]` rules are evaluated in the order they appear in the configuration file. For any given log line, **only the first matching rule** will be triggered. Therefore, you should place more specific rules before more general ones.

## Usage

1.  **Clone the repository:**
    ```bash
    git clone <repository-url>
    cd journal-alerts
    ```

2.  **Create your configuration:**
    Copy the example config and edit it with your details (e.g., `systemd_unit`, `slack_webhook_url`, and rules).
    ```bash
    cp deploy/config.example.toml config.toml
    nano config.toml
    ```

3.  **Build the application:**
    For optimal performance, build the project in release mode.
    ```bash
    cargo build --release
    ```
    The binary will be located at `target/release/journal-alerts`.

4.  **Run the application:**
    ```bash
    ./target/release/journal-alerts
    ```

## Deployment

This application is intended to be run as a `systemd` service. A unit file is provided at `deploy/journal-alerts.service`.

1.  **Copy the binary** to a suitable location, such as `/usr/local/bin`.
    ```bash
    sudo cp target/release/journal-alerts /usr/local/bin/
    ```

2.  **Copy the configuration file** to `/etc`.
    ```bash
    sudo cp config.toml /etc/journal-alerts.toml
    ```

3.  **Copy and enable the service file:**
    Make sure to edit `journal-alerts.service` to point to the correct binary and configuration file paths if you changed them.
    ```bash
    sudo cp deploy/journal-alerts.service /etc/systemd/system/
    sudo systemctl daemon-reload
    sudo systemctl enable --now journal-alerts.service
    ```

4.  **Check the status and logs:**
    ```bash
    sudo systemctl status journal-alerts
    sudo journalctl -u journal-alerts -f
    ```

## Development

The project includes CI workflows for linting and quality checks, which are defined in the `.github/workflows/` directory.

-   **Linting:** Run `cargo clippy` for static analysis.
-   **Formatting:** Run `cargo fmt` to format the code according to project standards.
-   **Checks:** Run `cargo check` to quickly check for errors without compiling.

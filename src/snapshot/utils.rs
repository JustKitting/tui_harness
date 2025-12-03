use chrono::Utc;
use serde_json;
use std::fs;

use crate::snapshot::types::{Snapshot, SnapshotConfig, SnapshotResult};

/// Generate a timestamp string in YYYYMMDD_HHMMSS format
pub fn generate_timestamp() -> String {
    Utc::now().format("%Y%m%d_%H%M%S").to_string()
}

/// Generate a filename for snapshot images
pub fn generate_filename(prefix: &str, timestamp: &str) -> String {
    format!("{}_{}.png", prefix, timestamp)
}

/// Create base metadata map for snapshots
pub fn create_base_metadata(
    width: u32,
    height: u32,
    source: &str,
    timestamp: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let mut meta = serde_json::Map::new();
    meta.insert("width".to_string(), serde_json::Value::Number(width.into()));
    meta.insert(
        "height".to_string(),
        serde_json::Value::Number(height.into()),
    );
    meta.insert(
        "source".to_string(),
        serde_json::Value::String(source.to_string()),
    );
    meta.insert(
        "timestamp".to_string(),
        serde_json::Value::String(timestamp.to_string()),
    );
    meta
}

/// Write the JSON manifest for a snapshot if configured
pub fn write_manifest(snapshot: &Snapshot, config: &SnapshotConfig) -> SnapshotResult<()> {
    if config.include_manifest {
        let manifest_path = snapshot.image_path.with_extension("json");
        let manifest_data = serde_json::to_value(snapshot)?;
        fs::write(manifest_path, serde_json::to_string_pretty(&manifest_data)?)?;
    }
    Ok(())
}

/// Write a text description file for a snapshot
pub fn write_description(snapshot: &Snapshot, config: &SnapshotConfig) -> SnapshotResult<()> {
    if config.include_metadata {
        let description_path = snapshot.image_path.with_extension("txt");

        // Build description from metadata or defaults
        let visual_content = "not visualized yet";
        let description = if let Some(metadata) = &snapshot.metadata {
            if let Some(state) = metadata.get("state").and_then(|v| v.as_str()) {
                let state_description = metadata
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let detailed_description =
                    generate_state_description(snapshot.source.as_str(), state, state_description);
                format!(
                    "State: {}\nDescription: {}\nSource: {}\nTimestamp: {}\n\n{}\n\nActual visual content: {}",
                    state,
                    state_description,
                    snapshot.source,
                    snapshot.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                    detailed_description,
                    visual_content
                )
            } else if let Some(url) = metadata.get("url").and_then(|v| v.as_str()) {
                format!(
                    "Web page screenshot\nURL: {}\nSource: {}\nTimestamp: {}\n\nThis snapshot captures the web page at the specified URL.\n\nActual visual content: {}",
                    url,
                    snapshot.source,
                    snapshot.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                    visual_content
                )
            } else {
                format!(
                    "{} screenshot\nSource: {}\nTimestamp: {}\n\nThis snapshot captures a {} screen.\n\nActual visual content: {}",
                    snapshot.source,
                    snapshot.source,
                    snapshot.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                    snapshot.source,
                    visual_content
                )
            }
        } else {
            format!(
                "{} screenshot\nSource: {}\nTimestamp: {}\n\nThis snapshot captures a {} screen.\n\nActual visual content: {}",
                snapshot.source,
                snapshot.source,
                snapshot.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                snapshot.source,
                visual_content
            )
        };

        fs::write(description_path, description)?;
    }
    Ok(())
}

/// Generate a detailed description based on the application state
fn generate_state_description(source: &str, state: &str, state_desc: &str) -> String {
    match source {
        "cli" => generate_cli_state_description(state, state_desc),
        "web" => generate_web_state_description(state, state_desc),
        _ => format!(
            "This snapshot captures the '{}' state of the application.",
            state
        ),
    }
}

/// Generate CLI-specific state description
fn generate_cli_state_description(state: &str, _state_desc: &str) -> String {
    match state {
        "initial" => "CLI application in 'initial' state: displaying status bar with uptime and terminal size, progress bar at 0%, three buttons (Increment, Reset, Exit) with Increment selected, checkbox unchecked, slider at 5, no info box visible, counter at 0.".to_string(),
        "navigate_to_increment" => "CLI application in 'navigate_to_increment' state: displaying status bar, progress bar advancing, Increment button selected after navigation, counter at 0.".to_string(),
        "increment_counter" => "CLI application in 'increment_counter' state: displaying status bar, progress bar advancing, Increment button selected, counter incremented from 0.".to_string(),
        "navigate_to_reset" => "CLI application in 'navigate_to_reset' state: displaying status bar, progress bar advancing, Reset button selected after navigation, counter at incremented value.".to_string(),
        "reset_counter" => "CLI application in 'reset_counter' state: displaying status bar, progress bar advancing, Reset button selected, counter reset to 0.".to_string(),
        _ => format!("CLI application in '{}' state: {}", state, _state_desc),
    }
}

/// Generate web-specific state description
fn generate_web_state_description(state: &str, _state_desc: &str) -> String {
    match state {
        "initial" => "Web application in 'initial' state: displaying header with navigation, sidebar with collapsible sections (Section 1 expanded, others collapsed), main content with welcome message, interactive buttons (Primary, Secondary, Accent), volume slider at 50%, color dropdown set to Default, status display showing 'Ready'.".to_string(),
        _ => format!("Web application in '{}' state: {}", state, _state_desc),
    }
}

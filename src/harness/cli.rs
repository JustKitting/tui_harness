use chrono::Utc;

use crate::harness::types::{HarnessConfig, HarnessResult, InputAction};
use crate::snapshot::{Snapshot, SnapshotConfig};

/// Runs the CLI harness using PTY-based VT100 rendering.
/// Returns a list of (state_name, snapshot) pairs.
pub fn run_harness(config: &HarnessConfig) -> HarnessResult<Vec<(String, Snapshot)>> {
    let run_id = format!("run_{}", i64::MAX - Utc::now().timestamp_millis());
    let run_dir = config.output_dir.join(&run_id);

    let snapshot_config = SnapshotConfig {
        output_dir: run_dir.clone(),
        include_metadata: true,
        include_manifest: true,
        allow_mock_captures: false,
    };

    std::fs::create_dir_all(&config.output_dir)?;

    let mut results = Vec::new();

    for state_config in &config.states {
        if state_config.capture_snapshot {
            let mut metadata = serde_json::Map::new();
            metadata.insert(
                "state".to_string(),
                serde_json::Value::String(state_config.name.clone()),
            );
            metadata.insert(
                "description".to_string(),
                serde_json::Value::String(state_config.description.clone()),
            );
            if let Some(expected) = &state_config.expected_description {
                metadata.insert(
                    "expected_description".to_string(),
                    serde_json::Value::String(expected.clone()),
                );
            }

            let snapshot = capture_cli_snapshot_pty(
                &snapshot_config,
                config.binary_path.to_str().unwrap(),
                &config.args,
                &state_config.inputs,
                Some(serde_json::Value::Object(metadata)),
            )?;

            results.push((state_config.name.clone(), snapshot));
        }
    }

    Ok(results)
}

/// Captures a screenshot for CLI testing using PTY-based VT100 rendering
fn capture_cli_snapshot_pty(
    config: &SnapshotConfig,
    binary_path: &str,
    args: &[String],
    inputs: &[InputAction],
    extra_metadata: Option<serde_json::Value>,
) -> HarnessResult<Snapshot> {
    use crate::snapshot::pty::capture_cli_screenshot_pty;

    let mut snapshot = capture_cli_screenshot_pty(config, binary_path, args, inputs)?;

    if let Some(meta) = snapshot.metadata.as_mut() {
        if let serde_json::Value::Object(map) = meta {
            map.insert(
                "source".to_string(),
                serde_json::Value::String("cli".to_string()),
            );
            if let Some(extra) = extra_metadata {
                if let serde_json::Value::Object(extra_map) = extra {
                    for (k, v) in extra_map {
                        map.insert(k, v);
                    }
                }
            }
        }
    }

    Ok(snapshot)
}

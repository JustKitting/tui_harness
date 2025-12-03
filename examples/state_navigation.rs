//! Example demonstrating state-based navigation in the harness system

use cli_vision::harness::{HarnessConfig, InputAction, StateConfig};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_config = create_cli_harness_config();
    println!("CLI Harness Configuration:");
    println!("- Binary: {}", cli_config.binary_path.display());
    println!("- States: {}", cli_config.states.len());

    for state in &cli_config.states {
        println!("  - {}: {}", state.name, state.description);
        println!("    Inputs: {}", state.inputs.len());
    }

    println!("\nTo run the harness:");
    println!("cargo run -- harness /path/to/binary --output ./snapshots");

    Ok(())
}

fn create_cli_harness_config() -> HarnessConfig {
    HarnessConfig {
        binary_path: PathBuf::from("./target/debug/cli_demo"),
        args: vec!["--headless".to_string()],
        output_dir: PathBuf::from("./cli_snapshots"),
        states: vec![
            StateConfig {
                name: "initial".to_string(),
                description: "Initial state".to_string(),
                inputs: vec![],
                capture_snapshot: true,
                expected_description: Some("Status bar visible, Increment button highlighted.".to_string()),
            },
            StateConfig {
                name: "navigate_right".to_string(),
                description: "Navigate to next button".to_string(),
                inputs: vec![InputAction::SendKey("right".to_string())],
                capture_snapshot: true,
                expected_description: Some("Highlight moves to next button.".to_string()),
            },
            StateConfig {
                name: "press_enter".to_string(),
                description: "Press Enter".to_string(),
                inputs: vec![InputAction::SendKey("enter".to_string())],
                capture_snapshot: true,
                expected_description: Some("Button action executed.".to_string()),
            },
        ],
    }
}

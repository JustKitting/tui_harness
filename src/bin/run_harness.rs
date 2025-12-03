use cli_vision::harness::{HarnessConfig, StateConfig};
use std::path::PathBuf;

fn main() {
    let config = HarnessConfig {
        binary_path: PathBuf::from("./target/release/cli_demo"),
        args: vec!["--headless".to_string()],
        output_dir: PathBuf::from("./harness_snapshots"),
        states: vec![StateConfig {
            name: "initial".to_string(),
            description: "Initial CLI interface".to_string(),
            inputs: vec![],
            capture_snapshot: true,
            expected_description: Some(
                "Status bar shows uptime, progress bar at 0%, Increment button selected.".to_string(),
            ),
        }],
    };

    match cli_vision::harness::run_harness(&config) {
        Ok(results) => {
            println!("Harness completed successfully");
            for (name, snapshot) in results {
                println!("State: {} -> {}", name, snapshot.image_path.display());
            }
        }
        Err(e) => {
            eprintln!("Harness failed: {}", e);
        }
    }
}

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for a specific application state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateConfig {
    /// Name of the state (e.g., "initial", "after_down")
    pub name: String,

    /// Description of what this state represents
    pub description: String,

    /// Sequence of input actions to reach this state from the previous state
    pub inputs: Vec<InputAction>,

    /// Whether to capture a snapshot at this state
    pub capture_snapshot: bool,

    /// Optional textual expectation for this state (for VLM comparison)
    pub expected_description: Option<String>,
}

/// Configuration for the harness execution
#[derive(Debug)]
pub struct HarnessConfig {
    /// Path to the binary to execute
    pub binary_path: PathBuf,

    /// Arguments to pass to the binary
    pub args: Vec<String>,

    /// Directory where snapshots will be saved
    pub output_dir: PathBuf,

    /// Sequence of states to navigate through
    pub states: Vec<StateConfig>,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("./target/debug/cli_demo"),
            args: vec!["--headless".to_string()],
            output_dir: PathBuf::from("./harness_snapshots"),
            states: vec![],
        }
    }
}

/// Represents an input action to send to the CLI application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputAction {
    /// Send a string as literal keypresses
    SendString(String),

    /// Send a special key (e.g., "enter", "up", "ctrl+c")
    SendKey(String),
}

/// Result type for harness operations
pub type HarnessResult<T> = Result<T, HarnessError>;

/// Error types for harness operations
#[derive(Debug)]
pub enum HarnessError {
    /// Error spawning or interacting with the process
    Process(String),

    /// Snapshot capture error
    Snapshot(crate::snapshot::SnapshotError),

    /// I/O error
    Io(std::io::Error),
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HarnessError::Process(msg) => write!(f, "Process error: {}", msg),
            HarnessError::Snapshot(err) => write!(f, "Snapshot error: {}", err),
            HarnessError::Io(err) => write!(f, "I/O error: {}", err),
        }
    }
}

impl std::error::Error for HarnessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HarnessError::Process(_) => None,
            HarnessError::Snapshot(err) => Some(err),
            HarnessError::Io(err) => Some(err),
        }
    }
}

impl From<std::io::Error> for HarnessError {
    fn from(err: std::io::Error) -> Self {
        HarnessError::Io(err)
    }
}

impl From<crate::snapshot::SnapshotError> for HarnessError {
    fn from(err: crate::snapshot::SnapshotError) -> Self {
        HarnessError::Snapshot(err)
    }
}

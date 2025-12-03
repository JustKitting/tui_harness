//! Types for test run results.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Result of a single state capture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateCapture {
    /// Step number (0 = initial state)
    pub step: usize,

    /// Input that led to this state (None for initial state)
    pub input: Option<String>,

    /// Path to the screenshot
    pub screenshot_path: PathBuf,

    /// VLM-generated description (if analyze=true)
    pub description: Option<String>,
}

/// Result of a complete test run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    /// Whether the run completed successfully
    pub success: bool,

    /// Error message if failed
    pub error: Option<String>,

    /// All captured states (N inputs → N+1 states)
    pub states: Vec<StateCapture>,
}

//! CLI Vision - Terminal UI testing with vision model analysis.
//!
//! This crate provides:
//! - PTY-based terminal capture for CLI/TUI applications (cross-platform)
//! - MockFramebuffer for testing
//! - Multi-state capture with input sequences
//! - Vision model integration for UI analysis
//! - Session management for organized temp files
//! - Configurable via environment variables
//!
//! # Configuration
//!
//! All major settings can be configured via environment variables:
//!
//! | Variable | Description | Default |
//! |----------|-------------|---------|
//! | `CLI_VISION_VLM_ENDPOINT` | VLM API endpoint | `http://127.0.0.1:8080/v1/chat/completions` |
//! | `CLI_VISION_VLM_MODEL` | Model name | `qwen3` |
//! | `CLI_VISION_SESSION_DIR` | Session directory | `/tmp/cli-vision` |
//! | `CLI_VISION_DEFAULT_DELAY` | Input delay (ms) | `100` |
//! | `CLI_VISION_DEFAULT_SIZE` | Terminal size | `standard` |
//!
//! See [`config`] module for full documentation.
//!
//! # Example
//!
//! ```rust,no_run
//! use cli_vision::snapshot::{PtyBackend, PtyBackendConfig, CaptureBackend};
//!
//! let config = PtyBackendConfig::new("/usr/bin/htop");
//! let mut backend = PtyBackend::new(config);
//! let result = backend.capture().unwrap();
//! std::fs::write("screenshot.png", &result.image_data).unwrap();
//! ```

pub mod config;
pub mod harness;
pub mod runner;
pub mod session;
pub mod snapshot;
pub mod vlm;

// Re-export runner types
pub use runner::{RunResult, StateCapture};

// Re-export harness types
pub use harness::{HarnessConfig, HarnessError, HarnessResult, InputAction, StateConfig, run_harness};

// Re-export snapshot types and backends
pub use snapshot::{
    CaptureBackend, CaptureResult, MockFramebuffer, PtyBackend, PtyBackendConfig,
    Snapshot, SnapshotConfig, SnapshotError, SnapshotResult, capture_with_backend,
};

// Re-export session management
pub use session::{Session, cleanup_old_sessions, list_sessions};

// Re-export VLM client
pub use vlm::{VlmConfig, VlmError, VlmProgress, VlmResult, analyze_image, analyze_image_with_progress, check_health, build_analysis_prompt};

// Re-export configuration
pub use config::{Config, VlmSettings, SessionSettings, DefaultSettings};

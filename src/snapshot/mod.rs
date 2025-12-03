pub mod backend;
pub mod pty;
pub mod types;
pub mod utils;

pub use types::{Snapshot, SnapshotConfig, SnapshotError, SnapshotResult};
pub use backend::{CaptureBackend, CaptureResult, MockFramebuffer, PtyBackend, PtyBackendConfig, capture_with_backend};
pub use pty::{run_with_inputs, run_with_inputs_sized, StateCaptureResult, TerminalSize, Vt100Parser, Vt100Terminal, CELL_HEIGHT, CELL_WIDTH};
pub use utils::{create_base_metadata, generate_filename, generate_timestamp, write_description, write_manifest};

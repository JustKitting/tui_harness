//! Session management for organized temporary file handling.
//!
//! Provides centralized management of capture sessions with:
//! - Unique session directories under a global temp location
//! - Automatic cleanup unless explicitly preserved
//! - Session metadata tracking

use std::path::PathBuf;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

/// Global base directory for all cli-vision sessions
const SESSION_BASE_DIR: &str = "/tmp/cli-vision";

/// A capture session with organized file management
#[derive(Debug, Clone)]
pub struct Session {
    /// Unique session ID
    pub id: String,
    /// Root directory for this session
    pub dir: PathBuf,
    /// Whether to keep files after session ends
    pub keep: bool,
    /// Terminal size used for this session (if applicable)
    pub terminal_size: Option<(u16, u16)>,
}

impl Session {
    /// Create a new session with a unique ID
    pub fn new() -> Self {
        let id = generate_session_id();
        let dir = PathBuf::from(SESSION_BASE_DIR).join(&id);

        Self {
            id,
            dir,
            keep: false,
            terminal_size: None,
        }
    }

    /// Create a session with a specific name/prefix
    pub fn with_name(name: &str) -> Self {
        let timestamp = generate_timestamp_suffix();
        let id = format!("{}_{}", sanitize_name(name), timestamp);
        let dir = PathBuf::from(SESSION_BASE_DIR).join(&id);

        Self {
            id,
            dir,
            keep: false,
            terminal_size: None,
        }
    }

    /// Create a session in a specific directory (for backwards compatibility)
    pub fn in_dir(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let id = dir.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| generate_session_id());

        Self {
            id,
            dir,
            keep: true, // User-specified directories are kept by default
            terminal_size: None,
        }
    }

    /// Set whether to keep files after session ends
    pub fn keep(mut self, keep: bool) -> Self {
        self.keep = keep;
        self
    }

    /// Set terminal size for this session
    pub fn with_terminal_size(mut self, cols: u16, rows: u16) -> Self {
        self.terminal_size = Some((cols, rows));
        self
    }

    /// Initialize the session directory
    pub fn init(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)?;

        // Write session metadata
        let metadata = serde_json::json!({
            "id": self.id,
            "created": chrono::Utc::now().to_rfc3339(),
            "terminal_size": self.terminal_size,
        });

        let metadata_path = self.dir.join(".session.json");
        fs::write(metadata_path, serde_json::to_string_pretty(&metadata)?)?;

        Ok(())
    }

    /// Get path for a state capture file
    pub fn state_path(&self, step: usize, input: Option<&str>) -> PathBuf {
        let filename = if step == 0 {
            "state_0_initial.png".to_string()
        } else {
            let input_name = input
                .map(|s| format!("_{}", sanitize_name(s)))
                .unwrap_or_default();
            format!("state_{}{}.png", step, input_name)
        };
        self.dir.join(filename)
    }

    /// Get path for a single capture file
    pub fn capture_path(&self, name: &str) -> PathBuf {
        let filename = format!("{}.png", sanitize_name(name));
        self.dir.join(filename)
    }

    /// Get subdirectory for a specific terminal size
    pub fn size_subdir(&self, cols: u16, rows: u16) -> PathBuf {
        self.dir.join(format!("{}x{}", cols, rows))
    }

    /// List all PNG files in the session
    pub fn list_captures(&self) -> std::io::Result<Vec<PathBuf>> {
        let mut captures = Vec::new();
        if self.dir.exists() {
            for entry in fs::read_dir(&self.dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map(|e| e == "png").unwrap_or(false) {
                    captures.push(path);
                }
            }
        }
        captures.sort();
        Ok(captures)
    }

    /// Clean up the session directory
    pub fn cleanup(&self) -> std::io::Result<()> {
        if self.dir.exists() && !self.keep {
            fs::remove_dir_all(&self.dir)?;
        }
        Ok(())
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// Generate a unique session ID
fn generate_session_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let pid = std::process::id();
    format!("session_{}_{}", timestamp, pid)
}

/// Generate a timestamp suffix
fn generate_timestamp_suffix() -> String {
    chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string()
}

/// Sanitize a name for use in filenames
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            ' ' | '+' | '/' | '\\' => '_',
            _ => '_',
        })
        .collect()
}

/// Clean up old sessions older than the specified duration
pub fn cleanup_old_sessions(max_age: std::time::Duration) -> std::io::Result<usize> {
    let base = PathBuf::from(SESSION_BASE_DIR);
    if !base.exists() {
        return Ok(0);
    }

    let now = SystemTime::now();
    let mut cleaned = 0;

    for entry in fs::read_dir(&base)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age > max_age {
                            if fs::remove_dir_all(&path).is_ok() {
                                cleaned += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(cleaned)
}

/// List all existing sessions
pub fn list_sessions() -> std::io::Result<Vec<PathBuf>> {
    let base = PathBuf::from(SESSION_BASE_DIR);
    if !base.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(&base)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            sessions.push(path);
        }
    }
    sessions.sort();
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let session = Session::new();
        assert!(session.id.starts_with("session_"));
        assert!(session.dir.starts_with(SESSION_BASE_DIR));
        assert!(!session.keep);
    }

    #[test]
    fn test_session_with_name() {
        let session = Session::with_name("my-test");
        assert!(session.id.starts_with("my-test_"));
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("hello world"), "hello_world");
        assert_eq!(sanitize_name("ctrl+c"), "ctrl_c");
        assert_eq!(sanitize_name("a/b\\c"), "a_b_c");
    }

    #[test]
    fn test_state_path() {
        let session = Session::new();
        assert!(session.state_path(0, None).ends_with("state_0_initial.png"));
        assert!(session.state_path(1, Some("down")).ends_with("state_1_down.png"));
        assert!(session.state_path(2, Some("ctrl+c")).ends_with("state_2_ctrl_c.png"));
    }
}

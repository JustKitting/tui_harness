//! Capture backend abstraction for cross-platform screenshot capture.
//!
//! This module provides a unified interface for different capture methods:
//! - PTY-based terminal rendering (cross-platform CLI capture)
//! - MockFramebuffer (testing and virtual display)

use font8x8::{BASIC_FONTS, UnicodeFonts};
use image::{ImageBuffer, RgbImage};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use super::types::{SnapshotError, SnapshotResult};
use crate::harness::types::InputAction;

/// Result of a capture operation
#[derive(Debug, Clone)]
pub struct CaptureResult {
    /// PNG-encoded image data
    pub image_data: Vec<u8>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Optional metadata about the capture
    pub metadata: Option<serde_json::Value>,
}

/// Trait for capture backends
///
/// Implementations provide different methods of capturing visual output:
/// - `PtyBackend` for terminal applications via PTY emulation
/// - `MockFramebuffer` for testing with programmatic drawing
pub trait CaptureBackend: Send + Sync {
    /// Perform a capture and return the result
    fn capture(&mut self) -> SnapshotResult<CaptureResult>;

    /// Get the source type identifier (e.g., "cli_pty", "mock", "web")
    fn source_type(&self) -> &str;

    /// Get the current width in pixels
    fn width(&self) -> u32;

    /// Get the current height in pixels
    fn height(&self) -> u32;
}

/// A virtual framebuffer for testing and programmatic drawing
///
/// Provides a full drawing API for creating test fixtures:
/// - `fill()` - Fill entire buffer with a color
/// - `draw_rect()` - Draw a filled rectangle
/// - `draw_text()` - Draw text using font8x8 glyphs
/// - `get_pixel()` / `set_pixel()` - Direct pixel access
#[derive(Debug, Clone)]
pub struct MockFramebuffer {
    /// Width in pixels
    width: u32,
    /// Height in pixels
    height: u32,
    /// RGB pixel buffer (row-major, 3 bytes per pixel)
    buffer: Vec<u8>,
}

impl MockFramebuffer {
    /// Create a new framebuffer with the given dimensions, initialized to black
    pub fn new(width: u32, height: u32) -> Self {
        let buffer = vec![0u8; (width * height * 3) as usize];
        Self {
            width,
            height,
            buffer,
        }
    }

    /// Create a framebuffer initialized to a specific color
    pub fn with_color(width: u32, height: u32, color: [u8; 3]) -> Self {
        let mut fb = Self::new(width, height);
        fb.fill(color);
        fb
    }

    /// Load a framebuffer from PNG image bytes
    pub fn from_png_bytes(data: &[u8]) -> SnapshotResult<Self> {
        let img = image::load_from_memory(data)
            .map_err(|e| SnapshotError::Capture(format!("Failed to load PNG: {}", e)))?;
        let rgb = img.to_rgb8();
        Ok(Self {
            width: rgb.width(),
            height: rgb.height(),
            buffer: rgb.into_raw(),
        })
    }

    /// Load a framebuffer from raw RGB bytes
    pub fn from_raw_rgb(width: u32, height: u32, data: Vec<u8>) -> SnapshotResult<Self> {
        let expected = (width * height * 3) as usize;
        if data.len() != expected {
            return Err(SnapshotError::Capture(format!(
                "Buffer size mismatch: expected {} bytes, got {}",
                expected,
                data.len()
            )));
        }
        Ok(Self {
            width,
            height,
            buffer: data,
        })
    }

    /// Fill the entire framebuffer with a color
    pub fn fill(&mut self, color: [u8; 3]) {
        for chunk in self.buffer.chunks_exact_mut(3) {
            chunk[0] = color[0];
            chunk[1] = color[1];
            chunk[2] = color[2];
        }
    }

    /// Draw a filled rectangle
    pub fn draw_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 3]) {
        for py in y..(y + h).min(self.height) {
            for px in x..(x + w).min(self.width) {
                self.set_pixel(px, py, color);
            }
        }
    }

    /// Draw text using font8x8 glyphs
    ///
    /// Each character is 8x8 pixels. Text does not wrap.
    pub fn draw_text(&mut self, x: u32, y: u32, text: &str, fg: [u8; 3], bg: [u8; 3]) {
        let mut cursor_x = x;
        for ch in text.chars() {
            self.draw_char(cursor_x, y, ch, fg, bg);
            cursor_x += 8;
            if cursor_x >= self.width {
                break;
            }
        }
    }

    /// Draw a single character using font8x8
    fn draw_char(&mut self, x: u32, y: u32, ch: char, fg: [u8; 3], bg: [u8; 3]) {
        let glyph = BASIC_FONTS.get(ch).unwrap_or([0u8; 8]);
        for (row_idx, row) in glyph.iter().enumerate() {
            let py = y + row_idx as u32;
            if py >= self.height {
                break;
            }
            for bit in 0..8 {
                let px = x + bit;
                if px >= self.width {
                    break;
                }
                // font8x8 stores LSB as leftmost pixel
                let is_fg = (row >> bit) & 1 == 1;
                let color = if is_fg { fg } else { bg };
                self.set_pixel(px, py, color);
            }
        }
    }

    /// Get the color of a pixel
    pub fn get_pixel(&self, x: u32, y: u32) -> [u8; 3] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0];
        }
        let idx = ((y * self.width + x) * 3) as usize;
        [self.buffer[idx], self.buffer[idx + 1], self.buffer[idx + 2]]
    }

    /// Set the color of a pixel
    pub fn set_pixel(&mut self, x: u32, y: u32, color: [u8; 3]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y * self.width + x) * 3) as usize;
        self.buffer[idx] = color[0];
        self.buffer[idx + 1] = color[1];
        self.buffer[idx + 2] = color[2];
    }

    /// Get the raw RGB buffer
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }

    /// Convert to an image buffer
    pub fn to_image(&self) -> RgbImage {
        ImageBuffer::from_raw(self.width, self.height, self.buffer.clone())
            .expect("Buffer size should match dimensions")
    }

    /// Encode the framebuffer as PNG bytes
    pub fn to_png(&self) -> SnapshotResult<Vec<u8>> {
        let img = self.to_image();
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .map_err(|e| SnapshotError::Capture(format!("Failed to encode PNG: {}", e)))?;
        Ok(bytes)
    }
}

impl CaptureBackend for MockFramebuffer {
    fn capture(&mut self) -> SnapshotResult<CaptureResult> {
        let image_data = self.to_png()?;
        Ok(CaptureResult {
            image_data,
            width: self.width,
            height: self.height,
            metadata: Some(serde_json::json!({
                "mock": true
            })),
        })
    }

    fn source_type(&self) -> &str {
        "mock"
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }
}

/// Configuration for PTY-based CLI capture
#[derive(Debug, Clone)]
pub struct PtyBackendConfig {
    /// Path to the binary to execute
    pub binary_path: PathBuf,
    /// Arguments to pass to the binary
    pub args: Vec<String>,
    /// Input actions to send after launch
    pub inputs: Vec<InputAction>,
    /// Terminal width in columns (default: 120)
    pub terminal_width: u16,
    /// Terminal height in rows (default: 40)
    pub terminal_height: u16,
}

impl Default for PtyBackendConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::new(),
            args: Vec::new(),
            inputs: Vec::new(),
            terminal_width: 120,
            terminal_height: 40,
        }
    }
}

impl PtyBackendConfig {
    /// Create a new PTY backend config for the given binary
    pub fn new(binary_path: impl Into<PathBuf>) -> Self {
        Self {
            binary_path: binary_path.into(),
            ..Default::default()
        }
    }

    /// Add an argument
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple arguments
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Add an input action
    pub fn input(mut self, action: InputAction) -> Self {
        self.inputs.push(action);
        self
    }

    /// Add multiple input actions
    pub fn inputs(mut self, actions: impl IntoIterator<Item = InputAction>) -> Self {
        self.inputs.extend(actions);
        self
    }

    /// Set terminal dimensions
    pub fn size(mut self, width: u16, height: u16) -> Self {
        self.terminal_width = width;
        self.terminal_height = height;
        self
    }
}

/// PTY-based capture backend for CLI applications
///
/// Spawns a CLI application in a pseudo-terminal, sends input actions,
/// and renders the terminal buffer to an image.
pub struct PtyBackend {
    config: PtyBackendConfig,
}

impl PtyBackend {
    /// Create a new PTY backend with the given configuration
    pub fn new(config: PtyBackendConfig) -> Self {
        Self { config }
    }

    /// Create a PTY backend for the given binary path
    pub fn for_binary(path: impl Into<PathBuf>) -> Self {
        Self::new(PtyBackendConfig::new(path))
    }
}

impl CaptureBackend for PtyBackend {
    fn capture(&mut self) -> SnapshotResult<CaptureResult> {
        use super::pty::{Vt100Parser, CELL_HEIGHT, CELL_WIDTH};
        use portable_pty::{native_pty_system, CommandBuilder, PtySize};
        use std::io::{Read, Write};
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        let terminal_width = self.config.terminal_width;
        let terminal_height = self.config.terminal_height;
        let mut parser = Vt100Parser::new(u32::from(terminal_width), u32::from(terminal_height));

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: terminal_height,
                cols: terminal_width,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| SnapshotError::Capture(format!("Failed to open PTY: {}", e)))?;

        let binary_path = self.config.binary_path.to_string_lossy().to_string();
        let mut cmd = CommandBuilder::new(&binary_path);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLUMNS", terminal_width.to_string());
        cmd.env("LINES", terminal_height.to_string());
        for arg in &self.config.args {
            cmd.arg(arg);
        }

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| SnapshotError::Capture(format!("Failed to spawn '{}': {}", binary_path, e)))?;
        drop(pair.slave);

        let _ = pair.master.resize(PtySize {
            rows: terminal_height,
            cols: terminal_width,
            pixel_width: 0,
            pixel_height: 0,
        });

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| SnapshotError::Capture(format!("Failed to clone PTY reader: {}", e)))?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|e| SnapshotError::Capture(format!("Failed to take PTY writer: {}", e)))?;

        // Spawn reader thread
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = reader;
            let mut buffer = [0u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => {
                        if tx.send(buffer[..size].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        // Wait for initial render
        drain_until_quiet(&rx, &mut parser, Duration::from_millis(180));

        // Send inputs
        for input in &self.config.inputs {
            match input {
                InputAction::SendString(text) => {
                    let _ = writer.write_all(text.as_bytes());
                    let _ = writer.write_all(&[b'\r']);
                    let _ = writer.flush();
                    drain_until_quiet(&rx, &mut parser, Duration::from_millis(180));
                }
                InputAction::SendKey(key) => {
                    let sequence = key_to_sequence(key);
                    let _ = writer.write_all(&sequence);
                    let _ = writer.flush();
                    drain_until_quiet(&rx, &mut parser, Duration::from_millis(180));
                }
            }
        }

        // Final drain and cleanup
        drain_until_quiet(&rx, &mut parser, Duration::from_millis(180));
        drop(writer);

        // Wait for process with timeout
        let start = std::time::Instant::now();
        let max_wait = Duration::from_secs(3);
        while start.elapsed() < max_wait {
            if let Ok(Some(_)) = child.try_wait() {
                drain_until_quiet(&rx, &mut parser, Duration::from_millis(180));
                break;
            }
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(60)) {
                for byte in chunk {
                    parser.process_byte(byte);
                }
            }
        }

        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Render to image
        let img = parser.terminal().render_to_image();
        let mut png_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            .map_err(|e| SnapshotError::Capture(format!("Failed to encode PNG: {}", e)))?;

        Ok(CaptureResult {
            image_data: png_bytes,
            width: u32::from(terminal_width) * CELL_WIDTH,
            height: u32::from(terminal_height) * CELL_HEIGHT,
            metadata: Some(serde_json::json!({
                "terminal_width": terminal_width,
                "terminal_height": terminal_height,
                "binary": binary_path,
            })),
        })
    }

    fn source_type(&self) -> &str {
        "cli_pty"
    }

    fn width(&self) -> u32 {
        use super::pty::CELL_WIDTH;
        u32::from(self.config.terminal_width) * CELL_WIDTH
    }

    fn height(&self) -> u32 {
        use super::pty::CELL_HEIGHT;
        u32::from(self.config.terminal_height) * CELL_HEIGHT
    }
}

/// Drain reader channel until quiet for the given duration
fn drain_until_quiet(
    rx: &mpsc::Receiver<Vec<u8>>,
    parser: &mut super::pty::Vt100Parser,
    quiet_window: Duration,
) {
    use std::time::Instant;

    let mut last_activity = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => {
                for byte in chunk {
                    parser.process_byte(byte);
                }
                last_activity = Instant::now();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if last_activity.elapsed() >= quiet_window {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    // Final drain of any buffered data
    while let Ok(chunk) = rx.try_recv() {
        for byte in chunk {
            parser.process_byte(byte);
        }
    }
}

/// Convert key name to VT100 sequence
fn key_to_sequence(key: &str) -> Vec<u8> {
    match key.to_lowercase().as_str() {
        "up" => b"\x1b[A".to_vec(),
        "down" => b"\x1b[B".to_vec(),
        "right" => b"\x1b[C".to_vec(),
        "left" => b"\x1b[D".to_vec(),
        "enter" => vec![b'\r'],
        "space" => vec![b' '],
        "tab" => vec![b'\t'],
        "backspace" => vec![0x08],
        "escape" | "esc" => vec![0x1b],
        other if other.len() == 1 => other.as_bytes().to_vec(),
        other => other.as_bytes().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_framebuffer_new() {
        let fb = MockFramebuffer::new(100, 50);
        assert_eq!(fb.width(), 100);
        assert_eq!(fb.height(), 50);
        // Should be initialized to black
        assert_eq!(fb.get_pixel(0, 0), [0, 0, 0]);
        assert_eq!(fb.get_pixel(99, 49), [0, 0, 0]);
    }

    #[test]
    fn test_mock_framebuffer_fill() {
        let mut fb = MockFramebuffer::new(10, 10);
        fb.fill([255, 128, 64]);
        assert_eq!(fb.get_pixel(0, 0), [255, 128, 64]);
        assert_eq!(fb.get_pixel(5, 5), [255, 128, 64]);
        assert_eq!(fb.get_pixel(9, 9), [255, 128, 64]);
    }

    #[test]
    fn test_mock_framebuffer_draw_rect() {
        let mut fb = MockFramebuffer::new(20, 20);
        fb.fill([0, 0, 0]);
        fb.draw_rect(5, 5, 10, 10, [255, 0, 0]);

        // Outside rect
        assert_eq!(fb.get_pixel(0, 0), [0, 0, 0]);
        assert_eq!(fb.get_pixel(4, 4), [0, 0, 0]);

        // Inside rect
        assert_eq!(fb.get_pixel(5, 5), [255, 0, 0]);
        assert_eq!(fb.get_pixel(10, 10), [255, 0, 0]);
        assert_eq!(fb.get_pixel(14, 14), [255, 0, 0]);

        // Just outside rect
        assert_eq!(fb.get_pixel(15, 15), [0, 0, 0]);
    }

    #[test]
    fn test_mock_framebuffer_draw_text() {
        let mut fb = MockFramebuffer::new(80, 16);
        fb.fill([0, 0, 0]);
        fb.draw_text(0, 0, "Hi", [255, 255, 255], [0, 0, 0]);

        // 'H' should have some white pixels (it's not empty)
        let mut has_white = false;
        for y in 0..8 {
            for x in 0..8 {
                if fb.get_pixel(x, y) == [255, 255, 255] {
                    has_white = true;
                    break;
                }
            }
        }
        assert!(has_white, "Character 'H' should have some foreground pixels");
    }

    #[test]
    fn test_mock_framebuffer_capture() {
        let mut fb = MockFramebuffer::with_color(50, 50, [128, 128, 128]);
        let result = fb.capture().unwrap();

        assert_eq!(result.width, 50);
        assert_eq!(result.height, 50);
        assert!(!result.image_data.is_empty());
        // Check PNG magic bytes
        assert_eq!(&result.image_data[0..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_mock_framebuffer_roundtrip() {
        let mut fb = MockFramebuffer::new(32, 32);
        fb.fill([100, 150, 200]);
        fb.draw_rect(8, 8, 16, 16, [255, 0, 0]);

        let png = fb.to_png().unwrap();
        let fb2 = MockFramebuffer::from_png_bytes(&png).unwrap();

        assert_eq!(fb2.width(), fb.width());
        assert_eq!(fb2.height(), fb.height());
        assert_eq!(fb2.get_pixel(0, 0), [100, 150, 200]);
        assert_eq!(fb2.get_pixel(10, 10), [255, 0, 0]);
    }
}

// =============================================================================
// Capture utilities (merged from capture.rs)
// =============================================================================

use std::fs;
use crate::snapshot::utils::{
    generate_filename, generate_timestamp, write_description, write_manifest,
};
use crate::snapshot::{Snapshot, SnapshotConfig};

/// Capture a snapshot using the provided backend and save it according to config.
pub fn capture_with_backend(
    backend: &mut dyn CaptureBackend,
    config: &SnapshotConfig,
) -> SnapshotResult<Snapshot> {
    fs::create_dir_all(&config.output_dir)?;

    let timestamp = generate_timestamp();
    let filename = generate_filename(backend.source_type(), &timestamp);
    let image_path = config.output_dir.join(&filename);

    let result = backend.capture()?;
    fs::write(&image_path, &result.image_data)?;

    let metadata = if config.include_metadata {
        let mut meta = crate::snapshot::utils::create_base_metadata(
            result.width,
            result.height,
            backend.source_type(),
            &timestamp,
        );
        if let Some(serde_json::Value::Object(extra)) = result.metadata {
            for (k, v) in extra {
                meta.insert(k, v);
            }
        }
        Some(serde_json::Value::Object(meta))
    } else {
        None
    };

    let snapshot = Snapshot::new(image_path.clone(), backend.source_type().to_string(), metadata);

    write_manifest(&snapshot, config)?;
    write_description(&snapshot, config)?;

    Ok(snapshot)
}

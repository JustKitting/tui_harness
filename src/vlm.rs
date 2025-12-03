//! Vision Language Model (VLM) client with streaming support.
//!
//! Provides robust VLM API communication with:
//! - Streaming responses (no total timeout, activity-based timeout)
//! - Connection health checks
//! - Progress callbacks for long-running analysis
//!
//! # Configuration
//!
//! VLM settings can be configured via environment variables:
//! - `CLI_VISION_VLM_ENDPOINT`: API endpoint URL
//! - `CLI_VISION_VLM_MODEL`: Model name
//! - `CLI_VISION_VLM_MAX_TOKENS`: Max tokens in response
//! - `CLI_VISION_VLM_TIMEOUT`: Activity timeout (seconds)
//! - `CLI_VISION_VLM_CONNECT_TIMEOUT`: Connection timeout (seconds)

use base64::Engine;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::config;

/// Result type for VLM operations
pub type VlmResult<T> = Result<T, VlmError>;

/// Errors that can occur during VLM operations
#[derive(Debug)]
pub enum VlmError {
    /// Failed to connect to the VLM endpoint
    ConnectionFailed(String),
    /// No activity for too long during streaming
    ActivityTimeout(Duration),
    /// Invalid response from the VLM
    InvalidResponse(String),
    /// IO error
    Io(std::io::Error),
}

impl std::fmt::Display for VlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VlmError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            VlmError::ActivityTimeout(d) => write!(f, "No response for {:?}", d),
            VlmError::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
            VlmError::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for VlmError {}

impl From<std::io::Error> for VlmError {
    fn from(e: std::io::Error) -> Self {
        VlmError::Io(e)
    }
}

/// Configuration for VLM client
#[derive(Debug, Clone)]
pub struct VlmConfig {
    /// API endpoint URL
    pub endpoint: String,
    /// Model name to use
    pub model: String,
    /// Maximum tokens in response
    pub max_tokens: u32,
    /// Timeout for initial connection (seconds)
    pub connection_timeout: u64,
    /// Timeout for inactivity during streaming (seconds)
    pub activity_timeout: u64,
}

impl Default for VlmConfig {
    fn default() -> Self {
        let cfg = config::get();
        Self {
            endpoint: cfg.vlm.endpoint.clone(),
            model: cfg.vlm.model.clone(),
            max_tokens: cfg.vlm.max_tokens,
            connection_timeout: cfg.vlm.connect_timeout,
            activity_timeout: cfg.vlm.activity_timeout,
        }
    }
}

impl VlmConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            ..Default::default()
        }
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    pub fn activity_timeout(mut self, seconds: u64) -> Self {
        self.activity_timeout = seconds;
        self
    }
}

/// Progress update during VLM analysis
#[derive(Debug, Clone)]
pub enum VlmProgress {
    /// Connection established
    Connected,
    /// Receiving data (partial content so far)
    Receiving(String),
    /// Analysis complete
    Complete(String),
    /// Error occurred
    Error(String),
}

/// Check if a VLM endpoint is reachable (connection-only check).
///
/// This only verifies the server accepts TCP connections - it doesn't wait
/// for a full response since VLM requests can take 30+ seconds for large images.
pub fn check_health(endpoint: &str, timeout_secs: u64) -> VlmResult<bool> {
    // Extract host:port from endpoint URL for connection test
    let url = endpoint.trim_start_matches("http://").trim_start_matches("https://");
    let host_port = url.split('/').next().unwrap_or("127.0.0.1:8080");

    // Use curl to just test if we can connect (not wait for response)
    let output = Command::new("curl")
        .args([
            "-s",
            "-o", "/dev/null",
            "-w", "%{http_code}",
            "--connect-timeout", &timeout_secs.to_string(),
            "--max-time", &timeout_secs.to_string(),
            "-I", // HEAD request - just check if server responds to connection
            &format!("http://{}", host_port),
        ])
        .output()?;

    let status = String::from_utf8_lossy(&output.stdout);
    // Any response (even 4xx/5xx) means server is reachable
    // 000 means connection failed entirely
    let code: u16 = status.trim().parse().unwrap_or(0);
    Ok(code > 0)
}

/// Analyze an image with the VLM using streaming to avoid timeouts
pub fn analyze_image(
    config: &VlmConfig,
    image_data: &[u8],
    prompt: &str,
) -> VlmResult<String> {
    analyze_image_with_progress(config, image_data, prompt, |_| {})
}

/// Analyze an image with progress callbacks
pub fn analyze_image_with_progress<F>(
    config: &VlmConfig,
    image_data: &[u8],
    prompt: &str,
    mut on_progress: F,
) -> VlmResult<String>
where
    F: FnMut(VlmProgress),
{
    let img_base64 = base64::engine::general_purpose::STANDARD.encode(image_data);

    let request = serde_json::json!({
        "model": config.model,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/png;base64,{}", img_base64)
                    }
                },
                {
                    "type": "text",
                    "text": prompt
                }
            ]
        }],
        "max_tokens": config.max_tokens,
        "stream": true
    });

    let request_json = serde_json::to_string(&request)
        .map_err(|e| VlmError::InvalidResponse(e.to_string()))?;

    // Spawn curl with streaming
    let mut child = Command::new("curl")
        .args([
            "-s",
            "-N", // Disable buffering for streaming
            "-X", "POST",
            &config.endpoint,
            "-H", "Content-Type: application/json",
            "-d", &request_json,
            "--connect-timeout", &config.connection_timeout.to_string(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take()
        .ok_or_else(|| VlmError::Io(std::io::Error::other("Failed to capture stdout")))?;

    // Read streaming response with activity timeout
    let (tx, rx) = mpsc::channel();
    let activity_timeout = Duration::from_secs(config.activity_timeout);

    // Spawn reader thread
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if tx.send(Ok(line)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                    break;
                }
            }
        }
    });

    on_progress(VlmProgress::Connected);

    let mut full_content = String::new();
    let mut last_activity = Instant::now();

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(line)) => {
                last_activity = Instant::now();

                // Parse SSE data
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        break;
                    }

                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        // Extract delta content
                        if let Some(content) = json["choices"][0]["delta"]["content"].as_str() {
                            full_content.push_str(content);
                            on_progress(VlmProgress::Receiving(full_content.clone()));
                        }
                        // Also check for reasoning_content (thinking models)
                        if let Some(content) = json["choices"][0]["delta"]["reasoning_content"].as_str() {
                            full_content.push_str(content);
                            on_progress(VlmProgress::Receiving(full_content.clone()));
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                return Err(VlmError::Io(e));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if last_activity.elapsed() > activity_timeout {
                    let _ = child.kill();
                    return Err(VlmError::ActivityTimeout(activity_timeout));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    // Wait for process to finish
    let status = child.wait()?;

    if !status.success() && full_content.is_empty() {
        return Err(VlmError::ConnectionFailed("curl process failed".to_string()));
    }

    // If streaming didn't work, try parsing as non-streaming response
    if full_content.is_empty() {
        // Fall back to non-streaming request
        return analyze_image_non_streaming(config, image_data, prompt);
    }

    on_progress(VlmProgress::Complete(full_content.clone()));
    Ok(full_content)
}

/// Fallback non-streaming analysis (for APIs that don't support streaming)
fn analyze_image_non_streaming(
    config: &VlmConfig,
    image_data: &[u8],
    prompt: &str,
) -> VlmResult<String> {
    let img_base64 = base64::engine::general_purpose::STANDARD.encode(image_data);

    let request = serde_json::json!({
        "model": config.model,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/png;base64,{}", img_base64)
                    }
                },
                {
                    "type": "text",
                    "text": prompt
                }
            ]
        }],
        "max_tokens": config.max_tokens
    });

    let request_json = serde_json::to_string(&request)
        .map_err(|e| VlmError::InvalidResponse(e.to_string()))?;

    // Use a very long timeout for non-streaming (since we can't detect activity)
    let output = Command::new("curl")
        .args([
            "-s",
            "-X", "POST",
            &config.endpoint,
            "-H", "Content-Type: application/json",
            "-d", &request_json,
            "--connect-timeout", &config.connection_timeout.to_string(),
            // No --max-time for non-streaming - let it run
        ])
        .output()?;

    if !output.status.success() {
        return Err(VlmError::ConnectionFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }

    let response: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| VlmError::InvalidResponse(e.to_string()))?;

    // Extract content
    let content = response["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");

    // Try reasoning_content for thinking models
    let result = if content.is_empty() {
        response["choices"][0]["message"]["reasoning_content"]
            .as_str()
            .unwrap_or("No description available")
    } else {
        content
    };

    Ok(result.to_string())
}

/// Build a prompt for analyzing a TUI screenshot
pub fn build_analysis_prompt(step: usize, input: Option<&str>, custom_prompt: Option<&str>) -> String {
    if let Some(custom) = custom_prompt {
        let input_str = input.unwrap_or("none");
        custom
            .replace("{step}", &step.to_string())
            .replace("{input}", input_str)
    } else if step == 0 {
        "Describe the initial state of this terminal application. What UI elements are visible? What is selected or highlighted?".to_string()
    } else {
        let input_str = input.unwrap_or("unknown");
        format!(
            "The user pressed '{}'. Describe what changed in this terminal application. What is the current state? What is now selected or highlighted?",
            input_str
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_analysis_prompt_initial() {
        let prompt = build_analysis_prompt(0, None, None);
        assert!(prompt.contains("initial state"));
    }

    #[test]
    fn test_build_analysis_prompt_with_input() {
        let prompt = build_analysis_prompt(1, Some("down"), None);
        assert!(prompt.contains("down"));
    }

    #[test]
    fn test_build_analysis_prompt_custom() {
        let prompt = build_analysis_prompt(2, Some("enter"), Some("Step {step}: Did pressing {input} work?"));
        assert_eq!(prompt, "Step 2: Did pressing enter work?");
    }

    #[test]
    fn test_vlm_config_builder() {
        let config = VlmConfig::new("http://localhost:8080")
            .model("llava")
            .max_tokens(200)
            .activity_timeout(30);

        assert_eq!(config.endpoint, "http://localhost:8080");
        assert_eq!(config.model, "llava");
        assert_eq!(config.max_tokens, 200);
        assert_eq!(config.activity_timeout, 30);
    }
}

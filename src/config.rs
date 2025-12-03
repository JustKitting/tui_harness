//! Configuration management with environment variable support.
//!
//! This module provides centralized configuration for CLI Vision, supporting:
//! - Environment variables for all configurable values
//! - Sensible defaults that match the original hardcoded values
//! - Builder pattern for programmatic configuration
//!
//! # Environment Variables
//!
//! | Variable | Description | Default |
//! |----------|-------------|---------|
//! | `CLI_VISION_VLM_ENDPOINT` | VLM API endpoint URL | `http://127.0.0.1:8080/v1/chat/completions` |
//! | `CLI_VISION_VLM_MODEL` | Model name for VLM | `qwen3` |
//! | `CLI_VISION_VLM_MAX_TOKENS` | Maximum tokens in VLM response | `400` |
//! | `CLI_VISION_VLM_TIMEOUT` | VLM activity timeout in seconds | `60` |
//! | `CLI_VISION_VLM_CONNECT_TIMEOUT` | VLM connection timeout in seconds | `10` |
//! | `CLI_VISION_SESSION_DIR` | Base directory for sessions | `/tmp/cli-vision` |
//! | `CLI_VISION_DEFAULT_DELAY` | Default delay between inputs (ms) | `100` |
//! | `CLI_VISION_DEFAULT_SIZE` | Default terminal size | `standard` |
//!
//! # Example
//!
//! ```bash
//! # Use a different VLM endpoint
//! export CLI_VISION_VLM_ENDPOINT="http://localhost:11434/v1/chat/completions"
//! export CLI_VISION_VLM_MODEL="llava"
//!
//! # Use a custom session directory
//! export CLI_VISION_SESSION_DIR="/var/tmp/cli-vision-sessions"
//! ```

use std::env;
use std::sync::OnceLock;

// ============================================================================
// Default Values (matching original hardcoded values)
// ============================================================================

/// Default VLM API endpoint
pub const DEFAULT_VLM_ENDPOINT: &str = "http://127.0.0.1:8080/v1/chat/completions";

/// Default VLM model name
pub const DEFAULT_VLM_MODEL: &str = "qwen3";

/// Default max tokens for VLM responses
pub const DEFAULT_VLM_MAX_TOKENS: u32 = 400;

/// Default VLM connection timeout (seconds)
pub const DEFAULT_VLM_CONNECT_TIMEOUT: u64 = 10;

/// Default VLM activity timeout (seconds)
pub const DEFAULT_VLM_ACTIVITY_TIMEOUT: u64 = 60;

/// Default session base directory
pub const DEFAULT_SESSION_DIR: &str = "/tmp/cli-vision";

/// Default delay between inputs (milliseconds)
pub const DEFAULT_INPUT_DELAY: u64 = 100;

/// Default terminal size preset
pub const DEFAULT_TERMINAL_SIZE: &str = "standard";

/// Default terminal width (columns)
pub const DEFAULT_TERMINAL_WIDTH: u16 = 120;

/// Default terminal height (rows)
pub const DEFAULT_TERMINAL_HEIGHT: u16 = 40;

/// Default mock screenshot width (pixels)
pub const DEFAULT_MOCK_WIDTH: u32 = 800;

/// Default mock screenshot height (pixels)
pub const DEFAULT_MOCK_HEIGHT: u32 = 600;

// ============================================================================
// Environment Variable Names
// ============================================================================

/// Environment variable for VLM endpoint
pub const ENV_VLM_ENDPOINT: &str = "CLI_VISION_VLM_ENDPOINT";

/// Environment variable for VLM model
pub const ENV_VLM_MODEL: &str = "CLI_VISION_VLM_MODEL";

/// Environment variable for VLM max tokens
pub const ENV_VLM_MAX_TOKENS: &str = "CLI_VISION_VLM_MAX_TOKENS";

/// Environment variable for VLM connection timeout
pub const ENV_VLM_CONNECT_TIMEOUT: &str = "CLI_VISION_VLM_CONNECT_TIMEOUT";

/// Environment variable for VLM activity timeout
pub const ENV_VLM_ACTIVITY_TIMEOUT: &str = "CLI_VISION_VLM_TIMEOUT";

/// Environment variable for session directory
pub const ENV_SESSION_DIR: &str = "CLI_VISION_SESSION_DIR";

/// Environment variable for default input delay
pub const ENV_DEFAULT_DELAY: &str = "CLI_VISION_DEFAULT_DELAY";

/// Environment variable for default terminal size
pub const ENV_DEFAULT_SIZE: &str = "CLI_VISION_DEFAULT_SIZE";

// ============================================================================
// Legacy Environment Variable Support (for backwards compatibility)
// ============================================================================

/// Legacy environment variable for VLM endpoint (used by MCP server)
pub const ENV_VLM_ENDPOINT_LEGACY: &str = "VLM_ENDPOINT";

/// Legacy environment variable for CLI Vision binary path (used by MCP server)
pub const ENV_CLI_VISION_PATH: &str = "CLI_VISION_PATH";

// ============================================================================
// Configuration Getters (with caching)
// ============================================================================

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Get the global configuration (initialized from environment on first access)
pub fn get() -> &'static Config {
    CONFIG.get_or_init(Config::from_env)
}

/// Centralized configuration for CLI Vision
#[derive(Debug, Clone)]
pub struct Config {
    /// VLM configuration
    pub vlm: VlmSettings,
    /// Session configuration
    pub session: SessionSettings,
    /// Default values for CLI arguments
    pub defaults: DefaultSettings,
}

/// VLM-related settings
#[derive(Debug, Clone)]
pub struct VlmSettings {
    /// API endpoint URL
    pub endpoint: String,
    /// Model name
    pub model: String,
    /// Maximum tokens in response
    pub max_tokens: u32,
    /// Connection timeout (seconds)
    pub connect_timeout: u64,
    /// Activity timeout during streaming (seconds)
    pub activity_timeout: u64,
}

/// Session-related settings
#[derive(Debug, Clone)]
pub struct SessionSettings {
    /// Base directory for session storage
    pub base_dir: String,
}

/// Default values for CLI arguments
#[derive(Debug, Clone)]
pub struct DefaultSettings {
    /// Default delay between inputs (milliseconds)
    pub input_delay: u64,
    /// Default terminal size preset
    pub terminal_size: String,
    /// Default terminal width
    pub terminal_width: u16,
    /// Default terminal height
    pub terminal_height: u16,
    /// Default mock width
    pub mock_width: u32,
    /// Default mock height
    pub mock_height: u32,
}

impl Config {
    /// Create configuration from environment variables, falling back to defaults
    pub fn from_env() -> Self {
        Self {
            vlm: VlmSettings::from_env(),
            session: SessionSettings::from_env(),
            defaults: DefaultSettings::from_env(),
        }
    }

    /// Create configuration with all defaults (ignoring environment)
    pub fn defaults() -> Self {
        Self {
            vlm: VlmSettings::defaults(),
            session: SessionSettings::defaults(),
            defaults: DefaultSettings::defaults(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

impl VlmSettings {
    /// Create VLM settings from environment variables
    pub fn from_env() -> Self {
        Self {
            endpoint: env::var(ENV_VLM_ENDPOINT)
                .or_else(|_| env::var(ENV_VLM_ENDPOINT_LEGACY))
                .unwrap_or_else(|_| DEFAULT_VLM_ENDPOINT.to_string()),
            model: env::var(ENV_VLM_MODEL)
                .unwrap_or_else(|_| DEFAULT_VLM_MODEL.to_string()),
            max_tokens: env::var(ENV_VLM_MAX_TOKENS)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_VLM_MAX_TOKENS),
            connect_timeout: env::var(ENV_VLM_CONNECT_TIMEOUT)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_VLM_CONNECT_TIMEOUT),
            activity_timeout: env::var(ENV_VLM_ACTIVITY_TIMEOUT)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_VLM_ACTIVITY_TIMEOUT),
        }
    }

    /// Create VLM settings with defaults
    pub fn defaults() -> Self {
        Self {
            endpoint: DEFAULT_VLM_ENDPOINT.to_string(),
            model: DEFAULT_VLM_MODEL.to_string(),
            max_tokens: DEFAULT_VLM_MAX_TOKENS,
            connect_timeout: DEFAULT_VLM_CONNECT_TIMEOUT,
            activity_timeout: DEFAULT_VLM_ACTIVITY_TIMEOUT,
        }
    }
}

impl SessionSettings {
    /// Create session settings from environment variables
    pub fn from_env() -> Self {
        Self {
            base_dir: env::var(ENV_SESSION_DIR)
                .unwrap_or_else(|_| DEFAULT_SESSION_DIR.to_string()),
        }
    }

    /// Create session settings with defaults
    pub fn defaults() -> Self {
        Self {
            base_dir: DEFAULT_SESSION_DIR.to_string(),
        }
    }
}

impl DefaultSettings {
    /// Create default settings from environment variables
    pub fn from_env() -> Self {
        let terminal_size = env::var(ENV_DEFAULT_SIZE)
            .unwrap_or_else(|_| DEFAULT_TERMINAL_SIZE.to_string());

        // Parse terminal size to get dimensions
        let (width, height) = parse_terminal_size(&terminal_size)
            .unwrap_or((DEFAULT_TERMINAL_WIDTH, DEFAULT_TERMINAL_HEIGHT));

        Self {
            input_delay: env::var(ENV_DEFAULT_DELAY)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_INPUT_DELAY),
            terminal_size,
            terminal_width: width,
            terminal_height: height,
            mock_width: DEFAULT_MOCK_WIDTH,
            mock_height: DEFAULT_MOCK_HEIGHT,
        }
    }

    /// Create default settings with hardcoded defaults
    pub fn defaults() -> Self {
        Self {
            input_delay: DEFAULT_INPUT_DELAY,
            terminal_size: DEFAULT_TERMINAL_SIZE.to_string(),
            terminal_width: DEFAULT_TERMINAL_WIDTH,
            terminal_height: DEFAULT_TERMINAL_HEIGHT,
            mock_width: DEFAULT_MOCK_WIDTH,
            mock_height: DEFAULT_MOCK_HEIGHT,
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse a terminal size string into (width, height)
/// Supports: "compact" (80x24), "standard" (120x40), "large" (160x50), "xl" (200x60), or "WxH"
fn parse_terminal_size(size: &str) -> Option<(u16, u16)> {
    match size.to_lowercase().as_str() {
        "compact" => Some((80, 24)),
        "standard" => Some((120, 40)),
        "large" => Some((160, 50)),
        "xl" => Some((200, 60)),
        custom => {
            let parts: Vec<&str> = custom.split('x').collect();
            if parts.len() == 2 {
                let w = parts[0].parse().ok()?;
                let h = parts[1].parse().ok()?;
                Some((w, h))
            } else {
                None
            }
        }
    }
}

/// Get VLM endpoint from environment (convenience function)
pub fn vlm_endpoint() -> String {
    get().vlm.endpoint.clone()
}

/// Get VLM model from environment (convenience function)
pub fn vlm_model() -> String {
    get().vlm.model.clone()
}

/// Get session base directory (convenience function)
pub fn session_base_dir() -> String {
    get().session.base_dir.clone()
}

/// Get default input delay (convenience function)
pub fn default_input_delay() -> u64 {
    get().defaults.input_delay
}

/// Get default terminal size (convenience function)
pub fn default_terminal_size() -> String {
    get().defaults.terminal_size.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_terminal_size_presets() {
        assert_eq!(parse_terminal_size("compact"), Some((80, 24)));
        assert_eq!(parse_terminal_size("standard"), Some((120, 40)));
        assert_eq!(parse_terminal_size("large"), Some((160, 50)));
        assert_eq!(parse_terminal_size("xl"), Some((200, 60)));
    }

    #[test]
    fn test_parse_terminal_size_custom() {
        assert_eq!(parse_terminal_size("100x30"), Some((100, 30)));
        assert_eq!(parse_terminal_size("200x80"), Some((200, 80)));
    }

    #[test]
    fn test_parse_terminal_size_invalid() {
        assert_eq!(parse_terminal_size("invalid"), None);
        assert_eq!(parse_terminal_size("100"), None);
    }

    #[test]
    fn test_config_defaults() {
        let config = Config::defaults();
        assert_eq!(config.vlm.endpoint, DEFAULT_VLM_ENDPOINT);
        assert_eq!(config.vlm.model, DEFAULT_VLM_MODEL);
        assert_eq!(config.session.base_dir, DEFAULT_SESSION_DIR);
    }
}

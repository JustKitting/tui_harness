# CLI Vision

A cross-platform Rust tool for automated testing of terminal UI (TUI) applications. CLI Vision wraps binaries in a PTY, sends keyboard inputs, captures terminal screenshots as PNG images, and optionally analyzes each state with a vision-language model (VLM).

## Features

- **Cross-platform PTY capture** - No X11/display server required
- **VT100/ANSI terminal emulation** - Full color support (16, 256, 24-bit RGB)
- **Input automation** - Send keyboard inputs (arrows, function keys, ctrl combos, etc.)
- **Multi-state capture** - Capture screenshots after each input
- **VLM integration** - Optional AI-powered analysis of UI states
- **MCP server** - Integration with AI agents via Model Context Protocol

## Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/cli-vision.git
cd cli-vision

# Build
cargo build --release

# The binary will be at ./target/release/cli-vision
```

## Quick Start

### Capture a single screenshot

```bash
./target/release/cli-vision cli --binary /usr/bin/htop
```

### Run with inputs and capture each state

```bash
./target/release/cli-vision run \
  --binary /usr/bin/htop \
  --inputs "down,down,enter,q" \
  --delay 150
```

### With VLM analysis

```bash
./target/release/cli-vision run \
  --binary ./my-tui-app \
  --inputs "down,down,enter,escape" \
  --delay 150 \
  --analyze \
  --json
```

## Configuration

CLI Vision is highly configurable via environment variables and CLI arguments.

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `CLI_VISION_VLM_ENDPOINT` | VLM API endpoint URL | `http://127.0.0.1:8080/v1/chat/completions` |
| `CLI_VISION_VLM_MODEL` | Model name for VLM | `qwen3` |
| `CLI_VISION_VLM_MAX_TOKENS` | Max tokens in VLM response | `400` |
| `CLI_VISION_VLM_TIMEOUT` | VLM activity timeout (seconds) | `60` |
| `CLI_VISION_VLM_CONNECT_TIMEOUT` | VLM connection timeout (seconds) | `10` |
| `CLI_VISION_SESSION_DIR` | Base directory for sessions | `/tmp/cli-vision` |
| `CLI_VISION_DEFAULT_DELAY` | Default delay between inputs (ms) | `100` |
| `CLI_VISION_DEFAULT_SIZE` | Default terminal size | `standard` |
| `VLM_ENDPOINT` | Legacy: VLM endpoint (fallback) | - |
| `CLI_VISION_PATH` | Path to cli-vision binary (for MCP server) | auto-detected |

### Example Configuration

```bash
# Use a different VLM (e.g., Ollama with llava)
export CLI_VISION_VLM_ENDPOINT="http://localhost:11434/v1/chat/completions"
export CLI_VISION_VLM_MODEL="llava"

# Use a custom session directory
export CLI_VISION_SESSION_DIR="/var/tmp/cli-vision-sessions"

# Set defaults for all runs
export CLI_VISION_DEFAULT_DELAY="200"
export CLI_VISION_DEFAULT_SIZE="large"
```

## Commands

### `cli` - Single Screenshot Capture

Capture the initial state of a CLI application.

```bash
cli-vision cli --binary <PATH> [OPTIONS]

Options:
  -b, --binary <PATH>     Path to the binary to capture
  -o, --output <DIR>      Output directory for screenshots
  -k, --keep              Keep screenshots after completion
  -s, --size <SIZE>       Terminal size (compact, standard, large, xl, or WxH)
```

### `run` - Multi-State Capture with Inputs

Run an application with inputs and capture each state.

```bash
cli-vision run --binary <PATH> --inputs <INPUTS> [OPTIONS]

Options:
  -b, --binary <PATH>        Path to the binary
  -i, --inputs <INPUTS>      Comma-separated inputs (e.g., "down,down,enter")
  -a, --args <ARGS>          Arguments to pass to the binary
  -d, --delay <MS>           Delay between inputs (default: 100)
  -o, --output <DIR>         Output directory
  -k, --keep                 Keep screenshots
      --analyze              Analyze with VLM
      --vlm-endpoint <URL>   VLM endpoint URL
      --vlm-model <NAME>     VLM model name
      --prompt <PROMPT>      Custom analysis prompt
      --step-prompts <JSON>  Per-step prompts
      --json                 Output as JSON
  -s, --size <SIZE>          Terminal size
      --multi-size           Test with all preset sizes
```

### `mock` - Mock Framebuffer

Create test screenshots for development.

```bash
cli-vision mock --width 800 --height 600 --color ff0000 --output test.png
```

## Supported Keyboard Inputs

| Category | Keys |
|----------|------|
| Arrow keys | `up`, `down`, `left`, `right` |
| Navigation | `home`, `end`, `pageup`, `pagedown`, `insert`, `delete` |
| Common | `enter`, `space`, `tab`, `backspace`, `escape` |
| Function keys | `f1` through `f12` |
| Ctrl combos | `ctrl+a` through `ctrl+z` |
| Alt combos | `alt+<key>` |
| Characters | Any single printable character |

## Terminal Sizes

| Preset | Dimensions |
|--------|------------|
| `compact` | 80x24 |
| `standard` | 120x40 (default) |
| `large` | 160x50 |
| `xl` | 200x60 |
| Custom | `WxH` (e.g., `100x30`) |

## MCP Server Integration

CLI Vision includes an MCP (Model Context Protocol) server for integration with AI agents like Claude.

### Setup

```bash
# Install Python dependencies
cd mcp_server
pip install mcp

# Configure in your MCP client (e.g., ~/.config/opencode/opencode.json)
```

```json
{
  "mcp": {
    "cli-vision": {
      "type": "local",
      "command": ["python3", "/path/to/mcp_server/tui_qa_server.py"],
      "environment": {
        "CLI_VISION_PATH": "/path/to/target/release/cli-vision",
        "CLI_VISION_VLM_ENDPOINT": "http://127.0.0.1:8080/v1/chat/completions"
      }
    }
  }
}
```

### Available MCP Tools

- **`tui_test`** - Run TUI with inputs, capture & analyze each state
- **`tui_capture`** - Capture single screenshot of initial state
- **`list_supported_keys`** - List all supported keyboard inputs

## VLM Requirements

For AI-powered analysis, you need a VLM server that:
- Accepts OpenAI-compatible chat completions API
- Supports image input (base64-encoded)
- Supports streaming responses (recommended)

Compatible servers:
- [llama.cpp](https://github.com/ggerganov/llama.cpp) with vision models
- [Ollama](https://ollama.ai/) with llava, bakllava, etc.
- Any OpenAI-compatible API with vision support

## Development

```bash
# Run tests
cargo test

# Run benchmarks
cargo bench

# Build in release mode
cargo build --release
```

## Architecture

```
cli-vision run --binary app --inputs "down,enter"
    │
    ▼
┌─────────────────────────────────────────────────┐
│  1. Spawn app in PTY (portable-pty)             │
│  2. Capture initial state (step 0)              │
│  3. For each input:                             │
│     a. Wait delay_ms                            │
│     b. Send input bytes to PTY                  │
│     c. Wait for render to settle                │
│     d. Capture state as PNG                     │
│  4. If --analyze: Send each PNG to VLM          │
│  5. Output JSON with states and descriptions    │
└─────────────────────────────────────────────────┘
```

## License

MIT

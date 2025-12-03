# CLI Vision - MCP Server

An MCP (Model Context Protocol) server that provides terminal UI testing capabilities to AI agents like OpenCode. Allows agents to run CLI/TUI applications, send keyboard inputs, capture screenshots at each state, and get AI-powered descriptions of what changed.

## Installation

1. Build cli-vision:
   ```bash
   cd /path/to/cli-vision
   cargo build --release
   ```

2. Install Python dependencies:
   ```bash
   pip install -r requirements.txt
   ```

3. (Optional) Set environment variables:
   ```bash
   export CLI_VISION_PATH="/path/to/target/release/cli-vision"
   export VLM_ENDPOINT="http://127.0.0.1:8080/v1/chat/completions"
   ```

## OpenCode Configuration

Add to your `~/.config/opencode/opencode.json` or project's `opencode.json`:

```json
{
  "mcp": {
    "cli-vision": {
      "type": "local",
      "command": ["python3", "/path/to/mcp_server/tui_qa_server.py"],
      "enabled": true,
      "environment": {
        "CLI_VISION_PATH": "/path/to/target/release/cli-vision",
        "VLM_ENDPOINT": "http://127.0.0.1:8080/v1/chat/completions"
      }
    }
  }
}
```

## Available Tools

### `tui_test`

Run a TUI/CLI application with keyboard inputs and capture state changes.

**Parameters:**
- `binary` (required): Path to the TUI/CLI binary to test
- `inputs` (required): Comma-separated list of inputs (e.g., "down,down,enter,escape")
- `args`: Comma-separated arguments for the binary
- `delay_ms`: Milliseconds between inputs (default: 150)
- `analyze`: Whether to analyze with vision model (default: true)
- `output_dir`: Directory for screenshots

**Supported Inputs:**
- Arrow keys: `up`, `down`, `left`, `right`
- Navigation: `home`, `end`, `pageup`, `pagedown`, `insert`, `delete`
- Common: `enter`, `space`, `tab`, `backspace`, `escape`
- Function keys: `f1` through `f12`
- Ctrl combinations: `ctrl+c`, `ctrl+x`, `ctrl+z`, etc.
- Alt combinations: `alt+f`, `alt+x`, etc.
- Any single character

**Returns:**
```json
{
  "success": true,
  "states": [
    {
      "step": 0,
      "input": null,
      "screenshot_path": "/tmp/tui_qa/state_0_initial.png",
      "description": "Initial state: File browser showing..."
    },
    {
      "step": 1,
      "input": "down",
      "screenshot_path": "/tmp/tui_qa/state_1_down.png",
      "description": "Cursor moved down, now highlighting..."
    }
  ]
}
```

### `tui_capture`

Capture a single screenshot of an app's initial state (no inputs).

**Parameters:**
- `binary` (required): Path to the binary
- `args`: Arguments for the binary
- `output_path`: Where to save the screenshot

### `list_supported_keys`

Returns a dictionary of all supported keyboard inputs.

## Example Usage in OpenCode

Once configured, agents can use the tool like this:

```
Use the tui_test tool to test htop:
- binary: /usr/bin/htop
- inputs: down,down,F9,escape
- analyze: true

This will show me what happens when navigating htop's process list.
```

## Architecture

```
OpenCode Agent
     │
     ▼
MCP Protocol (stdio)
     │
     ▼
tui_qa_server.py
     │
     ▼
cli-vision binary
     │
     ├──► PTY (spawns TUI app)
     ├──► VT100 Terminal Emulator
     ├──► PNG Renderer
     └──► VLM API (for descriptions)
```

## Requirements

- Python 3.8+
- Rust toolchain (for building cli-vision)
- A vision-language model server (e.g., llama.cpp with Qwen-VL) for `--analyze` mode

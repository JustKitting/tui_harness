#!/usr/bin/env python3
"""
CLI Vision - MCP Server

An MCP server that provides terminal UI testing capabilities to AI agents.
Allows agents to run CLI/TUI applications, send keyboard inputs, capture
screenshots at each state, and get AI-powered descriptions of what changed.

Tools:
  - tui_test: Run a TUI app with inputs, capture & analyze state changes
  - tui_capture: Capture a single screenshot of an app's initial state
  - list_supported_keys: List all supported keyboard inputs
"""

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Optional

# Check for mcp package
try:
    from mcp.server.fastmcp import FastMCP
except ImportError:
    print("Error: mcp package not installed. Install with: pip install mcp", file=sys.stderr)
    sys.exit(1)

# Path to the cli-vision binary (adjust as needed)
CLI_VISION_PATH = os.environ.get(
    "CLI_VISION_PATH",
    str(Path(__file__).parent.parent / "target" / "release" / "cli-vision")
)

# Default VLM endpoint
DEFAULT_VLM_ENDPOINT = os.environ.get(
    "VLM_ENDPOINT",
    "http://127.0.0.1:8080/v1/chat/completions"
)

# Create the MCP server
mcp = FastMCP("cli-vision")


@mcp.tool()
def tui_test(
    binary: str,
    inputs: str,
    args: Optional[str] = None,
    delay_ms: int = 150,
    analyze: bool = True,
    output_dir: Optional[str] = None,
    keep: bool = False,
    size: str = "standard",
    prompt: Optional[str] = None,
    step_prompts: Optional[str] = None,
) -> dict:
    """
    Run a TUI/CLI application with keyboard inputs and capture state changes.

    This tool spawns a terminal application in a PTY, sends a sequence of
    keyboard inputs, and captures a screenshot after each input. Optionally,
    each screenshot is analyzed by a vision model to describe what changed.

    Args:
        binary: Path to the TUI/CLI binary to test (e.g., "/usr/bin/htop", "./my-app")
        inputs: Comma-separated list of inputs to send. Supported inputs:
                - Arrow keys: up, down, left, right
                - Navigation: home, end, pageup, pagedown, insert, delete
                - Common: enter, space, tab, backspace, escape
                - Function keys: f1, f2, ..., f12
                - Ctrl combinations: ctrl+c, ctrl+x, ctrl+z, etc.
                - Alt combinations: alt+f, alt+x, etc.
                - Any single character: a, b, 1, 2, etc.
                Example: "down,down,enter,escape"
        args: Comma-separated arguments to pass to the binary (e.g., "--headless,--config,foo.yaml")
        delay_ms: Milliseconds to wait between inputs (default: 150). Increase for slower apps.
        analyze: Whether to analyze screenshots with vision model (default: True)
        output_dir: Directory to save screenshots (default: auto-generated in /tmp/cli-vision/)
        keep: Keep screenshots after completion (default: False, auto-cleanup)
        size: Terminal size - compact (80x24), standard (120x40), large (160x50), xl (200x60), or WxH
        prompt: Custom analysis prompt for all steps. Use {step} and {input} as placeholders.
                Example: "Is the {input} button now highlighted? Describe what you see."
        step_prompts: JSON object mapping step numbers to custom prompts for specific steps.
                Example: '{"0": "Is there a blue button visible?", "2": "Did a dialog open?"}'
                Step-specific prompts override the general prompt for that step.

    Returns:
        A dictionary with:
        - success: Boolean indicating if the test completed
        - error: Error message if failed, null otherwise
        - states: List of state captures, each containing:
            - step: Step number (0 = initial state before any input)
            - input: The input that led to this state (null for step 0)
            - screenshot_path: Path to the PNG screenshot
            - description: AI-generated description of the UI state/changes

    Example:
        tui_test(
            binary="/usr/bin/htop",
            inputs="down,down,enter,q",
            delay_ms=200,
            analyze=True
        )

        Returns descriptions like:
        - Step 0: "htop showing process list, CPU usage at 15%, first process 'systemd' highlighted"
        - Step 1: "Cursor moved down, now highlighting 'kthreadd' process"
        - Step 2: "Cursor moved down, now highlighting 'rcu_gp' process"
        - Step 3: "Process details panel opened for 'rcu_gp'"
        - Step 4: "htop exited, terminal returned to shell"
    """
    # Build command - let cli-vision handle session management
    cmd = [
        CLI_VISION_PATH,
        "run",
        "--binary", binary,
        "--inputs", inputs,
        "--delay", str(delay_ms),
        "--size", size,
        "--json",
    ]

    # If output_dir specified, use it and keep by default
    if output_dir:
        cmd.extend(["--output", output_dir])

    # Keep flag for debugging
    if keep:
        cmd.append("--keep")

    if args:
        cmd.extend(["--args", args])

    if analyze:
        cmd.append("--analyze")
        cmd.extend(["--vlm-endpoint", DEFAULT_VLM_ENDPOINT])

    if prompt:
        cmd.extend(["--prompt", prompt])

    if step_prompts:
        cmd.extend(["--step-prompts", step_prompts])

    # Run the tool - no timeout since cli-vision handles its own
    # activity-based timeouts for VLM communication
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            # No timeout - cli-vision uses activity-based timeouts internally
            # and will fail gracefully if VLM is unresponsive
        )

        if result.returncode != 0:
            return {
                "success": False,
                "error": f"Tool failed: {result.stderr}",
                "states": [],
            }

        # Parse JSON output
        try:
            output = json.loads(result.stdout)
            return output
        except json.JSONDecodeError as e:
            return {
                "success": False,
                "error": f"Failed to parse output: {e}\nStdout: {result.stdout}",
                "states": [],
            }

    except FileNotFoundError:
        return {
            "success": False,
            "error": f"cli-vision not found at {CLI_VISION_PATH}. "
                     f"Set CLI_VISION_PATH environment variable.",
            "states": [],
        }
    except Exception as e:
        return {
            "success": False,
            "error": f"Unexpected error: {e}",
            "states": [],
        }


@mcp.tool()
def tui_capture(
    binary: str,
    args: Optional[str] = None,
    output_path: Optional[str] = None,
    keep: bool = False,
    size: str = "standard",
) -> dict:
    """
    Capture a single screenshot of a TUI/CLI application's initial state.

    This is a simpler version of tui_test that just captures the initial
    state without sending any inputs. Useful for checking what an app
    looks like when first launched.

    Args:
        binary: Path to the TUI/CLI binary to capture
        args: Comma-separated arguments to pass to the binary
        output_path: Path to save the screenshot (default: auto-generated in /tmp/cli-vision/)
        keep: Keep screenshots after completion (default: False, auto-cleanup)
        size: Terminal size - compact (80x24), standard (120x40), large (160x50), xl (200x60), or WxH

    Returns:
        A dictionary with:
        - success: Boolean indicating if capture succeeded
        - error: Error message if failed
        - screenshot_path: Path to the saved PNG
        - width: Image width in pixels
        - height: Image height in pixels
    """
    cmd = [
        CLI_VISION_PATH,
        "cli",
        "--binary", binary,
        "--size", size,
    ]

    # If output_path specified, use its directory
    if output_path:
        output_dir = str(Path(output_path).parent)
        cmd.extend(["--output", output_dir])

    # Keep flag for debugging
    if keep:
        cmd.append("--keep")

    if args:
        # For cli command, args go after --
        cmd.append("--")
        cmd.extend(args.split(","))

    try:
        # Capture is quick - 60s is reasonable but we keep it for safety
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=60,
        )

        if result.returncode != 0:
            return {
                "success": False,
                "error": f"Capture failed: {result.stderr}",
                "screenshot_path": None,
                "width": 0,
                "height": 0,
            }

        # Parse output for dimensions and path
        width, height = 0, 0
        screenshot_path = None
        for line in result.stdout.split("\n"):
            if "Size:" in line:
                # Extract dimensions like "1920x1280 (terminal: 120x40)"
                size_part = line.split("Size:")[1].strip().split()[0]
                parts = size_part.split("x")
                if len(parts) == 2:
                    try:
                        width = int(parts[0])
                        height = int(parts[1])
                    except ValueError:
                        pass
            if "Captured CLI screenshot:" in line:
                screenshot_path = line.split("Captured CLI screenshot:")[1].strip()

        return {
            "success": True,
            "error": None,
            "screenshot_path": screenshot_path,
            "width": width,
            "height": height,
        }

    except subprocess.TimeoutExpired:
        return {
            "success": False,
            "error": "Capture timed out (60s) - app may be waiting for input",
            "screenshot_path": None,
            "width": 0,
            "height": 0,
        }
    except Exception as e:
        return {
            "success": False,
            "error": f"Unexpected error: {e}",
            "screenshot_path": None,
            "width": 0,
            "height": 0,
        }


@mcp.tool()
def list_supported_keys() -> dict:
    """
    List all supported keyboard inputs for tui_test.

    Returns a dictionary categorizing all supported key inputs that can
    be used in the 'inputs' parameter of tui_test.

    Returns:
        Dictionary with categories of supported keys
    """
    return {
        "arrow_keys": ["up", "down", "left", "right"],
        "navigation": ["home", "end", "pageup", "pagedown", "insert", "delete"],
        "common": ["enter", "space", "tab", "backspace", "escape"],
        "function_keys": ["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"],
        "ctrl_combinations": [
            "ctrl+a", "ctrl+b", "ctrl+c", "ctrl+d", "ctrl+e", "ctrl+f", "ctrl+g",
            "ctrl+h", "ctrl+i", "ctrl+j", "ctrl+k", "ctrl+l", "ctrl+m", "ctrl+n",
            "ctrl+o", "ctrl+p", "ctrl+q", "ctrl+r", "ctrl+s", "ctrl+t", "ctrl+u",
            "ctrl+v", "ctrl+w", "ctrl+x", "ctrl+y", "ctrl+z"
        ],
        "alt_combinations": ["alt+<any key>"],
        "single_characters": ["a-z", "A-Z", "0-9", "any printable character"],
        "examples": [
            "down,down,enter",
            "ctrl+c",
            "f1,escape",
            "hello,enter",
            "tab,tab,enter"
        ]
    }


if __name__ == "__main__":
    # Run the MCP server with stdio transport
    mcp.run(transport="stdio")

use clap::{Parser, Subcommand};
use std::error::Error;
use std::path::PathBuf;

use cli_vision::runner::{RunResult, StateCapture};
use cli_vision::session::Session;
use cli_vision::snapshot::{
    run_with_inputs_sized, CaptureBackend, MockFramebuffer, PtyBackend, PtyBackendConfig, TerminalSize,
};
use cli_vision::vlm::{VlmConfig, analyze_image, build_analysis_prompt, check_health};

/// CLI Vision - Terminal UI testing with vision model analysis
#[derive(Parser, Debug)]
#[command(
    name = "cli-vision",
    about = "Cross-platform terminal UI testing with PTY capture and vision model analysis",
    after_help = "ENVIRONMENT VARIABLES:\n\
        CLI_VISION_VLM_ENDPOINT    VLM API endpoint URL\n\
        CLI_VISION_VLM_MODEL       VLM model name\n\
        CLI_VISION_SESSION_DIR     Base directory for sessions\n\
        CLI_VISION_DEFAULT_DELAY   Default delay between inputs (ms)\n\
        CLI_VISION_DEFAULT_SIZE    Default terminal size"
)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Capture a CLI application screenshot using PTY emulation
    Cli {
        /// Path to the binary to capture
        #[arg(short, long)]
        binary: PathBuf,

        /// Output directory for screenshots (default: auto-generated in session dir)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Keep screenshots after completion (default: cleanup unless --output is specified)
        #[arg(long, short = 'k')]
        keep: bool,

        /// Terminal size: compact (80x24), standard (120x40), large (160x50), xl (200x60), or WxH
        #[arg(long, short = 's', env = "CLI_VISION_DEFAULT_SIZE", default_value = "standard")]
        size: String,

        /// Arguments to pass to the binary
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Run a CLI application with inputs, capturing state after each
    Run {
        /// Path to the binary to execute
        #[arg(short, long)]
        binary: PathBuf,

        /// Arguments to pass to the binary (comma-separated, e.g., "--headless,--config,foo.yaml")
        #[arg(short, long, value_delimiter = ',', allow_hyphen_values = true)]
        args: Vec<String>,

        /// Comma-separated list of inputs (e.g., "down,down,enter,escape")
        #[arg(short, long)]
        inputs: String,

        /// Delay in milliseconds between inputs
        #[arg(short, long, env = "CLI_VISION_DEFAULT_DELAY", default_value = "100")]
        delay: u64,

        /// Output directory for screenshots (default: auto-generated in session dir)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Keep screenshots after completion (default: cleanup unless --output is specified)
        #[arg(long, short = 'k')]
        keep: bool,

        /// Analyze screenshots with VLM and include descriptions
        #[arg(long)]
        analyze: bool,

        /// VLM endpoint URL
        #[arg(long, env = "CLI_VISION_VLM_ENDPOINT", default_value = "http://127.0.0.1:8080/v1/chat/completions")]
        vlm_endpoint: String,

        /// VLM model name
        #[arg(long, env = "CLI_VISION_VLM_MODEL", default_value = "qwen3")]
        vlm_model: String,

        /// Custom analysis prompt (use {input} and {step} as placeholders)
        #[arg(long)]
        prompt: Option<String>,

        /// Per-step prompts as JSON: {"1": "check if button is blue", "3": "verify dialog opened"}
        #[arg(long)]
        step_prompts: Option<String>,

        /// Output results as JSON
        #[arg(long)]
        json: bool,

        /// Terminal size: compact (80x24), standard (120x40), large (160x50), xl (200x60), or WxH (e.g., 100x30)
        #[arg(long, short = 's', env = "CLI_VISION_DEFAULT_SIZE", default_value = "standard")]
        size: String,

        /// Run with all preset sizes and compare results (useful for finding resize bugs)
        #[arg(long)]
        multi_size: bool,
    },

    /// Create a mock framebuffer screenshot for testing
    Mock {
        /// Width in pixels
        #[arg(short = 'W', long, default_value = "800")]
        width: u32,

        /// Height in pixels
        #[arg(short = 'H', long, default_value = "600")]
        height: u32,

        /// Output file path
        #[arg(short, long, default_value = "./mock_screenshot.png")]
        output: PathBuf,

        /// Fill color as hex (e.g., "ff0000" for red)
        #[arg(short, long, default_value = "000000")]
        color: String,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    match args.command {
        Some(Commands::Cli {
            binary,
            output,
            keep,
            size,
            args: binary_args,
        }) => {
            // Parse terminal size
            let term_size = TerminalSize::from_str(&size)
                .ok_or_else(|| format!("Invalid terminal size '{}'. Use: compact, standard, large, xl, or WxH", size))?;
            let (cols, rows) = term_size.dimensions();

            // Create session - if output specified, use that dir and keep by default
            let session = if let Some(ref dir) = output {
                Session::in_dir(dir).keep(keep || output.is_some())
            } else {
                let binary_name = binary.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "capture".to_string());
                Session::with_name(&binary_name).keep(keep)
            };
            session.init()?;

            let config = PtyBackendConfig::new(&binary)
                .args(binary_args)
                .size(cols, rows);
            let mut backend = PtyBackend::new(config);

            let result = backend.capture()?;
            let output_path = session.capture_path("capture");
            std::fs::write(&output_path, &result.image_data)?;

            println!("Captured CLI screenshot: {}", output_path.display());
            println!("  Size: {}x{} (terminal: {}x{})", result.width, result.height, cols, rows);

            // Keep session alive if needed (prevent Drop cleanup)
            if keep || output.is_some() {
                std::mem::forget(session);
            }
        }

        Some(Commands::Run {
            binary,
            args: binary_args,
            inputs,
            delay,
            output,
            keep,
            analyze,
            vlm_endpoint,
            vlm_model,
            prompt,
            step_prompts,
            json,
            size,
            multi_size,
        }) => {
            // Create session - if output specified, use that dir and keep by default
            let binary_name = binary.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "run".to_string());

            let session = if let Some(ref dir) = output {
                Session::in_dir(dir).keep(keep || output.is_some())
            } else {
                Session::with_name(&format!("{}_run", binary_name)).keep(keep)
            };
            session.init()?;

            // Parse inputs
            let input_list: Vec<String> = inputs
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // Parse step-specific prompts if provided
            let step_prompt_map: std::collections::HashMap<usize, String> = step_prompts
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();

            // Determine terminal sizes to test
            let sizes_to_test: Vec<TerminalSize> = if multi_size {
                TerminalSize::all_presets()
            } else {
                let term_size = TerminalSize::from_str(&size)
                    .ok_or_else(|| format!("Invalid terminal size '{}'. Use: compact, standard, large, xl, or WxH (e.g., 100x30)", size))?;
                vec![term_size]
            };

            // Process each size
            for term_size in &sizes_to_test {
                let (cols, rows) = term_size.dimensions();
                let size_output = if multi_size {
                    session.size_subdir(cols, rows)
                } else {
                    session.dir.clone()
                };
                std::fs::create_dir_all(&size_output)?;

            // Run with inputs and capture each state
            let captures = run_with_inputs_sized(
                binary.to_str().unwrap_or(""),
                &binary_args,
                &input_list,
                delay,
                *term_size,
            )?;

            // Check VLM health before starting analysis (if analyze is requested)
            let vlm_healthy = if analyze {
                match check_health(&vlm_endpoint, 5) {
                    Ok(true) => {
                        if !json {
                            eprintln!("VLM endpoint responding, starting analysis...");
                        }
                        true
                    }
                    Ok(false) | Err(_) => {
                        eprintln!("Warning: VLM endpoint not responding at {}", vlm_endpoint);
                        eprintln!("Skipping analysis. Screenshots will still be saved.");
                        false
                    }
                }
            } else {
                false
            };

            // Build result
            let mut states: Vec<StateCapture> = Vec::new();

            for capture in &captures {
                // Save screenshot
                let filename = if capture.step == 0 {
                    "state_0_initial.png".to_string()
                } else {
                    let input_name = capture
                        .input
                        .as_ref()
                        .map(|s| s.replace('+', "_").replace(' ', "_"))
                        .unwrap_or_default();
                    format!("state_{}_{}.png", capture.step, input_name)
                };
                let screenshot_path = size_output.join(&filename);
                std::fs::write(&screenshot_path, &capture.image_data)?;

                // Get VLM description if requested and VLM is healthy
                let description = if vlm_healthy {
                    // Check for step-specific prompt first, then custom prompt, then default
                    let custom_prompt = step_prompt_map
                        .get(&capture.step)
                        .map(|s| s.as_str())
                        .or(prompt.as_deref());

                    let analysis_prompt = build_analysis_prompt(
                        capture.step,
                        capture.input.as_deref(),
                        custom_prompt,
                    );

                    let vlm_config = VlmConfig::new(&vlm_endpoint).model(&vlm_model);

                    match analyze_image(&vlm_config, &capture.image_data, &analysis_prompt) {
                        Ok(desc) => Some(desc),
                        Err(e) => {
                            eprintln!("Warning: VLM analysis failed for step {}: {}", capture.step, e);
                            None
                        }
                    }
                } else {
                    None
                };

                states.push(StateCapture {
                    step: capture.step,
                    input: capture.input.clone(),
                    screenshot_path: screenshot_path.clone(),
                    description,
                });
            }

            let result = RunResult {
                success: true,
                error: None,
                states,
            };

            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                if multi_size {
                    println!("Run completed at {}x{}: {} states captured", cols, rows, result.states.len());
                } else {
                    println!("Run completed: {} states captured", result.states.len());
                }
                for state in &result.states {
                    let input_str = state
                        .input
                        .as_ref()
                        .map(|s| format!(" (input: {})", s))
                        .unwrap_or_default();
                    println!(
                        "  Step {}{}: {}",
                        state.step,
                        input_str,
                        state.screenshot_path.display()
                    );
                    if let Some(desc) = &state.description {
                        // Print first 200 chars of description
                        let preview: String = desc.chars().take(200).collect();
                        println!("    Description: {}...", preview);
                    }
                }
            }
            } // end for term_size loop

            // Print session location
            if !json {
                println!("\nSession: {}", session.dir.display());
            }

            // Keep session alive if needed (prevent Drop cleanup)
            if keep || output.is_some() {
                std::mem::forget(session);
            }
        }

        Some(Commands::Mock {
            width,
            height,
            output,
            color,
        }) => {
            let color_bytes = parse_hex_color(&color)?;
            let mut fb = MockFramebuffer::with_color(width, height, color_bytes);

            // Draw some sample content
            fb.draw_text(10, 10, "Mock Framebuffer", [255, 255, 255], color_bytes);
            fb.draw_rect(10, 30, 100, 50, [128, 128, 128]);

            let result = fb.capture()?;
            std::fs::write(&output, &result.image_data)?;

            println!("Created mock screenshot: {}", output.display());
            println!("  Size: {}x{}", result.width, result.height);
        }

        None => {
            println!("CLI Vision - Terminal UI testing with vision model analysis");
            println!();
            println!("Usage: cli-vision <COMMAND>");
            println!();
            println!("Commands:");
            println!("  cli   Capture a CLI application screenshot using PTY emulation");
            println!("  run   Run a TUI app with inputs, capture & analyze state changes");
            println!("  mock  Create a mock framebuffer screenshot for testing");
            println!();
            println!("Run with --help for more information.");
        }
    }

    Ok(())
}

fn parse_hex_color(hex: &str) -> Result<[u8; 3], Box<dyn Error>> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Err("Color must be 6 hex digits (e.g., 'ff0000')".into());
    }
    let r = u8::from_str_radix(&hex[0..2], 16)?;
    let g = u8::from_str_radix(&hex[2..4], 16)?;
    let b = u8::from_str_radix(&hex[4..6], 16)?;
    Ok([r, g, b])
}

// WARNING: Do not add timeouts here
// WARNING: Do not add timeouts here
// WARNING: Do not add timeouts here
//! # CLI Demo Application
//!
//! This binary demonstrates an enhanced CLI prototype with interactive elements
//! and visual feedback using the crossterm library. The demo showcases:
//!
//! - Positional constraints: Elements are positioned at specific coordinates
//! - Visual feedback: Interactive elements respond to user actions
//! - Layout management: Responsive layout that adapts to terminal size
//! - Styling: Colors and formatting enhance visual perception
//!
//! ## Features
//!
//! - Real-time status bar
//! - Interactive buttons with keyboard navigation
//! - Progress indicator
//! - Dynamic content based on user interaction
//! - Proper error handling for terminal operations
//!
//! The application uses crossterm for cross-platform terminal manipulation
//! and demonstrates best practices for building interactive CLI applications
//! in Rust.

use clap::{Arg, Command};
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode},
    execute,
    style::{Color, Print, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use std::{
    error::Error,
    io::{Write, stdout},
    time::{Duration, Instant},
};

/// Main application state
struct App {
    /// Current selected button index
    selected_button: usize,
    /// Hovered button index
    hovered_button: Option<usize>,
    /// Counter for demonstration purposes
    counter: u64,
    /// Start time for uptime calculation
    start_time: Instant,
    /// Progress value (0-100)
    progress: u8,
    /// Whether the box in lower right is visible
    box_visible: bool,
    /// Checkbox state
    checkbox_checked: bool,
    /// Slider value (0-10)
    slider_value: u8,
}

impl App {
    /// Create a new application instance
    fn new() -> Self {
        Self {
            selected_button: 0,
            hovered_button: None,
            counter: 0,
            start_time: Instant::now(),
            progress: 0,
            box_visible: false,
            checkbox_checked: false,
            slider_value: 5,
        }
    }

    /// Update the application state
    fn update(&mut self) {
        self.counter += 1;
        // Simulate progress
        self.progress = (self.progress + 1) % 101;
    }
}

/// Button style
#[derive(Clone, Copy)]
enum ButtonStyle {
    Normal,
    Hovered,
    Selected,
}

/// Button widget
struct Button {
    label: &'static str,
    x: u16,
    y: u16,
    width: u16,
}

impl Button {
    /// Create a new button
    fn new(label: &'static str, x: u16, y: u16) -> Self {
        Self {
            label,
            x,
            y,
            width: (label.len() + 4) as u16, // padding
        }
    }

    /// Render the button
    fn render(&self, style: ButtonStyle, w: &mut std::io::Stdout) -> Result<(), Box<dyn Error>> {
        let (fg_color, bg_color, border_color) = match style {
            ButtonStyle::Normal => (Color::White, None, Color::White),
            ButtonStyle::Hovered => (Color::Red, None, Color::Yellow),
            ButtonStyle::Selected => (Color::White, Some(Color::Blue), Color::White),
        };

        if let Some(bg) = bg_color {
            execute!(w, SetBackgroundColor(bg))?;
        }
        execute!(w, SetForegroundColor(border_color))?;

        // Draw button border
        execute!(
            w,
            crossterm::cursor::MoveTo(self.x, self.y),
            Print("┌"),
            Print("─".repeat((self.width - 2) as usize)),
            Print("┐"),
        )?;

        // Draw middle row with label
        execute!(
            w,
            crossterm::cursor::MoveTo(self.x, self.y + 1),
            Print("│"),
            Print("  "),
            SetForegroundColor(fg_color),
            Print(self.label),
            SetForegroundColor(border_color),
            Print("  "),
            Print("│"),
        )?;

        // Draw bottom border
        execute!(
            w,
            crossterm::cursor::MoveTo(self.x, self.y + 2),
            Print("└"),
            Print("─".repeat((self.width - 2) as usize)),
            Print("┘"),
        )?;

        // Reset colors
        execute!(
            w,
            SetBackgroundColor(Color::Reset),
            SetForegroundColor(Color::Reset)
        )?;

        Ok(())
    }
}

/// Status bar component
struct StatusBar {
    message: String,
    width: u16,
}

impl StatusBar {
    /// Create a new status bar
    fn new(width: u16) -> Self {
        Self {
            message: "Ready".to_string(),
            width,
        }
    }

    /// Update status message
    fn update(&mut self, message: String) {
        self.message = message;
    }

    /// Render the status bar
    fn render(&self, w: &mut std::io::Stdout) -> Result<(), Box<dyn Error>> {
        execute!(
            w,
            SetBackgroundColor(Color::DarkGrey),
            SetForegroundColor(Color::White),
            crossterm::cursor::MoveTo(0, 0),
            Clear(ClearType::FromCursorDown),
        )?;

        // Draw status bar background
        let message_len = self.message.len();
        let width_usize = self.width as usize;
        let padding_len = width_usize.saturating_sub(message_len);
        let bar_content = format!("{}{}", self.message, " ".repeat(padding_len));

        execute!(w, crossterm::cursor::MoveTo(0, 0), Print(&bar_content),)?;

        Ok(())
    }
}

/// Checkbox component
struct Checkbox {
    x: u16,
    y: u16,
    label: &'static str,
}

impl Checkbox {
    fn new(x: u16, y: u16, label: &'static str) -> Self {
        Self { x, y, label }
    }

    fn render(&self, checked: bool, w: &mut std::io::Stdout) -> Result<(), Box<dyn Error>> {
        execute!(
            w,
            crossterm::cursor::MoveTo(self.x, self.y),
            SetForegroundColor(Color::White),
            Print("["),
            Print(if checked { "x" } else { " " }),
            Print("] "),
            Print(self.label),
        )?;
        Ok(())
    }
}

/// Slider component
struct Slider {
    x: u16,
    y: u16,
    width: u16,
    max_value: u8,
}

impl Slider {
    fn new(x: u16, y: u16, width: u16) -> Self {
        Self {
            x,
            y,
            width,
            max_value: 10,
        }
    }

    fn render(&self, value: u8, w: &mut std::io::Stdout) -> Result<(), Box<dyn Error>> {
        let filled =
            ((value as u16 * (self.width - 2)) / self.max_value as u16).min(self.width - 2);
        execute!(
            w,
            crossterm::cursor::MoveTo(self.x, self.y),
            SetForegroundColor(Color::White),
            Print("["),
        )?;
        for i in 0..(self.width - 2) {
            let ch = if i < filled { "=" } else { "-" };
            execute!(w, Print(ch))?;
        }
        execute!(
            w,
            Print("] "),
            Print(format!("{}/{}", value, self.max_value)),
        )?;
        Ok(())
    }
}

/// Box component for lower right
struct InfoBox {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl InfoBox {
    fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn render(&self, w: &mut std::io::Stdout) -> Result<(), Box<dyn Error>> {
        // Top border
        execute!(
            w,
            crossterm::cursor::MoveTo(self.x, self.y),
            SetForegroundColor(Color::Cyan),
            Print("┌"),
            Print("─".repeat((self.width - 2) as usize)),
            Print("┐"),
        )?;

        // Sides
        for i in 1..(self.height - 1) {
            execute!(
                w,
                crossterm::cursor::MoveTo(self.x, self.y + i),
                Print("│"),
                crossterm::cursor::MoveTo(self.x + self.width - 1, self.y + i),
                Print("│"),
            )?;
        }

        // Bottom border
        execute!(
            w,
            crossterm::cursor::MoveTo(self.x, self.y + self.height - 1),
            Print("└"),
            Print("─".repeat((self.width - 2) as usize)),
            Print("┘"),
        )?;

        // Content
        execute!(
            w,
            crossterm::cursor::MoveTo(self.x + 2, self.y + 1),
            Print("Info Box"),
            crossterm::cursor::MoveTo(self.x + 2, self.y + 2),
            Print("Visible"),
        )?;

        Ok(())
    }
}

/// Progress bar component
struct ProgressBar {
    x: u16,
    y: u16,
    width: u16,
    value: u8, // 0-100
}

impl ProgressBar {
    /// Create a new progress bar
    fn new(x: u16, y: u16, width: u16) -> Self {
        Self {
            x,
            y,
            width,
            value: 0,
        }
    }

    /// Set progress value
    fn set_value(&mut self, value: u8) {
        self.value = value.min(100);
    }

    /// Render the progress bar
    fn render(&self, w: &mut std::io::Stdout) -> Result<(), Box<dyn Error>> {
        let filled_width = ((self.width - 2) * self.value as u16) / 100;

        // Draw border
        execute!(
            w,
            crossterm::cursor::MoveTo(self.x, self.y),
            SetForegroundColor(Color::White),
            Print("["),
        )?;

        // Draw filled portion
        execute!(
            w,
            SetBackgroundColor(Color::Blue),
            SetForegroundColor(Color::Blue),
            Print("█".repeat(filled_width as usize)),
        )?;

        // Draw empty portion
        execute!(
            w,
            SetBackgroundColor(Color::DarkGrey),
            SetForegroundColor(Color::DarkGrey),
            Print("█".repeat((self.width - 2 - filled_width) as usize)),
        )?;

        // Close border
        execute!(
            w,
            SetBackgroundColor(Color::Reset),
            SetForegroundColor(Color::White),
            Print("]"),
            SetForegroundColor(Color::Reset),
        )?;

        // Show percentage
        execute!(
            w,
            crossterm::cursor::MoveTo(self.x + self.width + 1, self.y),
            Print(format!("{}%", self.value)),
        )?;

        Ok(())
    }
}

/// Main function
fn main() -> Result<(), Box<dyn Error>> {
    // Parse command line arguments
    let matches = Command::new("cli_demo")
        .version("1.0")
        .author("Screenshot Tool")
        .about("CLI demo application for visual QA testing")
        .arg(
            Arg::new("headless")
                .long("headless")
                .help("Run in headless mode for testing (runs for 2 seconds then exits)")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("state")
                .long("state")
                .help("Set initial application state for testing")
                .value_name("STATE")
                .default_value("initial"),
        )
        .get_matches();

    let headless = matches.get_flag("headless");
    let state = matches.get_one::<String>("state").unwrap();

    // Initialize terminal
    let mut stdout = stdout();
    execute!(stdout, Hide)?;
    terminal::enable_raw_mode()?;

    // Ensure cleanup on exit
    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut app = App::new();

        // Set initial state based on --state argument
        match state.as_str() {
            "button_hovered" => {
                app.hovered_button = Some(0);
            }
            "button_selected" => {
                app.selected_button = 0;
            }
            "box_visible" => {
                app.box_visible = true;
            }
            "multiple_elements" => {
                app.box_visible = true;
                app.selected_button = 0;
                app.checkbox_checked = true;
                app.slider_value = 8;
            }
            _ => {} // initial or unknown
        }

        // Get terminal size
        let width = terminal::size()?.0;

        // Create UI components
        let mut status_bar = StatusBar::new(width);
        let mut progress_bar = ProgressBar::new(2, 8, width - 4);

        // Create buttons
        let buttons = [
            Button::new("Increment", 2, 4),
            Button::new("Reset", 14, 4),
            Button::new("Exit", 24, 4),
        ];

        // Create additional components
        let checkbox = Checkbox::new(2, 12, "Enable feature");
        let slider = Slider::new(2, 14, 20);
        let info_box = InfoBox::new(width - 22, 18, 20, 4);

        // Main loop
        loop {
            // In headless mode, exit after 2 seconds
            if headless && app.start_time.elapsed() > Duration::from_secs(2) {
                break;
            }

            // Clear screen
            execute!(stdout, Clear(ClearType::All))?;

            // Get current terminal size
            let (term_width, term_height) = terminal::size()?;

            // Update progress bar
            progress_bar.set_value(app.progress);

            // Render status bar
            let uptime = app.start_time.elapsed();
            status_bar.update(format!(
                "Uptime: {:?} | Counter: {} | Terminal: {}x{}",
                uptime, app.counter, term_width, term_height
            ));
            status_bar.render(&mut stdout)?;

            // Render progress bar
            progress_bar.render(&mut stdout)?;

            // Render buttons
            for (i, button) in buttons.iter().enumerate() {
                let style = if app.hovered_button == Some(i) {
                    ButtonStyle::Hovered
                } else if app.selected_button == i {
                    ButtonStyle::Selected
                } else {
                    ButtonStyle::Normal
                };
                button.render(style, &mut stdout)?;
            }

            // Render additional components
            checkbox.render(app.checkbox_checked, &mut stdout)?;
            slider.render(app.slider_value, &mut stdout)?;
            if app.box_visible {
                info_box.render(&mut stdout)?;
            }

            // Render dynamic content
            execute!(
                stdout,
                crossterm::cursor::MoveTo(2, 16),
                SetForegroundColor(Color::Green),
                Print("Dynamic Content Area".to_string()),
            )?;

            execute!(
                stdout,
                crossterm::cursor::MoveTo(2, 17),
                SetForegroundColor(Color::Cyan),
                Print(format!("Selected: {}", buttons[app.selected_button].label)),
            )?;

            // Flush output
            stdout.flush()?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => break,
                        KeyCode::Left => {
                            if app.selected_button > 0 {
                                app.selected_button -= 1;
                                status_bar.update(format!(
                                    "Navigated to {}",
                                    buttons[app.selected_button].label
                                ));
                            }
                        }
                        KeyCode::Right => {
                            if app.selected_button < buttons.len() - 1 {
                                app.selected_button += 1;
                                status_bar.update(format!(
                                    "Navigated to {}",
                                    buttons[app.selected_button].label
                                ));
                            }
                        }
                        KeyCode::Enter => {
                            match app.selected_button {
                                0 => {
                                    // Increment
                                    app.update();
                                    status_bar.update("Counter incremented".to_string());
                                }
                                1 => {
                                    // Reset
                                    app.counter = 0;
                                    status_bar.update("Counter reset".to_string());
                                }
                                2 => {
                                    // Exit
                                    status_bar.update("Exiting application...".to_string());
                                    break;
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                // Timeout reached, update animation
                app.update();
            }
        }

        Ok(())
    })();

    // Cleanup terminal
    execute!(stdout, Show)?;
    terminal::disable_raw_mode()?;

    // Print final message if there was an error
    if let Err(ref e) = result {
        eprintln!("Application error: {}", e);
    }

    result?;
    Ok(())
}

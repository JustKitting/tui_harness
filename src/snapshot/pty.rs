use font8x8::{BASIC_FONTS, BLOCK_FONTS, BOX_FONTS, GREEK_FONTS, HIRAGANA_FONTS, LATIN_FONTS, MISC_FONTS, UnicodeFonts};
use image::{ImageBuffer, Rgb};
use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};
use vte::{Params, Parser as AnsiParser, Perform};

const DEFAULT_TERMINAL_WIDTH: u16 = 120;
const DEFAULT_TERMINAL_HEIGHT: u16 = 40;
const FONT_WIDTH: u32 = 8;
const FONT_HEIGHT: u32 = 16;
const PIXEL_SCALE: u32 = 2;
/// Width of a terminal cell in pixels (font width * pixel scale)
pub const CELL_WIDTH: u32 = FONT_WIDTH * PIXEL_SCALE;
/// Height of a terminal cell in pixels (font height * pixel scale)
pub const CELL_HEIGHT: u32 = FONT_HEIGHT * PIXEL_SCALE;
const QUIET_WINDOW: Duration = Duration::from_millis(180);
/// Maximum time to wait for initial render (for apps that output continuously)
const MAX_INITIAL_RENDER_WAIT: Duration = Duration::from_secs(3);
/// Maximum time to wait for render after each input
const MAX_INPUT_RENDER_WAIT: Duration = Duration::from_secs(2);
const PROCESS_DRAIN_TIMEOUT: Duration = Duration::from_secs(3);

const ANSI_COLORS: [[u8; 3]; 8] = [
    [0, 0, 0],
    [205, 49, 49],
    [13, 188, 121],
    [229, 229, 16],
    [36, 114, 200],
    [188, 63, 188],
    [17, 168, 205],
    [229, 229, 229],
];

const ANSI_BRIGHT_COLORS: [[u8; 3]; 8] = [
    [102, 102, 102],
    [241, 76, 76],
    [35, 209, 139],
    [245, 245, 67],
    [59, 142, 234],
    [214, 112, 214],
    [41, 184, 219],
    [255, 255, 255],
];

fn clamp_u16_to_u8(value: u16) -> u8 {
    value.min(255) as u8
}

/// Brighten a color for bold text
fn brighten_color(color: [u8; 3]) -> [u8; 3] {
    // Increase each component by ~30% or to at least 128
    [
        color[0].saturating_add(64).max(color[0].saturating_mul(4) / 3),
        color[1].saturating_add(64).max(color[1].saturating_mul(4) / 3),
        color[2].saturating_add(64).max(color[2].saturating_mul(4) / 3),
    ]
}

fn xterm_256_to_rgb(idx: u8) -> [u8; 3] {
    match idx {
        0..=7 => ANSI_COLORS[idx as usize],
        8..=15 => ANSI_BRIGHT_COLORS[(idx - 8) as usize],
        16..=231 => {
            let normalized = idx - 16;
            let r = normalized / 36;
            let g = (normalized % 36) / 6;
            let b = normalized % 6;
            let scale = [0, 95, 135, 175, 215, 255];
            [scale[r as usize], scale[g as usize], scale[b as usize]]
        }
        232..=255 => {
            let shade = 8 + (idx - 232) * 10;
            [shade, shade, shade]
        }
    }
}

fn get_char_bitmap(ch: char) -> [u8; 16] {
    font8x8_bitmap(ch)
}

fn font8x8_bitmap(ch: char) -> [u8; 16] {
    fn expand(glyph: [u8; 8]) -> [u8; 16] {
        let mut out = [0u8; 16];
        for (idx, row) in glyph.iter().enumerate() {
            let target = idx * 2;
            out[target] = *row;
            out[target + 1] = *row;
        }
        out
    }

    // font8x8 glyph sets
    if let Some(glyph) = BASIC_FONTS.get(ch) { return expand(glyph); }
    if let Some(glyph) = BOX_FONTS.get(ch) { return expand(glyph); }
    if let Some(glyph) = BLOCK_FONTS.get(ch) { return expand(glyph); }
    if let Some(glyph) = LATIN_FONTS.get(ch) { return expand(glyph); }
    if let Some(glyph) = GREEK_FONTS.get(ch) { return expand(glyph); }
    if let Some(glyph) = HIRAGANA_FONTS.get(ch) { return expand(glyph); }
    if let Some(glyph) = MISC_FONTS.get(ch) { return expand(glyph); }

    // Braille (U+2800-U+28FF) - used by ratatui Canvas for plotting
    if let Some(braille) = render_braille(ch) { return braille; }

    [0; 16]
}

/// Render Braille character (U+2800-U+28FF) to 8x16 bitmap.
/// Braille: 2 cols × 4 rows of dots. Bits 0-2,6 = left col, bits 3-5,7 = right col.
fn render_braille(ch: char) -> Option<[u8; 16]> {
    let code = ch as u32;
    if !(0x2800..=0x28FF).contains(&code) {
        return None;
    }

    let pattern = (code - 0x2800) as u8;
    let mut bitmap = [0u8; 16];
    let left = 0b00001110u8;
    let right = 0b01110000u8;

    // Left column: bits 0,1,2,6 → rows 1-2, 5-6, 9-10, 13-14
    if pattern & 0x01 != 0 { bitmap[1] |= left; bitmap[2] |= left; }
    if pattern & 0x02 != 0 { bitmap[5] |= left; bitmap[6] |= left; }
    if pattern & 0x04 != 0 { bitmap[9] |= left; bitmap[10] |= left; }
    if pattern & 0x40 != 0 { bitmap[13] |= left; bitmap[14] |= left; }

    // Right column: bits 3,4,5,7 → rows 1-2, 5-6, 9-10, 13-14
    if pattern & 0x08 != 0 { bitmap[1] |= right; bitmap[2] |= right; }
    if pattern & 0x10 != 0 { bitmap[5] |= right; bitmap[6] |= right; }
    if pattern & 0x20 != 0 { bitmap[9] |= right; bitmap[10] |= right; }
    if pattern & 0x80 != 0 { bitmap[13] |= right; bitmap[14] |= right; }

    Some(bitmap)
}

struct TerminalPerformer<'a> {
    terminal: &'a mut Vt100Terminal,
}

impl<'a> TerminalPerformer<'a> {
    fn param_or(params: &Params, index: usize, default: u16) -> u16 {
        params
            .iter()
            .nth(index)
            .and_then(|p| p.first())
            .copied()
            .filter(|v| *v != 0)
            .unwrap_or(default)
    }

    fn handle_sgr(&mut self, params: &Params) {
        if params.is_empty() {
            self.terminal.reset_attributes();
            return;
        }

        let values: Vec<u16> = params.iter().flat_map(|chunk| chunk.iter().copied()).collect();
        if values.is_empty() {
            self.terminal.reset_attributes();
            return;
        }

        let mut i = 0;
        while i < values.len() {
            let value = values[i];
            match value {
                0 => self.terminal.reset_attributes(),
                1 => self.terminal.set_bold(true),
                4 => self.terminal.set_underline(true),
                7 => self.terminal.set_inverse(true),
                22 => self.terminal.set_bold(false), // Normal intensity (not bold)
                24 => self.terminal.set_underline(false),
                27 => self.terminal.set_inverse(false),
                30..=37 => {
                    self.terminal
                        .set_fg_color(ANSI_COLORS[(value - 30) as usize]);
                }
                40..=47 => {
                    self.terminal
                        .set_bg_color(ANSI_COLORS[(value - 40) as usize]);
                }
                90..=97 => {
                    self.terminal
                        .set_fg_color(ANSI_BRIGHT_COLORS[(value - 90) as usize]);
                }
                100..=107 => {
                    self.terminal
                        .set_bg_color(ANSI_BRIGHT_COLORS[(value - 100) as usize]);
                }
                38 | 48 => {
                    let is_fg = value == 38;
                    if i + 1 >= values.len() {
                        break;
                    }
                    let mode = values[i + 1];
                    match mode {
                        2 => {
                            if i + 4 >= values.len() {
                                break;
                            }
                            let r = clamp_u16_to_u8(values[i + 2]);
                            let g = clamp_u16_to_u8(values[i + 3]);
                            let b = clamp_u16_to_u8(values[i + 4]);
                            let color = [r, g, b];
                            if is_fg {
                                self.terminal.set_fg_color(color);
                            } else {
                                self.terminal.set_bg_color(color);
                            }
                            i += 5;
                            continue;
                        }
                        5 => {
                            if i + 2 >= values.len() {
                                break;
                            }
                            let idx = values[i + 2] as u8;
                            let color = xterm_256_to_rgb(idx);
                            if is_fg {
                                self.terminal.set_fg_color(color);
                            } else {
                                self.terminal.set_bg_color(color);
                            }
                            i += 3;
                            continue;
                        }
                        _ => {
                            i += 2;
                            continue;
                        }
                    }
                }
                39 => self.terminal.reset_fg(),
                49 => self.terminal.reset_bg(),
                _ => {}
            }
            i += 1;
        }
    }
}

impl<'a> Perform for TerminalPerformer<'a> {
    fn print(&mut self, c: char) {
        self.terminal.write_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.terminal.write_char('\n'),
            b'\r' => self.terminal.write_char('\r'),
            b'\t' => self.terminal.write_char('\t'),
            0x08 => self.terminal.backspace(),
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let private_mode = intermediates.iter().any(|b| *b == b'?');

        match action {
            'H' | 'f' => {
                let row = Self::param_or(params, 0, 1).saturating_sub(1);
                let col = Self::param_or(params, 1, 1).saturating_sub(1);
                self.terminal
                    .move_cursor(u32::from(col), u32::from(row));
            }
            'A' => {
                let value = Self::param_or(params, 0, 1) as i32;
                self.terminal.move_cursor_rel(0, -(value as i32));
            }
            'B' => {
                let value = Self::param_or(params, 0, 1) as i32;
                self.terminal.move_cursor_rel(0, value as i32);
            }
            'C' => {
                let value = Self::param_or(params, 0, 1) as i32;
                self.terminal.move_cursor_rel(value as i32, 0);
            }
            'D' => {
                let value = Self::param_or(params, 0, 1) as i32;
                self.terminal.move_cursor_rel(-(value as i32), 0);
            }
            'J' => {
                let mode = Self::param_or(params, 0, 0);
                match mode {
                    0 => self.terminal.clear_from_cursor(),
                    1 => {} // unsupported
                    2 | 3 => self.terminal.clear(),
                    _ => {}
                }
            }
            'K' => self.terminal.clear_line_from_cursor(),
            'm' => self.handle_sgr(params),
            's' => self.terminal.save_cursor(),
            'u' => self.terminal.restore_cursor(),
            'h' if private_mode => {
                // Handle private mode set
                let mode = Self::param_or(params, 0, 0);
                match mode {
                    47 | 1047 | 1049 => {
                        // Enter alternate screen buffer
                        self.terminal.enter_alternate_screen();
                    }
                    _ => {} // Ignore other private modes (cursor visibility, etc.)
                }
            }
            'l' if private_mode => {
                // Handle private mode reset
                let mode = Self::param_or(params, 0, 0);
                match mode {
                    47 | 1047 | 1049 => {
                        // Leave alternate screen buffer
                        self.terminal.leave_alternate_screen();
                    }
                    _ => {} // Ignore other private modes
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => self.terminal.save_cursor(),
            b'8' => self.terminal.restore_cursor(),
            b'c' => self.terminal.clear(),
            _ => {}
        }
    }
}

/// Text attributes for a single cell
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CellAttributes {
    pub bold: bool,
    pub underline: bool,
    pub inverse: bool,
}

/// Saved state for alternate screen buffer
#[derive(Debug, Clone)]
struct SavedScreen {
    buffer: Vec<Vec<char>>,
    fg_colors: Vec<Vec<[u8; 3]>>,
    bg_colors: Vec<Vec<[u8; 3]>>,
    attributes: Vec<Vec<CellAttributes>>,
    cursor_x: u32,
    cursor_y: u32,
}

/// Represents the state of a VT100 terminal
#[derive(Debug, Clone)]
pub struct Vt100Terminal {
    /// Terminal width in characters
    pub width: u32,
    /// Terminal height in characters
    pub height: u32,
    /// Character buffer (height x width)
    pub buffer: Vec<Vec<char>>,
    /// Foreground color buffer
    pub fg_colors: Vec<Vec<[u8; 3]>>,
    /// Background color buffer
    pub bg_colors: Vec<Vec<[u8; 3]>>,
    /// Cell attributes buffer (bold, underline, inverse)
    pub attributes: Vec<Vec<CellAttributes>>,
    /// Cursor position
    pub cursor_x: u32,
    pub cursor_y: u32,
    /// Current colors
    pub current_fg: [u8; 3],
    pub current_bg: [u8; 3],
    /// Current text attributes
    pub current_attrs: CellAttributes,
    /// Default colors
    default_fg: [u8; 3],
    default_bg: [u8; 3],
    /// Saved cursor position
    saved_cursor: Option<(u32, u32)>,
    /// Alternate screen buffer (for vim, less, htop, etc.)
    alternate_screen: Option<Box<SavedScreen>>,
    /// Whether we're currently in the alternate screen
    in_alternate_screen: bool,
}

impl Vt100Terminal {
    /// Create a new terminal with default settings
    pub fn new(width: u32, height: u32) -> Self {
        let mut buffer = Vec::with_capacity(height as usize);
        let mut fg_colors = Vec::with_capacity(height as usize);
        let mut bg_colors = Vec::with_capacity(height as usize);
        let mut attributes = Vec::with_capacity(height as usize);

        for _ in 0..height {
            buffer.push(vec![' '; width as usize]);
            fg_colors.push(vec![[255, 255, 255]; width as usize]); // White text
            bg_colors.push(vec![[0, 0, 0]; width as usize]); // Black background
            attributes.push(vec![CellAttributes::default(); width as usize]);
        }

        Self {
            width,
            height,
            buffer,
            fg_colors,
            bg_colors,
            attributes,
            cursor_x: 0,
            cursor_y: 0,
            current_fg: [255, 255, 255],
            current_bg: [0, 0, 0],
            current_attrs: CellAttributes::default(),
            default_fg: [255, 255, 255],
            default_bg: [0, 0, 0],
            saved_cursor: None,
            alternate_screen: None,
            in_alternate_screen: false,
        }
    }

    /// Clear the screen
    pub fn clear(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.buffer[y as usize][x as usize] = ' ';
                self.fg_colors[y as usize][x as usize] = self.default_fg;
                self.bg_colors[y as usize][x as usize] = self.default_bg;
                self.attributes[y as usize][x as usize] = CellAttributes::default();
            }
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.saved_cursor = None;
        self.reset_attributes();
    }

    /// Write a character at the current cursor position
    pub fn write_char(&mut self, ch: char) {
        if ch == '\n' {
            self.cursor_y += 1;
            self.cursor_x = 0;
        } else if ch == '\r' {
            self.cursor_x = 0;
        } else if ch == '\t' {
            self.cursor_x = ((self.cursor_x / 8) + 1) * 8;
        } else {
            if self.cursor_x < self.width && self.cursor_y < self.height {
                let row = self.cursor_y as usize;
                let col = self.cursor_x as usize;
                self.buffer[row][col] = ch;
                self.fg_colors[row][col] = self.current_fg;
                self.bg_colors[row][col] = self.current_bg;
                self.attributes[row][col] = self.current_attrs;
            }
            self.cursor_x += 1;
        }

        // Handle line wrapping
        if self.cursor_x >= self.width {
            self.cursor_x = 0;
            self.cursor_y += 1;
        }

        // Handle scrolling
        if self.cursor_y >= self.height {
            // Scroll up
            self.buffer.remove(0);
            self.fg_colors.remove(0);
            self.bg_colors.remove(0);
            self.attributes.remove(0);

            self.buffer.push(vec![' '; self.width as usize]);
            self.fg_colors.push(vec![[255, 255, 255]; self.width as usize]);
            self.bg_colors.push(vec![[0, 0, 0]; self.width as usize]);
            self.attributes.push(vec![CellAttributes::default(); self.width as usize]);

            self.cursor_y = self.height - 1;
        }
    }

    /// Move cursor to position
    pub fn move_cursor(&mut self, x: u32, y: u32) {
        self.cursor_x = x.min(self.width.saturating_sub(1));
        self.cursor_y = y.min(self.height.saturating_sub(1));
    }

    /// Set current foreground color
    pub fn set_fg_color(&mut self, color: [u8; 3]) {
        self.current_fg = color;
    }

    /// Set current background color
    pub fn set_bg_color(&mut self, color: [u8; 3]) {
        self.current_bg = color;
    }

    /// Reset current attributes to defaults
    pub fn reset_attributes(&mut self) {
        self.current_fg = self.default_fg;
        self.current_bg = self.default_bg;
        self.current_attrs = CellAttributes::default();
    }

    pub fn reset_fg(&mut self) {
        self.current_fg = self.default_fg;
    }

    pub fn reset_bg(&mut self) {
        self.current_bg = self.default_bg;
    }

    /// Set bold attribute
    pub fn set_bold(&mut self, enabled: bool) {
        self.current_attrs.bold = enabled;
    }

    /// Set underline attribute
    pub fn set_underline(&mut self, enabled: bool) {
        self.current_attrs.underline = enabled;
    }

    /// Set inverse (reverse video) attribute
    pub fn set_inverse(&mut self, enabled: bool) {
        self.current_attrs.inverse = enabled;
    }

    /// Enter alternate screen buffer (used by vim, less, htop, etc.)
    pub fn enter_alternate_screen(&mut self) {
        if self.in_alternate_screen {
            return; // Already in alternate screen
        }

        // Save current screen state
        let saved = SavedScreen {
            buffer: self.buffer.clone(),
            fg_colors: self.fg_colors.clone(),
            bg_colors: self.bg_colors.clone(),
            attributes: self.attributes.clone(),
            cursor_x: self.cursor_x,
            cursor_y: self.cursor_y,
        };
        self.alternate_screen = Some(Box::new(saved));
        self.in_alternate_screen = true;

        // Clear the screen for the alternate buffer
        self.clear();
    }

    /// Leave alternate screen buffer and restore previous state
    pub fn leave_alternate_screen(&mut self) {
        if !self.in_alternate_screen {
            return; // Not in alternate screen
        }

        if let Some(saved) = self.alternate_screen.take() {
            self.buffer = saved.buffer;
            self.fg_colors = saved.fg_colors;
            self.bg_colors = saved.bg_colors;
            self.attributes = saved.attributes;
            self.cursor_x = saved.cursor_x;
            self.cursor_y = saved.cursor_y;
        }
        self.in_alternate_screen = false;
    }

    /// Check if we're in the alternate screen
    pub fn is_alternate_screen(&self) -> bool {
        self.in_alternate_screen
    }

    /// Clear from cursor to end of line
    pub fn clear_line_from_cursor(&mut self) {
        if self.cursor_y >= self.height {
            return;
        }
        for x in self.cursor_x..self.width {
            let idx = x as usize;
            let row = self.cursor_y as usize;
            self.buffer[row][idx] = ' ';
            self.fg_colors[row][idx] = self.current_fg;
            self.bg_colors[row][idx] = self.current_bg;
            self.attributes[row][idx] = CellAttributes::default();
        }
    }

    /// Clear from cursor to end of screen
    pub fn clear_from_cursor(&mut self) {
        let start_row = self.cursor_y;
        for y in start_row..self.height {
            let start_col = if y == start_row { self.cursor_x } else { 0 };
            for x in start_col..self.width {
                let row = y as usize;
                let col = x as usize;
                self.buffer[row][col] = ' ';
                self.fg_colors[row][col] = self.current_fg;
                self.bg_colors[row][col] = self.current_bg;
                self.attributes[row][col] = CellAttributes::default();
            }
        }
    }

    /// Move cursor relative
    pub fn move_cursor_rel(&mut self, dx: i32, dy: i32) {
        let new_x = (self.cursor_x as i32 + dx).clamp(0, self.width.saturating_sub(1) as i32);
        let new_y = (self.cursor_y as i32 + dy).clamp(0, self.height.saturating_sub(1) as i32);
        self.cursor_x = new_x as u32;
        self.cursor_y = new_y as u32;
    }

    /// Save cursor position
    pub fn save_cursor(&mut self) {
        self.saved_cursor = Some((self.cursor_x, self.cursor_y));
    }

    /// Restore cursor position
    pub fn restore_cursor(&mut self) {
        if let Some((x, y)) = self.saved_cursor {
            self.cursor_x = x.min(self.width.saturating_sub(1));
            self.cursor_y = y.min(self.height.saturating_sub(1));
        }
    }

    /// Handle backspace
    pub fn backspace(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        }
    }

    /// Render the terminal to an image buffer
    pub fn render_to_image(&self) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        let img_width = self.width * FONT_WIDTH * PIXEL_SCALE;
        let img_height = self.height * FONT_HEIGHT * PIXEL_SCALE;

        let mut img = ImageBuffer::new(img_width, img_height);

        for y in 0..self.height {
            for x in 0..self.width {
                let ch = self.buffer[y as usize][x as usize];
                let mut fg = self.fg_colors[y as usize][x as usize];
                let mut bg = self.bg_colors[y as usize][x as usize];
                let attrs = self.attributes[y as usize][x as usize];

                // Handle inverse (reverse video)
                if attrs.inverse {
                    std::mem::swap(&mut fg, &mut bg);
                }

                // Handle bold by brightening the foreground color
                if attrs.bold {
                    fg = brighten_color(fg);
                }

                let bitmap = get_char_bitmap(ch);

                for py in 0..FONT_HEIGHT {
                    let row = bitmap[py as usize];
                    for px in 0..FONT_WIDTH {
                        // font8x8 stores the leftmost pixel in the least significant bit
                        let bit = (row >> px) & 1;
                        let mut color = if bit == 1 { fg } else { bg };

                        // Draw underline on the last row of the character cell
                        if attrs.underline && py >= FONT_HEIGHT - 2 {
                            color = fg;
                        }

                        for sy in 0..PIXEL_SCALE {
                            for sx in 0..PIXEL_SCALE {
                                let img_x =
                                    x * FONT_WIDTH * PIXEL_SCALE + px * PIXEL_SCALE + sx;
                                let img_y =
                                    y * FONT_HEIGHT * PIXEL_SCALE + py * PIXEL_SCALE + sy;
                                if img_x < img_width && img_y < img_height {
                                    img.put_pixel(img_x, img_y, Rgb(color));
                                }
                            }
                        }
                    }
                }
            }
        }

        img
    }

    /// Dump the buffer as visible text (for debugging)
    pub fn to_text(&self) -> String {
        let mut out = String::with_capacity((self.width as usize + 1) * self.height as usize);
        for row in &self.buffer {
            for ch in row {
                out.push(*ch);
            }
            out.push('\n');
        }
        out
    }
}

/// VT100 Parser that processes ANSI escape sequences
pub struct Vt100Parser {
    terminal: Vt100Terminal,
    parser: AnsiParser,
}

impl Vt100Parser {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            terminal: Vt100Terminal::new(width, height),
            parser: AnsiParser::new(),
        }
    }

    /// Process a byte of input
    pub fn process_byte(&mut self, byte: u8) {
        let mut performer = TerminalPerformer {
            terminal: &mut self.terminal,
        };
        self.parser.advance(&mut performer, byte);
    }

    /// Get the current terminal state
    pub fn terminal(&self) -> &Vt100Terminal {
        &self.terminal
    }

    /// Get mutable access to the terminal
    pub fn terminal_mut(&mut self) -> &mut Vt100Terminal {
        &mut self.terminal
    }
}

/// Capture a screenshot of a CLI application by emulating it inside a portable PTY
pub fn capture_cli_screenshot_pty(
    config: &super::SnapshotConfig,
    command: &str,
    args: &[String],
    inputs: &[crate::harness::types::InputAction],
) -> super::SnapshotResult<super::Snapshot> {
    use super::utils::{
        create_base_metadata, generate_filename, generate_timestamp, write_description,
        write_manifest,
    };
    use super::{Snapshot, SnapshotError};

    std::fs::create_dir_all(&config.output_dir)?;

    let timestamp = generate_timestamp();
    let filename = generate_filename("cli_screenshot", &timestamp);
    let image_path = config.output_dir.join(&filename);

    let terminal_width: u16 = DEFAULT_TERMINAL_WIDTH;
    let terminal_height: u16 = DEFAULT_TERMINAL_HEIGHT;
    let mut parser = Vt100Parser::new(u32::from(terminal_width), u32::from(terminal_height));

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: terminal_height,
        cols: terminal_width,
        pixel_width: 0,
        pixel_height: 0,
    })
    .map_err(|e| SnapshotError::Capture(format!("Failed to open PTY: {}", e)))?;

    let resolved_command = resolve_binary_path(command);
    let program = resolved_command
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| command.to_string());

    let mut cmd = CommandBuilder::new(program.clone());
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLUMNS", terminal_width.to_string());
    cmd.env("LINES", terminal_height.to_string());
    for arg in args {
        cmd.arg(arg);
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| SnapshotError::Capture(format!("Failed to spawn '{}': {}", program, e)))?;
    drop(pair.slave);

    if let Err(err) = pair.master.resize(PtySize {
        rows: terminal_height,
        cols: terminal_width,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        eprintln!("Warning: unable to resize PTY to {}x{}: {}", terminal_width, terminal_height, err);
    }

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| SnapshotError::Capture(format!("Failed to clone PTY reader: {}", e)))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| SnapshotError::Capture(format!("Failed to take PTY writer: {}", e)))?;

    let rx = spawn_reader(reader);

    wait_for_initial_render(&rx, &mut parser);

    for input in inputs {
        match input {
            crate::harness::types::InputAction::SendString(text) => {
                writer.write_all(text.as_bytes()).map_err(|e| {
                    SnapshotError::Capture(format!("Failed to send text '{}': {}", text, e))
                })?;
                writer
                    .write_all(&[b'\r'])
                    .map_err(|e| SnapshotError::Capture(format!("Failed to send enter: {}", e)))?;
                writer.flush().map_err(SnapshotError::Io)?;
                wait_for_input_render(&rx, &mut parser);
            }
            crate::harness::types::InputAction::SendKey(key) => {
                let sequence = key_to_sequence(key);
                writer.write_all(&sequence).map_err(|e| {
                    SnapshotError::Capture(format!("Failed to send key '{}': {}", key, e))
                })?;
                writer.flush().map_err(SnapshotError::Io)?;
                wait_for_input_render(&rx, &mut parser);
            }
        }
    }

    wait_for_input_render(&rx, &mut parser);
    drop(writer);
    wait_for_process_exit(child.as_mut(), &rx, &mut parser, PROCESS_DRAIN_TIMEOUT);

    if child
        .try_wait()
        .map_err(|e| SnapshotError::Capture(format!("Failed to poll child: {}", e)))?
        .is_none()
    {
        let _ = child.kill();
        let _ = child.wait();
    }

    if std::env::var_os("CLI_SNAPSHOT_DUMP").is_some() {
        println!("--- CLI snapshot buffer ---");
        println!("{}", parser.terminal().to_text());
    }

    let img = parser.terminal().render_to_image();
    img.save(&image_path)
        .map_err(|e| SnapshotError::Io(std::io::Error::other(e.to_string())))?;

    let metadata = if config.include_metadata {
        let meta = create_base_metadata(
            u32::from(terminal_width) * CELL_WIDTH,
            u32::from(terminal_height) * CELL_HEIGHT,
            "cli_pty",
            &timestamp,
        );
        Some(serde_json::Value::Object(meta))
    } else {
        None
    };

    let snapshot = Snapshot::new(image_path.clone(), "cli_pty".to_string(), metadata);
    write_manifest(&snapshot, config)?;
    write_description(&snapshot, config)?;

    Ok(snapshot)
}

/// Result of a single state capture during a multi-input session
#[derive(Debug, Clone)]
pub struct StateCaptureResult {
    /// Step number (0 = initial state)
    pub step: usize,
    /// Input that led to this state (None for initial)
    pub input: Option<String>,
    /// PNG image data
    pub image_data: Vec<u8>,
    /// Image width
    pub width: u32,
    /// Image height
    pub height: u32,
}

/// Terminal size preset for common configurations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalSize {
    /// 80x24 - Classic VT100/minimal terminal
    Compact,
    /// 120x40 - Default, typical modern terminal
    Standard,
    /// 160x50 - Large widescreen terminal
    Large,
    /// 200x60 - Extra large for high-resolution displays
    ExtraLarge,
    /// Custom dimensions
    Custom(u16, u16),
}

impl TerminalSize {
    /// Get the dimensions as (cols, rows)
    pub fn dimensions(&self) -> (u16, u16) {
        match self {
            TerminalSize::Compact => (80, 24),
            TerminalSize::Standard => (120, 40),
            TerminalSize::Large => (160, 50),
            TerminalSize::ExtraLarge => (200, 60),
            TerminalSize::Custom(cols, rows) => (*cols, *rows),
        }
    }

    /// Parse from string (e.g., "80x24", "compact", "standard")
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "compact" | "small" | "minimal" => Some(TerminalSize::Compact),
            "standard" | "default" | "normal" => Some(TerminalSize::Standard),
            "large" | "wide" => Some(TerminalSize::Large),
            "xl" | "extralarge" | "extra-large" => Some(TerminalSize::ExtraLarge),
            _ => {
                // Try parsing as WxH format
                let parts: Vec<&str> = s.split('x').collect();
                if parts.len() == 2 {
                    let cols = parts[0].parse().ok()?;
                    let rows = parts[1].parse().ok()?;
                    Some(TerminalSize::Custom(cols, rows))
                } else {
                    None
                }
            }
        }
    }

    /// Get all preset sizes for testing
    pub fn all_presets() -> Vec<TerminalSize> {
        vec![
            TerminalSize::Compact,
            TerminalSize::Standard,
            TerminalSize::Large,
            TerminalSize::ExtraLarge,
        ]
    }
}

impl Default for TerminalSize {
    fn default() -> Self {
        TerminalSize::Standard
    }
}

/// Parse an input string into bytes to send to the PTY.
fn parse_input(input: &str) -> Vec<u8> {
    let input_lower = input.to_lowercase();
    let input_lower = input_lower.trim();

    match input_lower {
        // Arrow keys
        "up" => b"\x1b[A".to_vec(),
        "down" => b"\x1b[B".to_vec(),
        "right" => b"\x1b[C".to_vec(),
        "left" => b"\x1b[D".to_vec(),
        // Navigation keys
        "home" => b"\x1b[H".to_vec(),
        "end" => b"\x1b[F".to_vec(),
        "pageup" | "page_up" | "pgup" => b"\x1b[5~".to_vec(),
        "pagedown" | "page_down" | "pgdn" => b"\x1b[6~".to_vec(),
        "insert" | "ins" => b"\x1b[2~".to_vec(),
        "delete" | "del" => b"\x1b[3~".to_vec(),
        // Common keys
        "enter" | "return" => vec![b'\r'],
        "space" => vec![b' '],
        "tab" => vec![b'\t'],
        "backspace" | "bs" => vec![0x7f],
        "escape" | "esc" => vec![0x1b],
        // Function keys
        "f1" => b"\x1bOP".to_vec(),
        "f2" => b"\x1bOQ".to_vec(),
        "f3" => b"\x1bOR".to_vec(),
        "f4" => b"\x1bOS".to_vec(),
        "f5" => b"\x1b[15~".to_vec(),
        "f6" => b"\x1b[17~".to_vec(),
        "f7" => b"\x1b[18~".to_vec(),
        "f8" => b"\x1b[19~".to_vec(),
        "f9" => b"\x1b[20~".to_vec(),
        "f10" => b"\x1b[21~".to_vec(),
        "f11" => b"\x1b[23~".to_vec(),
        "f12" => b"\x1b[24~".to_vec(),
        // Ctrl combinations
        s if s.starts_with("ctrl+") || s.starts_with("ctrl-") || s.starts_with("c-") => {
            let key = s.split(&['+', '-'][..]).last().unwrap_or("");
            if key.len() == 1 {
                let ch = key.chars().next().unwrap().to_ascii_lowercase();
                if ch.is_ascii_lowercase() {
                    vec![(ch as u8) - b'a' + 1]
                } else {
                    input.as_bytes().to_vec()
                }
            } else if key == "space" {
                vec![0x00]
            } else {
                input.as_bytes().to_vec()
            }
        }
        // Alt combinations (send ESC prefix)
        s if s.starts_with("alt+") || s.starts_with("alt-") || s.starts_with("m-") => {
            let key = s.split(&['+', '-'][..]).last().unwrap_or("");
            let mut result = vec![0x1b];
            result.extend(key.as_bytes());
            result
        }
        // Single character or literal text
        _ => input.as_bytes().to_vec(),
    }
}

/// Run a CLI application with a sequence of inputs, capturing state after each.
///
/// Returns N+1 captures for N inputs (initial state + state after each input).
pub fn run_with_inputs(
    command: &str,
    args: &[String],
    inputs: &[String],
    input_delay_ms: u64,
) -> super::SnapshotResult<Vec<StateCaptureResult>> {
    run_with_inputs_sized(command, args, inputs, input_delay_ms, TerminalSize::default())
}

/// Run a CLI application with a sequence of inputs at a specific terminal size.
///
/// Returns N+1 captures for N inputs (initial state + state after each input).
pub fn run_with_inputs_sized(
    command: &str,
    args: &[String],
    inputs: &[String],
    input_delay_ms: u64,
    size: TerminalSize,
) -> super::SnapshotResult<Vec<StateCaptureResult>> {
    use super::SnapshotError;

    let (terminal_width, terminal_height) = size.dimensions();
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

    let resolved_command = resolve_binary_path(command);
    let program = resolved_command
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| command.to_string());

    let mut cmd = CommandBuilder::new(program.clone());
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLUMNS", terminal_width.to_string());
    cmd.env("LINES", terminal_height.to_string());
    for arg in args {
        cmd.arg(arg);
    }

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| SnapshotError::Capture(format!("Failed to spawn '{}': {}", program, e)))?;
    drop(pair.slave);

    if let Err(err) = pair.master.resize(PtySize {
        rows: terminal_height,
        cols: terminal_width,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        eprintln!(
            "Warning: unable to resize PTY to {}x{}: {}",
            terminal_width, terminal_height, err
        );
    }

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| SnapshotError::Capture(format!("Failed to clone PTY reader: {}", e)))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| SnapshotError::Capture(format!("Failed to take PTY writer: {}", e)))?;

    let rx = spawn_reader(reader);

    let mut captures = Vec::with_capacity(inputs.len() + 1);

    let img_width = u32::from(terminal_width) * CELL_WIDTH;
    let img_height = u32::from(terminal_height) * CELL_HEIGHT;

    // Wait for initial render and capture state 0
    wait_for_initial_render(&rx, &mut parser);
    captures.push(StateCaptureResult {
        step: 0,
        input: None,
        image_data: render_to_png(&parser),
        width: img_width,
        height: img_height,
    });

    // Process each input
    for (i, input) in inputs.iter().enumerate() {
        // Apply delay before sending input
        if input_delay_ms > 0 {
            thread::sleep(Duration::from_millis(input_delay_ms));
        }

        // Parse and send the input
        let sequence = parse_input(input);
        writer.write_all(&sequence).map_err(|e| {
            SnapshotError::Capture(format!("Failed to send input '{}': {}", input, e))
        })?;
        writer.flush().map_err(SnapshotError::Io)?;

        // Wait for render to settle (shorter timeout per-input)
        wait_for_input_render(&rx, &mut parser);

        // Capture this state
        captures.push(StateCaptureResult {
            step: i + 1,
            input: Some(input.clone()),
            image_data: render_to_png(&parser),
            width: img_width,
            height: img_height,
        });
    }

    // Clean up
    drop(writer);
    wait_for_process_exit(child.as_mut(), &rx, &mut parser, PROCESS_DRAIN_TIMEOUT);

    if child
        .try_wait()
        .map_err(|e| SnapshotError::Capture(format!("Failed to poll child: {}", e)))?
        .is_none()
    {
        let _ = child.kill();
        let _ = child.wait();
    }

    Ok(captures)
}

/// Render the current terminal state to PNG bytes
fn render_to_png(parser: &Vt100Parser) -> Vec<u8> {
    let img = parser.terminal().render_to_image();
    let mut png_data = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_data);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .expect("Failed to encode PNG");
    png_data
}

fn spawn_reader(mut reader: Box<dyn Read + Send>) -> Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    if tx.send(buffer[..size].to_vec()).is_err() {
                        break;
                    }
                }
                Err(err) => match err.kind() {
                    ErrorKind::Interrupted => continue,
                    ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    _ => break,
                },
            }
        }
    });
    rx
}

fn wait_for_initial_render(rx: &Receiver<Vec<u8>>, parser: &mut Vt100Parser) {
    drain_until_quiet_with_max(rx, parser, QUIET_WINDOW, MAX_INITIAL_RENDER_WAIT);
}

fn wait_for_input_render(rx: &Receiver<Vec<u8>>, parser: &mut Vt100Parser) {
    drain_until_quiet_with_max(rx, parser, QUIET_WINDOW, MAX_INPUT_RENDER_WAIT);
}

fn wait_for_process_exit(
    child: &mut dyn Child,
    rx: &Receiver<Vec<u8>>,
    parser: &mut Vt100Parser,
    max_wait: Duration,
) {
    let start = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                drain_until_quiet(rx, parser, QUIET_WINDOW);
                return;
            }
            Ok(None) => {}
            Err(err) => {
                eprintln!("Warning: failed to poll PTY child: {}", err);
                break;
            }
        }

        if start.elapsed() >= max_wait {
            break;
        }

        match rx.recv_timeout(Duration::from_millis(60)) {
            Ok(chunk) => ingest_chunk(&chunk, parser),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn drain_until_quiet(
    rx: &Receiver<Vec<u8>>,
    parser: &mut Vt100Parser,
    quiet_window: Duration,
) {
    drain_until_quiet_with_max(rx, parser, quiet_window, MAX_INPUT_RENDER_WAIT);
}

/// Drain output until quiet or max time reached.
/// This handles apps that continuously output (like animations).
fn drain_until_quiet_with_max(
    rx: &Receiver<Vec<u8>>,
    parser: &mut Vt100Parser,
    quiet_window: Duration,
    max_wait: Duration,
) {
    let start = Instant::now();
    let mut last_activity = Instant::now();

    loop {
        // Check if we've exceeded max wait time
        if start.elapsed() >= max_wait {
            break;
        }

        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => {
                ingest_chunk(&chunk, parser);
                last_activity = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {
                if last_activity.elapsed() >= quiet_window {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    // Final drain of any remaining data
    while let Ok(chunk) = rx.try_recv() {
        ingest_chunk(&chunk, parser);
    }
}

fn ingest_chunk(chunk: &[u8], parser: &mut Vt100Parser) {
    for &byte in chunk {
        parser.process_byte(byte);
    }
}

fn resolve_binary_path(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);

    let looks_like_path = path.is_absolute()
        || command.contains(std::path::MAIN_SEPARATOR)
        || command.starts_with("./")
        || command.starts_with(".\\");

    if !looks_like_path {
        return None;
    }

    if path.exists() {
        std::fs::canonicalize(path).ok()
    } else {
        Some(path.to_path_buf())
    }
}

/// Translate a logical key label into the VT100 control sequence used by the demo
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
        other if other.len() == 1 => other.as_bytes().to_vec(),
        other => other.as_bytes().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font8x8_bitmaps_are_scaled_consistently() {
        let bitmap = get_char_bitmap('A');
        assert!(
            bitmap.iter().any(|row| *row != 0),
            "bitmap should contain lit pixels"
        );
        for pair in bitmap.chunks_exact(2) {
            assert_eq!(
                pair[0], pair[1],
                "each row should be doubled to fill the cell height"
            );
        }
    }

    #[test]
    fn rendered_pixels_follow_font_bitmaps() {
        let mut terminal = Vt100Terminal::new(1, 2);
        let fg = [200, 210, 220];
        let bg = [10, 20, 30];
        terminal.set_fg_color(fg);
        terminal.set_bg_color(bg);
        terminal.write_char('R');
        assert_eq!(terminal.fg_colors[0][0], fg);
        assert_eq!(terminal.bg_colors[0][0], bg);

        let bitmap = get_char_bitmap('R');
        let image = terminal.render_to_image();

        for (py, row) in bitmap.iter().enumerate() {
            for px in 0..FONT_WIDTH as usize {
                let expected_bit = (row >> px) & 1;
                let sample_x = px as u32 * PIXEL_SCALE;
                let sample_y = py as u32 * PIXEL_SCALE;
                let pixel = image.get_pixel(sample_x, sample_y).0;
                if expected_bit == 1 {
                    assert_eq!(
                        pixel, fg,
                        "Expected foreground at glyph position ({px}, {py})"
                    );
                } else {
                    assert_eq!(
                        pixel, bg,
                        "Expected background at glyph position ({px}, {py})"
                    );
                }
            }
        }
    }
}

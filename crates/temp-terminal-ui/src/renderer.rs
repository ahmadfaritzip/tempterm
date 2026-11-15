//! Terminal renderer: parses ANSI SGR color/style sequences and layout cells.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalColor {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Rgb(u8, u8, u8),
}

/// Converts a 256-color palette index (0..255) into 24-bit RGB using standard xterm palette.
pub fn xterm_256_to_rgb(n: u8) -> (u8, u8, u8) {
    match n {
        // Standard 16 ANSI colors
        0 => (0x11, 0x16, 0x1c),
        1 => (0xff, 0x6f, 0x6f),
        2 => (0x63, 0xd5, 0x8d),
        3 => (0xe9, 0xc6, 0x6d),
        4 => (0x5a, 0xa9, 0xff),
        5 => (0xc7, 0x92, 0xea),
        6 => (0x61, 0xd8, 0xdf),
        7 => (0xd7, 0xdd, 0xe5),
        8 => (0x7f, 0x8a, 0x99),
        9 => (0xff, 0x85, 0x85),
        10 => (0xb9, 0xf2, 0x63),
        11 => (0xf3, 0xa8, 0x5f),
        12 => (0x7c, 0xbc, 0xff),
        13 => (0xd7, 0xa8, 0xf0),
        14 => (0x86, 0xe3, 0xe8),
        15 => (0xff, 0xff, 0xff),
        // 6x6x6 color cube (indices 16..231)
        16..=231 => {
            let idx = n - 16;
            let r = idx / 36;
            let g = (idx / 6) % 6;
            let b = idx % 6;
            let cv = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (cv(r), cv(g), cv(b))
        }
        // Grayscale ramp from 8 to 238 in steps of 10 (indices 232..255)
        232..=255 => {
            let gray = 8 + (n - 232) * 10;
            (gray, gray, gray)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalCell {
    pub ch: char,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub bold: bool,
    pub underline: bool,
    pub faint: bool,
    pub italic: bool,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: TerminalColor::Default,
            bg: TerminalColor::Default,
            bold: false,
            underline: false,
            faint: false,
            italic: false,
        }
    }
}

pub struct TerminalRenderer {
    pub font_size: u32,
    pub line_height: f32,
}

impl TerminalRenderer {
    pub fn new(font_size: u32, line_height: f32) -> Self {
        Self {
            font_size,
            line_height,
        }
    }

    /// Parses an ANSI formatted string line into stylized `TerminalCell`s.
    pub fn parse_ansi_line(&self, line: &str) -> Vec<TerminalCell> {
        let mut cells = Vec::new();
        let mut fg = TerminalColor::Default;
        let mut bg = TerminalColor::Default;
        let mut bold = false;
        let mut underline = false;
        let mut faint = false;
        let mut italic = false;
        let mut inverse = false;

        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                let mut params_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() || c == ';' {
                        params_str.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let cmd = chars.next().unwrap_or(' ');
                if cmd == 'm' {
                    // SGR command
                    if params_str.is_empty() {
                        // Reset
                        fg = TerminalColor::Default;
                        bg = TerminalColor::Default;
                        bold = false;
                        underline = false;
                        faint = false;
                        italic = false;
                        inverse = false;
                    } else {
                        let nums: Vec<u32> = params_str
                            .split(';')
                            .map(|s| s.parse::<u32>().unwrap_or(0))
                            .collect();
                        let mut i = 0;
                        while i < nums.len() {
                            match nums[i] {
                                0 => {
                                    fg = TerminalColor::Default;
                                    bg = TerminalColor::Default;
                                    bold = false;
                                    underline = false;
                                    faint = false;
                                    italic = false;
                                    inverse = false;
                                }
                                1 => bold = true,
                                2 => faint = true,
                                3 => italic = true,
                                4 => underline = true,
                                7 => inverse = true,
                                22 => {
                                    bold = false;
                                    faint = false;
                                }
                                23 => italic = false,
                                24 => underline = false,
                                27 => inverse = false,
                                30 => fg = TerminalColor::Black,
                                31 => fg = TerminalColor::Red,
                                32 => fg = TerminalColor::Green,
                                33 => fg = TerminalColor::Yellow,
                                34 => fg = TerminalColor::Blue,
                                35 => fg = TerminalColor::Magenta,
                                36 => fg = TerminalColor::Cyan,
                                37 => fg = TerminalColor::White,
                                38 => {
                                    // Extended foreground color: 38;2;r;g;b or 38;5;n
                                    if i + 1 < nums.len() {
                                        if nums[i + 1] == 2 && i + 4 < nums.len() {
                                            let r = nums[i + 2].min(255) as u8;
                                            let g = nums[i + 3].min(255) as u8;
                                            let b = nums[i + 4].min(255) as u8;
                                            fg = TerminalColor::Rgb(r, g, b);
                                            i += 4;
                                        } else if nums[i + 1] == 5 && i + 2 < nums.len() {
                                            let n = nums[i + 2].min(255) as u8;
                                            let (r, g, b) = xterm_256_to_rgb(n);
                                            fg = TerminalColor::Rgb(r, g, b);
                                            i += 2;
                                        }
                                    }
                                }
                                39 => fg = TerminalColor::Default,
                                40 => bg = TerminalColor::Black,
                                41 => bg = TerminalColor::Red,
                                42 => bg = TerminalColor::Green,
                                43 => bg = TerminalColor::Yellow,
                                44 => bg = TerminalColor::Blue,
                                45 => bg = TerminalColor::Magenta,
                                46 => bg = TerminalColor::Cyan,
                                47 => bg = TerminalColor::White,
                                48 => {
                                    // Extended background color: 48;2;r;g;b or 48;5;n
                                    if i + 1 < nums.len() {
                                        if nums[i + 1] == 2 && i + 4 < nums.len() {
                                            let r = nums[i + 2].min(255) as u8;
                                            let g = nums[i + 3].min(255) as u8;
                                            let b = nums[i + 4].min(255) as u8;
                                            bg = TerminalColor::Rgb(r, g, b);
                                            i += 4;
                                        } else if nums[i + 1] == 5 && i + 2 < nums.len() {
                                            let n = nums[i + 2].min(255) as u8;
                                            let (r, g, b) = xterm_256_to_rgb(n);
                                            bg = TerminalColor::Rgb(r, g, b);
                                            i += 2;
                                        }
                                    }
                                }
                                49 => bg = TerminalColor::Default,
                                90 => fg = TerminalColor::BrightBlack,
                                91 => fg = TerminalColor::BrightRed,
                                92 => fg = TerminalColor::BrightGreen,
                                93 => fg = TerminalColor::BrightYellow,
                                94 => fg = TerminalColor::BrightBlue,
                                95 => fg = TerminalColor::BrightMagenta,
                                96 => fg = TerminalColor::BrightCyan,
                                97 => fg = TerminalColor::BrightWhite,
                                100 => bg = TerminalColor::BrightBlack,
                                101 => bg = TerminalColor::BrightRed,
                                102 => bg = TerminalColor::BrightGreen,
                                103 => bg = TerminalColor::BrightYellow,
                                104 => bg = TerminalColor::BrightBlue,
                                105 => bg = TerminalColor::BrightMagenta,
                                106 => bg = TerminalColor::BrightCyan,
                                107 => bg = TerminalColor::BrightWhite,
                                _ => {}
                            }
                            i += 1;
                        }
                    }
                }
            } else if !ch.is_control() {
                let (cell_fg, cell_bg) = if inverse { (bg, fg) } else { (fg, bg) };
                cells.push(TerminalCell {
                    ch,
                    fg: cell_fg,
                    bg: cell_bg,
                    bold,
                    underline,
                    faint,
                    italic,
                });
            }
        }

        cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ansi_truecolor_24bit_fg_bg_no_faint_corruption() {
        let renderer = TerminalRenderer::new(14, 18.0);
        // TrueColor fg: 38;2;255;111;111 and TrueColor bg: 48;2;17;22;28
        let raw = "\x1b[38;2;255;111;111;48;2;17;22;28mAGY\x1b[0m";
        let cells = renderer.parse_ansi_line(raw);
        assert_eq!(cells.len(), 3);
        for c in &cells {
            assert_eq!(c.fg, TerminalColor::Rgb(255, 111, 111));
            assert_eq!(c.bg, TerminalColor::Rgb(17, 22, 28));
            // Crucial: parameter 2 in 38;2;... must NOT be parsed as faint!
            assert!(!c.faint, "Parameter 2 in 24-bit TrueColor must not trigger faint attribute");
        }
    }

    #[test]
    fn test_parse_ansi_256_colors() {
        let renderer = TerminalRenderer::new(14, 18.0);
        // 256 color index 196 (bright red in standard xterm 256 color table: 255, 0, 0)
        let raw = "\x1b[38;5;196mColor196\x1b[0m";
        let cells = renderer.parse_ansi_line(raw);
        assert_eq!(cells.len(), 8);
        assert_eq!(cells[0].fg, TerminalColor::Rgb(255, 0, 0));
        assert!(!cells[0].faint);
    }

    #[test]
    fn test_parse_ansi_standard_colors_and_inverse() {
        let renderer = TerminalRenderer::new(14, 18.0);
        let raw = "\x1b[31;42;7mInv\x1b[27mNorm\x1b[0m";
        let cells = renderer.parse_ansi_line(raw);
        assert_eq!(cells.len(), 7);
        // First 3 inverted: fg and bg swapped (fg=Green, bg=Red)
        assert_eq!(cells[0].fg, TerminalColor::Green);
        assert_eq!(cells[0].bg, TerminalColor::Red);
        // Next 4 normal (not inverted): fg=Red, bg=Green
        assert_eq!(cells[3].fg, TerminalColor::Red);
        assert_eq!(cells[3].bg, TerminalColor::Green);
    }
}

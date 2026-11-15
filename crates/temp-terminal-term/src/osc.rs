//! Streaming OSC scanner and status report parsing.
//! Ported with exact parity from `tempterm/internal/osc`.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Longest sequence kept, matching Go `osc.MaxLen = 4096`.
pub const MAX_LEN: usize = 4096;

/// Maximum kitty keyboard flags pushed.
const MAX_KITTY_STACK: usize = 64;

/// Known prefixes kept by the scanner.
const PREFIXES: &[&[u8]] = &[b"7501;", b"133;", b"9;4;"];

/// Scanner finds OSC 7501, 133 and 9;4 in program output, and full resets (RIS).
#[derive(Debug, Clone, Default)]
pub struct Scanner {
    state: usize, // 0 text, 1 after ESC, 2 in an OSC we keep, 3 in one we skip, 4 ESC in an OSC, 5 in a CSI
    buf: Vec<u8>,
    prev: u8,
    pub bracketed: bool,
    pub kitty: i32,
    pub modify_other_keys: i32,
    kitty_stacks: [Vec<i32>; 2],
    alt: bool,
    csi: Vec<u8>,
}

impl Scanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Scans a slice of incoming terminal bytes, calling `on_osc` for recognized OSC sequence bodies
    /// and `on_reset` when a full terminal reset (RIS `ESC c`) is encountered.
    pub fn scan<F, R>(&mut self, p: &[u8], mut on_osc: F, mut on_reset: R)
    where
        F: FnMut(&[u8]),
        R: FnMut(),
    {
        for &b in p {
            let c1 = self.prev < 0x80;
            self.prev = b;

            match self.state {
                0 => {
                    if b == 0x1b {
                        self.state = 1;
                    } else if b == 0x9d && c1 {
                        self.begin();
                    } else if b == 0x9b && c1 {
                        self.state = 5;
                        self.csi.clear();
                    }
                }
                1 => match b {
                    b']' => self.begin(),
                    b'[' => {
                        self.state = 5;
                        self.csi.clear();
                    }
                    _ if b != 0x1b => {
                        self.state = 0;
                        if b == b'c' {
                            self.bracketed = false;
                            self.kitty = 0;
                            self.kitty_stacks = [Vec::new(), Vec::new()];
                            self.alt = false;
                            self.modify_other_keys = 0;
                            on_reset();
                        }
                    }
                    _ => {}
                },
                5 => match b {
                    0x40..=0x7e => {
                        self.state = 0;
                        self.mode(b);
                    }
                    0x1b => {
                        self.state = 1;
                    }
                    _ if b < 0x20 || self.csi.len() >= 64 => {
                        if self.csi.len() >= 64 {
                            self.state = 0;
                        }
                    }
                    _ => {
                        self.csi.push(b);
                    }
                },
                2 | 3 => match b {
                    0x07 => {
                        self.end(&mut on_osc);
                    }
                    0x9c if c1 => {
                        self.end(&mut on_osc);
                    }
                    0x1b => {
                        self.state = 4;
                    }
                    _ => {
                        if self.state == 2 {
                            self.buf.push(b);
                            if !self.wanted() || self.buf.len() > MAX_LEN {
                                self.state = 3;
                                self.buf.clear();
                            }
                        }
                    }
                },
                4 => {
                    if b == b'\\' {
                        self.end(&mut on_osc);
                    } else {
                        self.state = 1;
                        self.buf.clear();
                        if b == b']' {
                            self.begin();
                        } else if b != 0x1b {
                            self.state = 0;
                        }
                    }
                }
                _ => self.state = 0,
            }
        }
    }

    fn begin(&mut self) {
        self.state = 2;
        self.buf.clear();
    }

    fn wanted(&self) -> bool {
        for &pre in PREFIXES {
            let n = self.buf.len().min(pre.len());
            if self.buf[..n] == pre[..n] {
                return true;
            }
        }
        false
    }

    fn end<F: FnMut(&[u8])>(&mut self, on_osc: &mut F) {
        if (self.state == 2 || (self.state == 4 && !self.buf.is_empty()))
            && self.wanted()
            && !self.buf.is_empty()
        {
            on_osc(&self.buf);
        }
        self.state = 0;
        self.buf.clear();
    }

    fn mode(&mut self, final_byte: u8) {
        match final_byte {
            b'h' | b'l' => self.dec_mode(final_byte),
            b'u' => self.keyboard(),
            b'm' => self.modify_other_keys(),
            _ => {}
        }
    }

    fn dec_mode(&mut self, final_byte: u8) {
        if self.csi.len() < 2 || self.csi[0] != b'?' {
            return;
        }
        let modes: Vec<Vec<u8>> = self.csi[1..]
            .split(|&b| b == b';')
            .map(|s| s.to_vec())
            .collect();
        for m in modes {
            match m.as_slice() {
                b"2004" => self.bracketed = final_byte == b'h',
                b"47" | b"1047" | b"1049" => self.screen(final_byte == b'h'),
                _ => {}
            }
        }
    }

    fn screen(&mut self, on: bool) {
        if on == self.alt {
            return;
        }
        self.alt = on;
        self.kitty_stacks[1].clear();
        self.set_kitty();
    }

    fn stack_mut(&mut self) -> &mut Vec<i32> {
        if self.alt {
            &mut self.kitty_stacks[1]
        } else {
            &mut self.kitty_stacks[0]
        }
    }

    fn set_kitty(&mut self) {
        let st = self.stack_mut();
        self.kitty = st.last().copied().unwrap_or(0);
    }

    fn keyboard(&mut self) {
        if self.csi.is_empty() {
            return;
        }
        let first = self.csi[0];
        let params: Vec<i32> = self.csi[1..]
            .split(|&b| b == b';')
            .map(|chunk| {
                std::str::from_utf8(chunk)
                    .unwrap_or("")
                    .parse::<i32>()
                    .unwrap_or(0)
            })
            .collect();
        let num = |i: usize| -> i32 { params.get(i).copied().unwrap_or(0) };

        match first {
            b'>' => {
                let st = self.stack_mut();
                if st.len() >= MAX_KITTY_STACK {
                    st.remove(0);
                }
                st.push(num(0));
            }
            b'<' => {
                let mut n = num(0);
                if n < 1 {
                    n = 1;
                }
                let st = self.stack_mut();
                let to_pop = (n as usize).min(st.len());
                st.truncate(st.len() - to_pop);
            }
            b'=' => {
                let st = self.stack_mut();
                if st.is_empty() {
                    st.push(0);
                }
                let flags = num(0);
                let mode = num(1);
                let last_idx = st.len() - 1;
                let mut cur = st[last_idx];
                match mode {
                    2 => cur |= flags,
                    3 => cur &= !flags,
                    _ => cur = flags,
                }
                st[last_idx] = cur;
            }
            _ => return,
        }
        self.set_kitty();
    }

    fn modify_other_keys(&mut self) {
        if self.csi.len() < 2 || self.csi[0] != b'>' {
            return;
        }
        let params: Vec<&[u8]> = self.csi[1..].split(|&b| b == b';').collect();
        if params.is_empty() || params[0] != b"4" {
            return;
        }
        if params.len() == 1 {
            self.modify_other_keys = 0;
            return;
        }
        if let Ok(s) = std::str::from_utf8(params[1]) {
            if let Ok(n) = s.parse::<i32>() {
                self.modify_other_keys = n;
            }
        }
    }
}

/// A parsed OSC 7501 report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub query: bool,
    pub record: temp_terminal_proto::proto::OscRecord,
}

/// Query reply for OSC 7501 query.
pub const QUERY_REPLY: &str = "\x1b]7501;?\x1b\\";

/// Parses the body of an OSC 7501 sequence following `7501;`.
pub fn parse_status(body: &str) -> Option<Report> {
    let body = body.trim();
    if body == "?" || body.starts_with("?:") {
        return Some(Report {
            query: true,
            record: temp_terminal_proto::proto::OscRecord::default(),
        });
    }

    let is_key_char = |c: char| c.is_ascii_lowercase() && c.is_ascii_alphabetic();
    let is_val_char =
        |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ',' | '+' | '/' | '=' | '-');
    let is_seg_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '+' | '-');

    let mut pairs = HashMap::new();
    for pair in body.split(':') {
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => continue,
        };
        if k.is_empty()
            || k.len() > 16
            || !k.chars().all(is_key_char)
            || !v.chars().all(is_val_char)
        {
            continue;
        }
        pairs.insert(k.to_string(), v.to_string());
    }

    let state = pairs.get("state")?.clone();
    match state.as_str() {
        "idle" | "working" | "done" | "blocked" | "error" | "clear" => {}
        _ => return None,
    }

    let mut r = temp_terminal_proto::proto::OscRecord {
        state,
        progress: -1,
        ..Default::default()
    };

    if let Some(id) = pairs.get("id") {
        let segs: Vec<&str> = id.split('/').collect();
        if id.len() > 128
            || segs.len() > 8
            || segs
                .iter()
                .any(|s| s.is_empty() || s.len() > 32 || !s.chars().all(is_seg_char))
        {
            return None;
        }
        r.id = id.clone();
    }

    if r.state == "blocked" {
        if let Some(k) = pairs.get("kind") {
            if matches!(k.as_str(), "permission" | "question" | "auth") {
                r.kind = k.clone();
            }
        }
    }

    if let Some(p_str) = pairs.get("progress") {
        if let Ok(p) = p_str.parse::<i32>() {
            if (0..=100).contains(&p) && (r.state == "working" || r.state == "blocked") {
                r.progress = p as i64;
            }
        }
    }

    if let Some(app) = pairs.get("app") {
        if !app.is_empty() && app.len() <= 32 && app.chars().all(is_seg_char) {
            r.app = app.clone();
        }
    }

    let parse_text = |key: &str, enc_limit: usize, dec_limit: usize| -> Result<String, ()> {
        let v = match pairs.get(key) {
            Some(v) => v,
            None => return Ok(String::new()),
        };
        if v.len() > enc_limit {
            return Err(());
        }
        let bytes = temp_terminal_proto::base64::decode(v).map_err(|_| ())?;
        if bytes.len() > dec_limit {
            return Err(());
        }
        let s = String::from_utf8(bytes).map_err(|_| ())?;
        for c in s.chars() {
            let u = c as u32;
            if u < 0x20 || (0x7f..=0x9f).contains(&u) {
                return Err(());
            }
        }
        Ok(neutralize(&s))
    };

    match parse_text("msg", 2732, 2048) {
        Ok(msg) => r.msg = msg,
        Err(_) => return None,
    }
    match parse_text("title", 256, 192) {
        Ok(title) => r.title = title,
        Err(_) => return None,
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    r.updated = now_ms;

    Some(Report {
        query: false,
        record: r,
    })
}

/// Neutralizes bidirectional overrides and zero-width characters that could mislead text outside grid.
pub fn neutralize(s: &str) -> String {
    s.chars()
        .filter(|&r| {
            let u = r as u32;
            !(matches!(u, 0x202a..=0x202e | 0x2066..=0x2069 | 0x200b..=0x200d)
                || u == 0x200e
                || u == 0x200f
                || u == 0x061c
                || u == 0xfeff)
        })
        .collect()
}

/// Encodes an OscRecord as an OSC 7501 escape sequence.
pub fn encode_status(r: &temp_terminal_proto::proto::OscRecord) -> String {
    let mut s = format!("\x1b]7501;state={}", r.state);
    if !r.id.is_empty() {
        s.push_str(":id=");
        s.push_str(&r.id);
    }
    if !r.kind.is_empty() {
        s.push_str(":kind=");
        s.push_str(&r.kind);
    }
    if r.progress >= 0 {
        s.push_str(&format!(":progress={}", r.progress));
    }
    if !r.app.is_empty() {
        s.push_str(":app=");
        s.push_str(&r.app);
    }
    if !r.title.is_empty() {
        s.push_str(":title=");
        s.push_str(&temp_terminal_proto::base64::encode(r.title.as_bytes()));
    }
    if !r.msg.is_empty() {
        s.push_str(":msg=");
        s.push_str(&temp_terminal_proto::base64::encode(r.msg.as_bytes()));
    }
    s.push_str("\x1b\\");
    s
}

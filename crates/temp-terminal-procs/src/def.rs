//! Process definition, validation, and variant hashing for temp-terminal.
//! Corresponds to Go `tempterm/internal/procs/def.go`.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

/// Errors that can occur when validating a process definition or URL.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DefError {
    #[error("the name {0:?} must be 1 to 32 of a-z, 0-9, - or _, starting with a letter or digit")]
    InvalidName(String),

    #[error("the command is empty")]
    EmptyCommand,

    #[error("the dir must be relative to the project")]
    DirAbsolute,

    #[error("the dir leaves the project")]
    DirLeavesProject,

    #[error("the port {0} must be 0 or 1 to 65535")]
    InvalidPort(i64),

    #[error("the restart {0:?} must be never, on-failure or always")]
    InvalidRestart(String),

    #[error("the url {0:?} has more than three parts")]
    UrlTooManyParts(String),

    #[error("the url {0:?} must be parts of a-z, 0-9 and -, joined by dots, as shop or api.shop")]
    UrlInvalidLabel(String),

    #[error("the url {0:?} leaves out .localhost: tempterm adds it")]
    UrlEndsWithLocalhost(String),
}

/// `Def` is a process a project defines: what to run, where, and how temp-terminal keeps it running.
/// Corresponds to Go `procs.Def` in `tempterm/internal/procs/def.go:21`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Def {
    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub command: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub port: i64,

    #[serde(default, skip_serializing_if = "is_false")]
    pub autostart: bool,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub restart: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub watch: Vec<String>,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,

    #[serde(skip)]
    pub local: bool,
}

#[inline]
fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

#[inline]
fn is_false(v: &bool) -> bool {
    !*v
}

/// Reports whether `name` is a valid process name according to Go `nameRE`:
/// `^[a-z0-9][a-z0-9_-]{0,31}$`.
pub fn is_valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 32 {
        return false;
    }
    let mut chars = name.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' && c != '_' {
            return false;
        }
    }
    true
}

/// Reports whether `name` is a valid URL according to Go `labelRE`:
/// `^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$`, 1 to 3 labels, and cannot end in `.localhost`.
pub fn is_valid_url(name: &str) -> Result<(), DefError> {
    let labels: Vec<&str> = name.split('.').collect();
    if labels.len() > 3 {
        return Err(DefError::UrlTooManyParts(name.to_string()));
    }
    for l in &labels {
        if !is_valid_url_label(l) {
            return Err(DefError::UrlInvalidLabel(name.to_string()));
        }
    }
    if let Some(last) = labels.last() {
        if *last == "localhost" {
            return Err(DefError::UrlEndsWithLocalhost(name.to_string()));
        }
    }
    Ok(())
}

fn is_valid_url_label(label: &str) -> bool {
    if label.is_empty() || label.len() > 63 {
        return false;
    }
    let mut chars = label.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    if label.len() == 1 {
        return true;
    }
    let last = label.chars().last().unwrap();
    if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
        return false;
    }
    for c in label[1..label.len() - 1].chars() {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
            return false;
        }
    }
    true
}

/// Returns the host a URL's name is served at (`name + ".localhost"`).
pub fn host(name: &str) -> String {
    format!("{name}.localhost")
}

/// Normalizes a directory path matching Go `cleanDir`:
/// makes "./web", "web/" and "" the same folder, returning forward slashes.
pub fn clean_dir(dir: &str) -> String {
    if dir.is_empty() {
        return String::new();
    }
    // Replace backslashes with slashes
    let normalized = dir.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        parts.push(part);
    }
    parts.join("/")
}

fn restart_policy(restart: &str) -> &str {
    if restart.is_empty() {
        "never"
    } else {
        restart
    }
}

/// Checks whether `inside` is a local lexical path inside root (matching Go `filepath.IsLocal`).
fn is_local_path(p: &Path) -> bool {
    let mut depth: isize = 0;
    for comp in p.components() {
        match comp {
            Component::Prefix(_) | Component::RootDir => return false,
            Component::CurDir => continue,
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            Component::Normal(_) => {
                depth += 1;
            }
        }
    }
    depth > 0
}

#[derive(serde::Serialize)]
struct VariantKey<'a> {
    command: &'a str,
    dir: String,
    env: Option<&'a BTreeMap<String, String>>,
    port: i64,
    restart: &'a str,
    watch: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<&'a str>,
}

impl Def {
    /// Computes the deterministic SHA-256 variant hash of the definition.
    /// Matches Go `tempterm/internal/procs/def.go:50`.
    pub fn variant(&self) -> String {
        let env_opt = if !self.env.is_empty() {
            Some(&self.env)
        } else {
            None
        };

        let watch_opt = if !self.watch.is_empty() {
            let mut sorted = self.watch.clone();
            sorted.sort();
            Some(sorted)
        } else {
            None
        };

        let url_opt = if !self.url.is_empty() {
            Some(self.url.as_str())
        } else {
            None
        };

        let key = VariantKey {
            command: &self.command,
            dir: clean_dir(&self.dir),
            env: env_opt,
            port: self.port,
            restart: restart_policy(&self.restart),
            watch: watch_opt,
            url: url_opt,
        };

        let json_data = serde_json::to_vec(&key).expect("variantKey serialization cannot fail");
        let hash = sha256_digest(&json_data);
        hex_encode(&hash)
    }

    /// Validates whether the def can run in the project at `root`.
    /// Matches Go `tempterm/internal/procs/def.go:94`.
    pub fn validate<P: AsRef<Path>>(&self, root: P) -> Result<(), DefError> {
        if !is_valid_name(&self.name) {
            return Err(DefError::InvalidName(self.name.clone()));
        }
        if self.command.trim().is_empty() {
            return Err(DefError::EmptyCommand);
        }
        if !self.dir.is_empty() {
            let dir_path = Path::new(&self.dir);
            if dir_path.is_absolute() {
                return Err(DefError::DirAbsolute);
            }
            // Check for escaping relative traversal (e.g. `../foo`)
            let mut joined = PathBuf::from(root.as_ref());
            joined.push(dir_path);
            let inside = match joined.strip_prefix(root.as_ref()) {
                Ok(rel) => rel,
                Err(_) => return Err(DefError::DirLeavesProject),
            };
            if !is_local_path(inside) {
                return Err(DefError::DirLeavesProject);
            }
        }
        if self.port < 0 || self.port > 65535 {
            return Err(DefError::InvalidPort(self.port));
        }
        match self.restart.as_str() {
            "" | "never" | "on-failure" | "always" => {}
            _ => return Err(DefError::InvalidRestart(self.restart.clone())),
        }
        if !self.url.is_empty() {
            is_valid_url(&self.url)?;
        }
        Ok(())
    }
}

/// Standalone pure-Rust SHA-256 implementation (FIPS 180-4) with zero dependencies.
pub fn sha256_digest(data: &[u8]) -> [u8; 32] {
    const H: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    fn update_state(state: &mut [u32; 8], data: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (idx, chunk) in data.chunks_exact(4).enumerate() {
            w[idx] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }

        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut h = *state;
        for i in 0..64 {
            let ch = (h[4] & h[5]) ^ (!h[4] & h[6]);
            let ma = (h[0] & h[1]) ^ (h[0] & h[2]) ^ (h[1] & h[2]);
            let s0 = h[0].rotate_right(2) ^ h[0].rotate_right(13) ^ h[0].rotate_right(22);
            let s1 = h[4].rotate_right(6) ^ h[4].rotate_right(11) ^ h[4].rotate_right(25);
            let t0 = h[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let t1 = s0.wrapping_add(ma);

            h[7] = h[6];
            h[6] = h[5];
            h[5] = h[4];
            h[4] = h[3].wrapping_add(t0);
            h[3] = h[2];
            h[2] = h[1];
            h[1] = h[0];
            h[0] = t0.wrapping_add(t1);
        }

        for (i, v) in state.iter_mut().enumerate() {
            *v = v.wrapping_add(h[i]);
        }
    }

    let mut state = H;
    let mut completed_blocks = 0u64;
    let mut pending = [0u8; 64];
    let mut num_pending = 0usize;

    let mut len = data.len();
    let mut offset = 0;

    if num_pending > 0 && num_pending + len >= 64 {
        pending[num_pending..].copy_from_slice(&data[..64 - num_pending]);
        update_state(&mut state, &pending);
        completed_blocks += 1;
        offset = 64 - num_pending;
        len -= offset;
        num_pending = 0;
    }

    let data_blocks = len / 64;
    let remain = len % 64;
    for _ in 0..data_blocks {
        let mut block = [0u8; 64];
        block.copy_from_slice(&data[offset..offset + 64]);
        update_state(&mut state, &block);
        offset += 64;
    }
    completed_blocks += data_blocks as u64;

    if remain > 0 {
        pending[num_pending..num_pending + remain].copy_from_slice(&data[offset..]);
        num_pending += remain;
    }

    // Finish padding
    let data_bits = completed_blocks * 512 + num_pending as u64 * 8;
    let mut pad = [0u8; 72];
    pad[0] = 128;

    let pad_offset = if num_pending < 56 {
        56 - num_pending
    } else {
        120 - num_pending
    };

    pad[pad_offset..pad_offset + 8].copy_from_slice(&data_bits.to_be_bytes());

    let final_pad = &pad[..pad_offset + 8];
    let mut p_len = final_pad.len();
    let mut p_off = 0;

    if num_pending > 0 && num_pending + p_len >= 64 {
        pending[num_pending..].copy_from_slice(&final_pad[..64 - num_pending]);
        update_state(&mut state, &pending);
        p_off = 64 - num_pending;
        p_len -= p_off;
        num_pending = 0;
    }
    let p_blocks = p_len / 64;
    let p_rem = p_len % 64;
    for _ in 0..p_blocks {
        let mut block = [0u8; 64];
        block.copy_from_slice(&final_pad[p_off..p_off + 64]);
        update_state(&mut state, &block);
        p_off += 64;
    }
    if p_rem > 0 {
        pending[num_pending..num_pending + p_rem].copy_from_slice(&final_pad[p_off..]);
    }

    let mut result = [0u8; 32];
    for (idx, &s) in state.iter().enumerate() {
        result[idx * 4..(idx + 1) * 4].copy_from_slice(&s.to_be_bytes());
    }
    result
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

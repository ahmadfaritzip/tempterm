//! URL parsing and browser URL resolution matching `tempterm/internal/proto/url.go`.

use std::net::IpAddr;
use std::sync::atomic::{AtomicPtr, Ordering};

/// Default blank URL.
pub const BLANK_URL: &str = "about:blank";

static SEARCH_URL: AtomicPtr<String> = AtomicPtr::new(std::ptr::null_mut());

fn default_search_engine() -> &'static str {
    "https://duckduckgo.com/?q=%s"
}

/// Checks if a custom search URL is valid (contains "%s" and has a valid scheme).
pub fn valid_search_url(u: &str) -> bool {
    u.contains("%s") && has_scheme(u)
}

/// Sets the current search engine or custom URL template containing "%s".
pub fn set_search_engine(engine: &str) {
    let resolved = match engine {
        "duckduckgo" => "https://duckduckgo.com/?q=%s",
        "google" => "https://www.google.com/search?q=%s",
        "kagi" => "https://kagi.com/search?q=%s",
        "brave" => "https://search.brave.com/search?q=%s",
        custom if valid_search_url(custom) => custom,
        _ => "https://duckduckgo.com/?q=%s",
    };

    let new_boxed = Box::into_raw(Box::new(resolved.to_string()));
    let prev = SEARCH_URL.swap(new_boxed, Ordering::SeqCst);
    if !prev.is_null() {
        unsafe {
            let _ = Box::from_raw(prev);
        }
    }
}

fn search_for(text: &str) -> String {
    let ptr = SEARCH_URL.load(Ordering::SeqCst);
    let template = if ptr.is_null() {
        default_search_engine()
    } else {
        unsafe { &*ptr }
    };

    let escaped = query_escape(text);
    template.replacen("%s", &escaped, 1)
}

fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => {
                out.push('+');
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

/// Evaluates what a user typed in address bar or CLI into a loadable URL.
pub fn browser_url(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return BLANK_URL.to_string();
    }
    if let Some(addr) = address(text) {
        return addr;
    }
    search_for(text)
}

/// Reports whether text is recognized as an address/host/URL rather than search query.
pub fn is_address(text: &str) -> bool {
    address(text.trim()).is_some()
}

fn address(text: &str) -> Option<String> {
    if has_scheme(text) {
        return Some(text.to_string());
    }
    if text.is_empty() || text.contains([' ', '\t']) {
        return None;
    }

    let mut host = text;
    if let Some(i) = host.find(['/', '?', '#']) {
        host = &host[..i];
    }

    if let Some((h, _)) = split_host_port(host) {
        host = h;
    }

    if local_host(host) {
        return Some(format!("http://{}", text));
    }
    if host.contains('.') && !host.starts_with('.') && !host.ends_with('.') {
        return Some(format!("https://{}", text));
    }

    None
}

fn split_host_port(hostport: &str) -> Option<(&str, &str)> {
    if hostport.starts_with('[') {
        if let Some(end_bracket) = hostport.find(']') {
            let host = &hostport[1..end_bracket];
            let rest = &hostport[end_bracket + 1..];
            if let Some(port) = rest.strip_prefix(':') {
                if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
                    return Some((host, port));
                }
            }
        }
        return None;
    }

    // If it contains multiple colons and isn't enclosed in brackets, it's a bare IPv6 address.
    // In Go net.SplitHostPort returns an error "too many colons in address" and does not split it!
    if hostport.chars().filter(|&c| c == ':').count() > 1 {
        return None;
    }

    if let Some(idx) = hostport.rfind(':') {
        let (host, port) = hostport.split_at(idx);
        let port = &port[1..];
        if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            return Some((host, port));
        }
    }
    None
}

fn has_scheme(text: &str) -> bool {
    let parts: Vec<&str> = text.splitn(2, ':').collect();
    if parts.len() < 2 || parts[0].is_empty() {
        return false;
    }

    let scheme = parts[0];
    if !scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
    {
        return false;
    }

    let rest = parts[1];
    if rest.starts_with("//") {
        return true;
    }

    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "about" | "data" | "mailto" | "javascript" | "blob" | "view-source"
    )
}

fn local_host(host: &str) -> bool {
    let host = host.trim_matches(|c| c == '[' || c == ']');
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".test") {
        return true;
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_unspecified(),
            IpAddr::V6(v6) => {
                // In Go net.IP.IsPrivate() covers RFC 4193 (IPv6 Unique Local Addresses fc00::/7)
                // in addition to loopback and unspecified.
                let is_ula = (v6.segments()[0] & 0xfe00) == 0xfc00;
                v6.is_loopback() || v6.is_unspecified() || is_ula
            }
        }
    } else {
        false
    }
}

/// Returns the host part of a URL stripped of "www.", or raw text if unparseable.
pub fn url_host(raw: &str) -> String {
    if let Some(pos) = raw.find("://") {
        let after_scheme = &raw[pos + 3..];
        let host_part = match after_scheme.find(['/', '?', '#']) {
            Some(end) => &after_scheme[..end],
            None => after_scheme,
        };
        if let Some(stripped) = host_part.strip_prefix("www.") {
            return stripped.to_string();
        }
        return host_part.to_string();
    }
    raw.to_string()
}

//! Pure restart policy, sliding-window rate limiting, and exponential backoff calculations.
//! Corresponds to Go `tempterm/internal/procs/supervisor.go:561-595`.

use std::time::Duration;

/// Default restart window: 60 seconds (1 minute).
pub const DEFAULT_RESTART_WINDOW: Duration = Duration::from_secs(60);

/// Default maximum restart requests in the window: 10.
pub const DEFAULT_MAX_RESTARTS: usize = 10;

/// Default base backoff delay: 1 second.
pub const DEFAULT_RESTART_BASE: Duration = Duration::from_secs(1);

/// Maximum backoff cap: 30 seconds.
pub const MAX_RESTART_DELAY: Duration = Duration::from_secs(30);

/// Outcome of evaluating a process exit against restart policy, rate limit, and backoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartAction {
    /// No restart should occur.
    None,
    /// Restart was paused due to exceeding the sliding-window limit (e.g. 10 restarts in a minute).
    Pause { message: String },
    /// Restart should be scheduled after the given delay with the updated retained timestamps.
    Schedule {
        delay: Duration,
        retained_timestamps: Vec<u64>,
    },
}

/// Evaluates whether a process exit should restart based on policy.
/// Corresponds to Go: `def.Restart == "always" || (def.Restart == "on-failure" && code != 0)`
pub fn should_restart(policy: &str, exit_code: i32) -> bool {
    match policy {
        "always" => true,
        "on-failure" => exit_code != 0,
        _ => false, // "never", empty, or unspecified
    }
}

/// Prunes timestamps strictly older than `window` and appends `now_ms`.
/// In Go: `now.Sub(at) <= window`, so timestamps where `now - at <= window` are retained.
/// A timestamp exactly `window` old remains in the window.
pub fn update_restart_window(timestamps: &[u64], now_ms: u64, window: Duration) -> Vec<u64> {
    let window_ms = window.as_millis() as u64;
    let mut kept = Vec::with_capacity(timestamps.len() + 1);
    for &at in timestamps {
        if now_ms >= at && (now_ms - at) <= window_ms {
            kept.push(at);
        }
    }
    kept.push(now_ms);
    kept
}

/// Calculates the exponential backoff delay based on the number of retained restart attempts.
/// Formula: `min(base * 2^min(count - 1, 5), 30s)`
/// Sequence for base 1s: 1s, 2s, 4s, 8s, 16s, 30s, 30s...
pub fn calculate_backoff(count: usize, base: Duration) -> Duration {
    if count == 0 {
        return base;
    }
    let exponent = (count - 1).min(5) as u32;
    let multiplier = 1u64 << exponent;
    let delay = base.saturating_mul(multiplier as u32);
    if delay > MAX_RESTART_DELAY {
        MAX_RESTART_DELAY
    } else {
        delay
    }
}

/// Formats the restart history marker appended to scrollback.
/// Exact Go format: `\r\n\x1b[2m── restarted (exit N) ──\x1b[0m\r\n`
pub fn format_restart_banner(exit_code: i32) -> Vec<u8> {
    format!("\r\n\x1b[2m── restarted (exit {}) ──\x1b[0m\r\n", exit_code).into_bytes()
}

/// Formats the manual restart history marker.
/// Exact Go format: `\r\n\x1b[2m── restarted ──\x1b[0m\r\n`
pub fn format_manual_restart_banner() -> Vec<u8> {
    b"\r\n\x1b[2m\xe2\x94\x80\xe2\x94\x80 restarted \xe2\x94\x80\xe2\x94\x80\x1b[0m\r\n".to_vec()
}

/// Evaluates the complete restart decision tree for an eligible exit.
/// Evaluates sliding window, rate limit pause, and exponential backoff.
pub fn evaluate_restart(
    policy: &str,
    exit_code: i32,
    existing_timestamps: &[u64],
    now_ms: u64,
    window: Duration,
    max_restarts: usize,
    base: Duration,
) -> (RestartAction, Vec<u64>) {
    if !should_restart(policy, exit_code) {
        return (RestartAction::None, existing_timestamps.to_vec());
    }

    let kept = update_restart_window(existing_timestamps, now_ms, window);
    let count = kept.len();

    if count >= max_restarts {
        let msg = format!("restart paused after {} restarts in a minute", max_restarts);
        return (RestartAction::Pause { message: msg }, kept);
    }

    let delay = calculate_backoff(count, base);
    (
        RestartAction::Schedule {
            delay,
            retained_timestamps: kept.clone(),
        },
        kept,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_restart_policy() {
        assert!(!should_restart("", 0));
        assert!(!should_restart("", 1));
        assert!(!should_restart("never", 0));
        assert!(!should_restart("never", 1));

        assert!(!should_restart("on-failure", 0));
        assert!(should_restart("on-failure", 1));
        assert!(should_restart("on-failure", -1));
        assert!(should_restart("on-failure", 137));

        assert!(should_restart("always", 0));
        assert!(should_restart("always", 1));
        assert!(should_restart("always", -1));
    }

    #[test]
    fn test_backoff_sequence() {
        let base = Duration::from_secs(1);
        assert_eq!(calculate_backoff(1, base), Duration::from_secs(1));
        assert_eq!(calculate_backoff(2, base), Duration::from_secs(2));
        assert_eq!(calculate_backoff(3, base), Duration::from_secs(4));
        assert_eq!(calculate_backoff(4, base), Duration::from_secs(8));
        assert_eq!(calculate_backoff(5, base), Duration::from_secs(16));
        assert_eq!(calculate_backoff(6, base), Duration::from_secs(30)); // 32s capped at 30s
        assert_eq!(calculate_backoff(7, base), Duration::from_secs(30));
        assert_eq!(calculate_backoff(8, base), Duration::from_secs(30));
        assert_eq!(calculate_backoff(9, base), Duration::from_secs(30));
    }

    #[test]
    fn test_sliding_window_60s_boundary() {
        let window = Duration::from_secs(60);
        let now = 100_000u64;

        // Exactly 60s old (now - 60_000 = 40_000)
        let ts_exact_60s = 40_000u64;
        // 60.001s old (now - 60_001 = 39_999)
        let ts_older = 39_999u64;
        // 30s old (now - 30_000 = 70_000)
        let ts_recent = 70_000u64;

        let timestamps = vec![ts_older, ts_exact_60s, ts_recent];
        let kept = update_restart_window(&timestamps, now, window);

        assert_eq!(kept, vec![ts_exact_60s, ts_recent, now]);
        assert_eq!(kept.len(), 3);
    }

    #[test]
    fn test_exact_ninth_and_tenth_boundary() {
        let window = Duration::from_secs(60);
        let base = Duration::from_secs(1);
        let max_restarts = 10;
        let now = 100_000u64;

        // 8 existing recent timestamps
        let history: Vec<u64> = (1..=8).map(|i| now - (10_000 - i * 1_000)).collect();
        assert_eq!(history.len(), 8);

        // 9th restart request
        let (action9, history9) =
            evaluate_restart("on-failure", 1, &history, now, window, max_restarts, base);
        assert_eq!(history9.len(), 9);
        match action9 {
            RestartAction::Schedule {
                delay,
                retained_timestamps,
            } => {
                assert_eq!(delay, Duration::from_secs(30)); // 2^5 = 32 -> capped at 30
                assert_eq!(retained_timestamps.len(), 9);
            }
            _ => panic!("Expected 9th restart to be scheduled"),
        }

        // 10th restart request within window
        let now10 = now + 1_000;
        let (action10, history10) = evaluate_restart(
            "on-failure",
            1,
            &history9,
            now10,
            window,
            max_restarts,
            base,
        );
        assert_eq!(history10.len(), 10);
        match action10 {
            RestartAction::Pause { message } => {
                assert_eq!(message, "restart paused after 10 restarts in a minute");
            }
            _ => panic!("Expected 10th restart to trigger Pause"),
        }
    }

    #[test]
    fn test_format_restart_banners() {
        assert_eq!(
            format_restart_banner(0),
            b"\r\n\x1b[2m\xe2\x94\x80\xe2\x94\x80 restarted (exit 0) \xe2\x94\x80\xe2\x94\x80\x1b[0m\r\n"
        );
        assert_eq!(
            format_restart_banner(1),
            b"\r\n\x1b[2m\xe2\x94\x80\xe2\x94\x80 restarted (exit 1) \xe2\x94\x80\xe2\x94\x80\x1b[0m\r\n"
        );
        assert_eq!(
            format_restart_banner(137),
            b"\r\n\x1b[2m\xe2\x94\x80\xe2\x94\x80 restarted (exit 137) \xe2\x94\x80\xe2\x94\x80\x1b[0m\r\n"
        );
    }
}

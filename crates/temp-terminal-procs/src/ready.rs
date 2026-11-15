use crate::def::Def;
use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Default probe connection timeout per attempt.
/// Corresponds to Go `tempterm/internal/procs/supervisor.go:668` (`100*time.Millisecond`).
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_millis(100);

/// Default interval between readiness checks.
/// Corresponds to Go `tempterm/internal/procs/supervisor.go:146` (`250 * time.Millisecond`).
pub const DEFAULT_CHECK_PERIOD: Duration = Duration::from_millis(250);

/// Default overall readiness deadline.
/// Corresponds to Go `tempterm/internal/procs/supervisor.go:144` (`2 * time.Second`).
pub const DEFAULT_ALIVE_DELAY: Duration = Duration::from_secs(2);

/// Resolves the run port according to Go `tempterm/internal/procs/supervisor.go:712-727`.
///
/// Logic:
/// 1. If `def.port > 0`, return `def.port`.
/// 2. If `def.url.is_empty()`, return 0.
/// 3. If `def.env["PORT"]` parses as 1..65535, return it.
/// 4. Otherwise, allocate an ephemeral free port on loopback. If allocation fails, return 0.
pub fn resolve_run_port(def: &Def) -> u16 {
    if def.port > 0 && def.port < 65536 {
        return def.port as u16;
    }
    if def.url.is_empty() {
        return 0;
    }
    if let Some(val) = def.env.get("PORT") {
        if let Ok(n) = val.parse::<u16>() {
            if n > 0 {
                return n;
            }
        }
        return 0;
    }
    allocate_free_port().unwrap_or(0)
}

/// Allocates an ephemeral loopback TCP port by binding to `127.0.0.1:0`.
/// Corresponds to Go `tempterm/internal/procs/supervisor.go:731-736`.
pub fn allocate_free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|ln| ln.local_addr().ok().map(|addr| addr.port()))
}

/// Returns the loopback socket addresses to probe in exact Go order:
/// 1. `127.0.0.1:<port>` (IPv4 loopback)
/// 2. `[::1]:<port>` (IPv6 loopback)
///
/// Corresponds to Go `tempterm/internal/procs/supervisor.go:665-667`.
pub fn loopback_probe_addrs(port: u16) -> [SocketAddr; 2] {
    [
        SocketAddr::V4(SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, port)),
        SocketAddr::V6(SocketAddrV6::new(std::net::Ipv6Addr::LOCALHOST, port, 0, 0)),
    ]
}

/// Probes whether a process is answering TCP connections on `port` on `127.0.0.1` or `::1`.
/// Checks `127.0.0.1` first, then `::1`.
///
/// Corresponds to Go `tempterm/internal/procs/supervisor.go:664-676`.
pub async fn is_listening(port: u16, connect_timeout: Duration) -> bool {
    if port == 0 {
        return false;
    }

    for addr in loopback_probe_addrs(port) {
        if let Ok(Ok(_stream)) = timeout(connect_timeout, TcpStream::connect(addr)).await {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_resolve_run_port_explicit() {
        let mut def = Def {
            port: 8080,
            ..Default::default()
        };
        assert_eq!(resolve_run_port(&def), 8080);

        def.url = "http://example.com".to_string();
        assert_eq!(resolve_run_port(&def), 8080);
    }

    #[test]
    fn test_resolve_run_port_no_port_no_url() {
        let def = Def::default();
        assert_eq!(resolve_run_port(&def), 0);
    }

    #[test]
    fn test_resolve_run_port_env_override() {
        let mut env = BTreeMap::new();
        env.insert("PORT".to_string(), "3000".to_string());
        let def = Def {
            url: "web".to_string(),
            env,
            ..Default::default()
        };

        assert_eq!(resolve_run_port(&def), 3000);
    }

    #[test]
    fn test_resolve_run_port_invalid_env_returns_zero() {
        let mut env = BTreeMap::new();
        env.insert("PORT".to_string(), "not_a_number".to_string());
        let def = Def {
            url: "web".to_string(),
            env,
            ..Default::default()
        };

        assert_eq!(resolve_run_port(&def), 0);
    }

    #[test]
    fn test_resolve_run_port_free_port_allocation() {
        let def = Def {
            url: "web".to_string(),
            ..Default::default()
        };

        let port = resolve_run_port(&def);
        assert!(port > 0, "expected allocated free port, got 0");
    }

    #[tokio::test]
    async fn test_is_listening_false_on_closed_port() {
        // Port 0 or unused port
        assert!(!is_listening(0, Duration::from_millis(10)).await);
    }

    #[tokio::test]
    async fn test_is_listening_true_on_ipv4_listener() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        assert!(is_listening(port, Duration::from_millis(50)).await);
    }

    #[tokio::test]
    async fn test_is_listening_true_on_ipv6_listener() {
        if let Ok(listener) = tokio::net::TcpListener::bind("[::1]:0").await {
            let port = listener.local_addr().unwrap().port();
            assert!(is_listening(port, Duration::from_millis(50)).await);
        }
    }
}

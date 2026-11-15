//! Dev URL reverse proxy and local routing for temp-terminal.
//! Corresponds to Go `tempterm/internal/devurl/router.go` and `tempterm/internal/devurl/pages.go`.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

/// Route is a host and the process it is served from.
/// Corresponds to Go `devurl.Route` (`tempterm/internal/devurl/router.go:10-18`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    pub host: String,
    pub project: String,
    pub process: String,
    pub port: u16,
    pub running: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
}

/// Router maps a host to its route. It is replaced as a whole, and safe for concurrent use.
/// Corresponds to Go `devurl.Router` (`tempterm/internal/devurl/router.go:22-26`).
#[derive(Debug, Default)]
pub struct Router {
    inner: RwLock<RouterInner>,
}

#[derive(Debug, Default)]
struct RouterInner {
    routes: HashMap<String, Route>,
    order: Vec<String>,
}

impl Router {
    /// Returns a router with no routes.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(RouterInner::default()),
        }
    }

    /// Sets and replaces all routes. A host used twice goes to the first; the routes
    /// it refuses are returned.
    /// Corresponds to Go `Router.Set` (`tempterm/internal/devurl/router.go:33-49`).
    pub fn set(&self, routes: Vec<Route>) -> Vec<Route> {
        let mut m = HashMap::with_capacity(routes.len());
        let mut order = Vec::with_capacity(routes.len());
        let mut refused = Vec::new();

        for mut rt in routes {
            rt.host = rt.host.to_ascii_lowercase();
            if m.contains_key(&rt.host) {
                refused.push(rt);
                continue;
            }
            order.push(rt.host.clone());
            m.insert(rt.host.clone(), rt);
        }

        let mut inner = self.inner.write().unwrap();
        inner.routes = m;
        inner.order = order;

        refused
    }

    /// Looks up the route for a Host header or a host name; a port in it is left out.
    /// Corresponds to Go `Router.Lookup` (`tempterm/internal/devurl/router.go:53-57`).
    pub fn lookup(&self, host: &str) -> Option<Route> {
        let key = host_only(host);
        let inner = self.inner.read().unwrap();
        inner.routes.get(&key).cloned()
    }

    /// Returns all the routes, in the order they were set.
    /// Corresponds to Go `Router.Routes` (`tempterm/internal/devurl/router.go:60-67`).
    pub fn routes(&self) -> Vec<Route> {
        let inner = self.inner.read().unwrap();
        let mut out = Vec::with_capacity(inner.order.len());
        for h in &inner.order {
            if let Some(rt) = inner.routes.get(h) {
                out.push(rt.clone());
            }
        }
        out
    }
}

/// Normalizes host name to lower case and removes port / trailing dots.
/// Corresponds to Go `devurl.hostOnly` (`tempterm/internal/devurl/router.go:70-75`).
pub fn host_only(host: &str) -> String {
    let mut h = host.trim();
    // Check if bracketed IPv6 with port, e.g. [::1]:80
    if h.starts_with('[') {
        if let Some(end_bracket) = h.find(']') {
            let inner = &h[1..end_bracket];
            return inner.to_ascii_lowercase();
        }
    }

    // Split on last colon if there's only one colon or if formatted as host:port
    if let Some(colon) = h.rfind(':') {
        // If there are no other colons, it's host:port
        if !h[..colon].contains(':') {
            h = &h[..colon];
        }
    }

    h.trim_end_matches('.').to_ascii_lowercase()
}

/// Validates whether a DNS host name is valid for a .localhost dev URL.
/// Corresponds to Go `devurl.hostLabelRE` and check rules.
pub fn is_valid_dev_host(host: &str) -> bool {
    let h = host_only(host);
    if !h.ends_with(".localhost") || h == ".localhost" || h == "localhost" {
        return false;
    }

    let prefix = &h[..h.len() - ".localhost".len()];
    if prefix.is_empty() {
        return false;
    }

    for label in prefix.split('.') {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return false;
        }
        if label.starts_with('-') || label.ends_with('-') {
            return false;
        }
    }

    true
}

/// HTML templates and error pages for devurl reverse proxy.
/// Corresponds to Go `tempterm/internal/devurl/pages.go`.
pub mod pages {
    use super::Route;

    pub const PAGE_STYLE: &str = r#"<style>
:root{color-scheme:light dark;--bg:#fafaf9;--fg:#1c1917;--dim:#78716c;--card:#fff;--line:#e7e5e4;--accent:#b45309}
@media (prefers-color-scheme:dark){:root{--bg:#1c1917;--fg:#f5f5f4;--dim:#a8a29e;--card:#292524;--line:#44403c;--accent:#fbbf24}}
body{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--fg);font:16px/1.5 system-ui,sans-serif}
main{box-sizing:border-box;width:min(32rem,100%);padding:2rem 1.25rem}
h1{font-size:1.4rem;margin:0 0 .5rem}
p{margin:.25rem 0;color:var(--dim)}
code{font:.9em ui-monospace,monospace;color:var(--accent)}
ul{list-style:none;margin:1rem 0 0;padding:0;border:1px solid var(--line);border-radius:.5rem;background:var(--card)}
li{display:flex;justify-content:space-between;gap:1rem;padding:.6rem .9rem;border-top:1px solid var(--line)}
li:first-child{border-top:0}
li span{color:var(--dim)}
li{transition:background-color 120ms}
li:hover{background:color-mix(in srgb,var(--line) 40%,transparent)}
main{animation:rise 360ms cubic-bezier(.2,.8,.3,1) both}
@keyframes rise{from{opacity:0;transform:translateY(8px)}}
@media (prefers-reduced-motion:reduce){main{animation:none}li{transition:none}}
</style>"#;

    pub fn render_not_running(route: &Route) -> String {
        let status = if !route.status.is_empty() {
            &route.status
        } else {
            "stopped"
        };
        format!(
            r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{} isn't running</title>{}</head>
<body><main>
<h1>{} isn't running</h1>
<p>Start it in tempterm, in the project <code>{}</code>.</p>
<p>Status: {}</p>
</main></body></html>"#,
            html_escape(&route.process),
            PAGE_STYLE,
            html_escape(&route.process),
            html_escape(&route.project),
            html_escape(status)
        )
    }

    pub fn render_unknown_host(host: &str, routes: &[Route]) -> String {
        let mut list_html = String::new();
        if routes.is_empty() {
            list_html.push_str(
                "<p>No process has a URL yet: give one a <code>url</code> in tempterm.yml.</p>",
            );
        } else {
            list_html.push_str("<p>These are served by tempterm:</p>\n<ul>");
            for rt in routes {
                list_html.push_str(&format!(
                    "<li><code>{}</code><span>{} in {}</span></li>",
                    html_escape(&rt.host),
                    html_escape(&rt.process),
                    html_escape(&rt.project)
                ));
            }
            list_html.push_str("</ul>");
        }

        format!(
            r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>No process at {}</title>{}</head>
<body><main>
<h1>No process at <code>{}</code></h1>
{}
</main></body></html>"#,
            html_escape(host),
            PAGE_STYLE,
            html_escape(host),
            list_html
        )
    }

    fn html_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }
}

/// HTTP Reverse Proxy Server forwarding requests according to a `Router`.
/// Corresponds to Go `devurl.Server` (`tempterm/internal/devurl/server.go`).
pub mod proxy {
    use super::*;
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::broadcast;

    /// Proxy server listening on an HTTP address and forwarding to registered processes.
    pub struct ProxyServer {
        local_addr: SocketAddr,
        router: Arc<Router>,
        shutdown_tx: broadcast::Sender<()>,
        join_handle: tokio::task::JoinHandle<()>,
    }

    impl ProxyServer {
        /// Starts the proxy listening on `bind_addr`.
        pub async fn bind(bind_addr: &str, router: Arc<Router>) -> std::io::Result<Self> {
            let listener = TcpListener::bind(bind_addr).await?;
            let local_addr = listener.local_addr()?;
            let (shutdown_tx, _) = broadcast::channel(1);

            let router_clone = Arc::clone(&router);
            let mut shutdown_rx = shutdown_tx.subscribe();

            let join_handle = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        accept_res = listener.accept() => {
                            match accept_res {
                                Ok((stream, client_addr)) => {
                                    let router_task = Arc::clone(&router_clone);
                                    tokio::spawn(async move {
                                        let _ = handle_http_connection(stream, client_addr, router_task).await;
                                    });
                                }
                                Err(_) => break,
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            break;
                        }
                    }
                }
            });

            Ok(Self {
                local_addr,
                router,
                shutdown_tx,
                join_handle,
            })
        }

        pub fn addr(&self) -> SocketAddr {
            self.local_addr
        }

        pub fn port(&self) -> u16 {
            self.local_addr.port()
        }

        pub fn router(&self) -> &Arc<Router> {
            &self.router
        }

        pub async fn close(self) {
            let _ = self.shutdown_tx.send(());
            let _ = self.join_handle.await;
        }
    }

    /// Handles a single incoming HTTP connection, parses request headers, routes to target port,
    /// or returns not_running / unknown_host HTML.
    async fn handle_http_connection(
        mut client_stream: TcpStream,
        client_addr: SocketAddr,
        router: Arc<Router>,
    ) -> std::io::Result<()> {
        let mut buf = vec![0u8; 8192];
        let n = client_stream.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }

        let req_bytes = &buf[..n];
        let req_str = String::from_utf8_lossy(req_bytes);

        // Parse HTTP request line and headers
        let mut lines = req_str.split("\r\n");
        let req_line = lines.next().unwrap_or("");
        if req_line.is_empty() {
            return Ok(());
        }

        let mut host_header = None;
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                if k.trim().eq_ignore_ascii_case("host") {
                    host_header = Some(v.trim().to_string());
                }
            }
        }

        let host = host_header.unwrap_or_default();
        let route = router.lookup(&host);

        match route {
            None => {
                let body = pages::render_unknown_host(&host_only(&host), &router.routes());
                let resp = format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                client_stream.write_all(resp.as_bytes()).await?;
            }
            Some(rt) if !rt.running || rt.port == 0 => {
                let body = pages::render_not_running(&rt);
                let resp = format!(
                    "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                client_stream.write_all(resp.as_bytes()).await?;
            }
            Some(rt) => {
                // Forward to backend on 127.0.0.1:port (or [::1]:port if 127.0.0.1 fails)
                let backend_stream = match TcpStream::connect(("127.0.0.1", rt.port)).await {
                    Ok(s) => Ok(s),
                    Err(_) => TcpStream::connect(("::1", rt.port)).await,
                };

                let mut backend_stream = match backend_stream {
                    Ok(s) => s,
                    Err(_) => {
                        let mut failed_rt = rt.clone();
                        failed_rt.running = false;
                        failed_rt.status = "not answering".to_string();
                        let body = pages::render_not_running(&failed_rt);
                        let resp = format!(
                            "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        client_stream.write_all(resp.as_bytes()).await?;
                        return Ok(());
                    }
                };

                // Rewrite headers: add X-Forwarded-For, X-Forwarded-Proto, X-Forwarded-Host
                let rewritten_req = rewrite_request(req_bytes, &host, client_addr);
                backend_stream.write_all(&rewritten_req).await?;

                // Bidirectional proxy copy (supports HTTP/1.1 streaming and WebSocket upgrades)
                let (mut client_read, mut client_write) = client_stream.into_split();
                let (mut backend_read, mut backend_write) = backend_stream.into_split();

                let client_to_backend = tokio::io::copy(&mut client_read, &mut backend_write);
                let backend_to_client = tokio::io::copy(&mut backend_read, &mut client_write);

                let _ = tokio::try_join!(client_to_backend, backend_to_client);
            }
        }

        Ok(())
    }

    /// Rewrites the initial HTTP request buffer adding standard X-Forwarded headers
    fn rewrite_request(original: &[u8], host: &str, client_addr: SocketAddr) -> Vec<u8> {
        let text = String::from_utf8_lossy(original);
        if let Some(pos) = text.find("\r\n\r\n") {
            let headers_part = &text[..pos];
            let body_part = &original[pos + 4..];

            let mut out = String::new();
            let mut lines = headers_part.split("\r\n");
            if let Some(first_line) = lines.next() {
                out.push_str(first_line);
                out.push_str("\r\n");
            }

            for line in lines {
                if line.is_empty() {
                    continue;
                }
                out.push_str(line);
                out.push_str("\r\n");
            }

            out.push_str("X-Forwarded-Proto: https\r\n");
            out.push_str(&format!("X-Forwarded-Host: {}\r\n", host));
            out.push_str(&format!("X-Forwarded-For: {}\r\n", client_addr.ip()));
            out.push_str("\r\n");

            let mut res = out.into_bytes();
            res.extend_from_slice(body_part);
            res
        } else {
            original.to_vec()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_only_parity_with_go() {
        assert_eq!(host_only("A.localhost"), "a.localhost");
        assert_eq!(host_only("a.localhost:443"), "a.localhost");
        assert_eq!(host_only("a.localhost."), "a.localhost");
        assert_eq!(host_only("[::1]:80"), "::1");
    }

    #[test]
    fn test_router_first_of_a_host_routes_parity() {
        let r = Router::new();
        let refused = r.set(vec![
            Route {
                host: "shop.localhost".to_string(),
                project: "x".to_string(),
                process: "web".to_string(),
                port: 1,
                running: true,
                status: String::new(),
            },
            Route {
                host: "api.localhost".to_string(),
                project: "x".to_string(),
                process: "api".to_string(),
                port: 2,
                running: true,
                status: String::new(),
            },
            Route {
                host: "Shop.localhost".to_string(),
                project: "y".to_string(),
                process: "web".to_string(),
                port: 3,
                running: true,
                status: String::new(),
            },
        ]);

        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].project, "y");

        let (rt, ok) = match r.lookup("shop.localhost:8443") {
            Some(rt) => (rt, true),
            None => panic!("expected route found"),
        };
        assert!(ok);
        assert_eq!(rt.process, "web");
        assert_eq!(rt.project, "x");

        assert!(r.lookup("nope.localhost").is_none());
        assert_eq!(r.routes().len(), 2);
    }

    #[test]
    fn test_router_set_replaces_all_parity() {
        let r = Router::new();
        r.set(vec![Route {
            host: "a.localhost".to_string(),
            project: "p".to_string(),
            process: "p".to_string(),
            port: 10,
            running: true,
            status: String::new(),
        }]);
        r.set(vec![Route {
            host: "b.localhost".to_string(),
            project: "p".to_string(),
            process: "p".to_string(),
            port: 20,
            running: true,
            status: String::new(),
        }]);

        assert!(r.lookup("a.localhost").is_none());
        assert_eq!(r.routes().len(), 1);
        assert_eq!(r.routes()[0].host, "b.localhost");
    }

    #[test]
    fn test_is_valid_dev_host() {
        assert!(is_valid_dev_host("shop.localhost"));
        assert!(is_valid_dev_host("api.shop.localhost"));
        assert!(is_valid_dev_host("SHOP.localhost"));

        assert!(!is_valid_dev_host("localhost"));
        assert!(!is_valid_dev_host(".localhost"));
        assert!(!is_valid_dev_host("example.com"));
        assert!(!is_valid_dev_host("a..localhost"));
        assert!(!is_valid_dev_host("x_y.localhost"));
    }
}

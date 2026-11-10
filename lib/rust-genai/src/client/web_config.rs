// MODIFIED by the Phosphor fork relative to upstream genai.
// Notice required by Apache-2.0 section 4(b); see lib/rust-genai/CHANGES-PHOSPHOR.md
// for what changed here and why. Original: https://github.com/jeremychone/rust-genai

use std::time::Duration;

/// Reqwest client configuration.
///
/// By default enables performance optimizations:
/// - TCP_NODELAY (lower latency, disables Nagle's algorithm)
/// - HTTP/2 keep-alive (prevents idle connection drops)
/// - HTTP/2 adaptive flow-control window
/// - Connection pool: 4 idle connections per host
///
/// **Note (Zap fork)**: gzip default is **false**. Upstream genai defaults
/// to `true`, but for AI streaming endpoints (Anthropic / OpenAI compatible)
/// `Accept-Encoding: gzip` causes proxies (nginx with `gzip on; gzip_proxied any;`)
/// to compress SSE responses. SSE over gzip forces the server to flush full
/// deflate frames before the client can decode any text → stream becomes ~K-byte
/// bursts every ~400ms instead of token-level streaming. zed/opencode don't
/// negotiate gzip on SSE so they don't hit this. Bandwidth impact of disabling
/// gzip on SSE is negligible (per-line small JSON is poorly compressible) but
/// the streaming UX improvement is dramatic.
#[derive(Debug, Clone)]
pub struct WebConfig {
	pub timeout: Option<Duration>,
	pub connect_timeout: Option<Duration>,
	pub read_timeout: Option<Duration>,
	pub default_headers: Option<reqwest::header::HeaderMap>,
	pub proxy: Option<reqwest::Proxy>,
	/// When true, disable automatic proxy discovery (system proxy, env vars).
	/// Calls `reqwest::ClientBuilder::no_proxy()`. If an explicit `proxy` is
	/// also set on this config, the explicit proxy still takes effect.
	/// Default: false.
	pub no_proxy: bool,
	/// Enable gzip response decompression. **Default: false** (Zap fork
	/// — upstream genai default is true). See struct-level docs for rationale.
	pub gzip: bool,
	/// Enable TCP_NODELAY (disable Nagle's algorithm). Default: true.
	pub tcp_nodelay: bool,
	/// Phosphor: decides whether each redirect hop may be followed (the chain's first URL, the
	/// hop's `url`, hops so far). `None` keeps reqwest's default policy. A `false` answer stops
	/// the chain at that hop.
	pub redirect_guard: Option<RedirectGuard>,
}

/// Phosphor: a redirect decision function (see [`WebConfig::redirect_guard`]). A newtype so
/// `WebConfig` keeps `Debug` and `Clone`.
#[derive(Clone)]
pub struct RedirectGuard(pub std::sync::Arc<dyn Fn(&str, &str, usize) -> bool + Send + Sync>);

impl RedirectGuard {
	pub fn new(f: impl Fn(&str, &str, usize) -> bool + Send + Sync + 'static) -> Self {
		Self(std::sync::Arc::new(f))
	}
}

impl std::fmt::Debug for RedirectGuard {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("RedirectGuard")
	}
}

impl Default for WebConfig {
	fn default() -> Self {
		Self {
			timeout: None,
			connect_timeout: None,
			read_timeout: None,
			default_headers: None,
			proxy: None,
			no_proxy: false,
			// Zap: gzip off by default — see struct-level docs above.
			gzip: false,
			tcp_nodelay: true,
			redirect_guard: None,
		}
	}
}

impl WebConfig {
	/// Sets the per-request timeout.
	pub fn with_timeout(mut self, timeout: Duration) -> Self {
		self.timeout = Some(timeout);
		self
	}

	/// Sets the connect timeout.
	pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
		self.connect_timeout = Some(timeout);
		self
	}

	/// Sets default headers.
	pub fn with_default_headers(mut self, headers: reqwest::header::HeaderMap) -> Self {
		self.default_headers = Some(headers);
		self
	}

	/// Sets the proxy.
	pub fn with_proxy(mut self, proxy: reqwest::Proxy) -> Self {
		self.proxy = Some(proxy);
		self
	}

	/// Sets the HTTP proxy from a URL.
	pub fn with_proxy_url(mut self, proxy_url: &str) -> Result<Self, reqwest::Error> {
		let proxy = reqwest::Proxy::http(proxy_url)?;
		self.proxy = Some(proxy);
		Ok(self)
	}

	/// Sets the HTTPS proxy from a URL.
	pub fn with_https_proxy_url(mut self, proxy_url: &str) -> Result<Self, reqwest::Error> {
		let proxy = reqwest::Proxy::https(proxy_url)?;
		self.proxy = Some(proxy);
		Ok(self)
	}

	/// Sets the proxy for all schemes from a URL.
	pub fn with_all_proxy_url(mut self, proxy_url: &str) -> Result<Self, reqwest::Error> {
		let proxy = reqwest::Proxy::all(proxy_url)?;
		self.proxy = Some(proxy);
		Ok(self)
	}

	/// Sets proxy from a URL with optional credentials and no-proxy host list.
	///
	/// Called from application code where the workspace reqwest version differs from
	/// this library's reqwest, so all proxy construction is done here within this library.
	pub fn set_proxy_settings(
		&mut self,
		url: &str,
		username: &str,
		password: &str,
		no_proxy: &str,
	) -> Result<(), reqwest::Error> {
		let mut proxy = reqwest::Proxy::all(url)?;
		if !username.is_empty() || !password.is_empty() {
			proxy = proxy.basic_auth(username, password);
		}
		if !no_proxy.trim().is_empty() {
			if let Some(no_proxy_val) = reqwest::NoProxy::from_string(no_proxy.trim()) {
				proxy = proxy.no_proxy(Some(no_proxy_val));
			}
		}
		self.proxy = Some(proxy);
		Ok(())
	}

	/// Applies this config to a reqwest::ClientBuilder.
	pub fn apply_to_builder(&self, mut builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
		if let Some(timeout) = self.timeout {
			builder = builder.timeout(timeout);
		}
		if let Some(connect_timeout) = self.connect_timeout {
			builder = builder.connect_timeout(connect_timeout);
		}
		if let Some(read_timeout) = self.read_timeout {
			builder = builder.read_timeout(read_timeout);
		}
		if let Some(ref headers) = self.default_headers {
			builder = builder.default_headers(headers.clone());
		}
		if self.no_proxy {
			builder = builder.no_proxy();
		}
		if let Some(ref proxy) = self.proxy {
			builder = builder.proxy(proxy.clone());
		}
		if let Some(guard) = &self.redirect_guard {
			let guard = guard.0.clone();
			builder = builder.redirect(reqwest::redirect::Policy::custom(move |attempt| {
				// The first URL is what the request's credentials were meant for; reqwest strips
				// only `Authorization`/`Cookie` across hosts, so the guard gets it to decide.
				let previous = attempt.previous();
				let first = previous.first().map_or(attempt.url().as_str(), |u| u.as_str());
				if guard(first, attempt.url().as_str(), previous.len()) {
					attempt.follow()
				} else {
					attempt.stop()
				}
			}));
		}
		// Performance optimizations
		if self.gzip {
			builder = builder.gzip(true);
		}
		if self.tcp_nodelay {
			builder = builder.tcp_nodelay(true);
		}
		// HTTP/2 connection tuning
		builder = builder
			.pool_max_idle_per_host(4)
			.http2_keep_alive_interval(Some(Duration::from_secs(20)))
			.http2_keep_alive_timeout(Duration::from_secs(10))
			.http2_keep_alive_while_idle(true)
			.http2_adaptive_window(true);
		builder
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn set_proxy_settings_valid_http_url() {
		let mut cfg = WebConfig::default();
		assert!(cfg.set_proxy_settings("http://proxy.corp:8080", "", "", "").is_ok());
		assert!(cfg.proxy.is_some());
	}

	#[test]
	fn set_proxy_settings_socks5_url() {
		let mut cfg = WebConfig::default();
		assert!(cfg.set_proxy_settings("socks5://127.0.0.1:1080", "", "", "").is_ok());
		assert!(cfg.proxy.is_some());
	}

	#[test]
	fn set_proxy_settings_invalid_url_returns_err() {
		let mut cfg = WebConfig::default();
		// Malformed IPv6 bracket causes reqwest::Proxy::all to return Err.
		assert!(cfg.set_proxy_settings("http://[invalid", "", "", "").is_err());
		assert!(cfg.proxy.is_none());
	}

	// Phosphor #1928: the guard is told where the chain began, and a hop it refuses is never
	// requested (the caller gets the 3xx). Server A answers 307 to server B, another origin; B
	// must see no connection, so a key header on the request cannot reach it.
	#[tokio::test]
	async fn redirect_guard_gets_the_first_url_and_a_refused_hop_is_not_requested() {
		use std::io::{Read, Write};
		use std::sync::{Arc, Mutex};

		let a = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
		let b = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
		let (a_addr, b_addr) = (a.local_addr().unwrap(), b.local_addr().unwrap());
		b.set_nonblocking(true).unwrap();
		let server = std::thread::spawn(move || {
			let (mut conn, _) = a.accept().unwrap();
			let mut request = Vec::new();
			let mut buf = [0u8; 1024];
			while !request.windows(4).any(|w| w == b"\r\n\r\n") {
				let n = conn.read(&mut buf).unwrap();
				if n == 0 {
					break;
				}
				request.extend_from_slice(&buf[..n]);
			}
			let reply = format!(
				"HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{b_addr}/next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
			);
			conn.write_all(reply.as_bytes()).unwrap();
		});

		let seen = Arc::new(Mutex::new(Vec::new()));
		let sink = seen.clone();
		let cfg = WebConfig {
			no_proxy: true,
			redirect_guard: Some(RedirectGuard::new(move |first, url, hops| {
				sink.lock().unwrap().push((first.to_owned(), url.to_owned(), hops));
				false
			})),
			..WebConfig::default()
		};
		let client = cfg.apply_to_builder(reqwest::Client::builder()).build().unwrap();
		let response = client
			.get(format!("http://{a_addr}/start"))
			.header("x-api-key", "not-a-real-key")
			.send()
			.await
			.unwrap();
		server.join().unwrap();

		assert_eq!(response.status().as_u16(), 307);
		assert_eq!(
			*seen.lock().unwrap(),
			vec![(format!("http://{a_addr}/start"), format!("http://{b_addr}/next"), 1)]
		);
		assert!(b.accept().is_err(), "the refused hop's server was never connected to");
	}
}

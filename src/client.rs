//! Tiny HTTP/1.1 client for the CLI commands that talk to a running `r3dsims live` (send, hooks,
//! overlay, control, state). Plain `http://` only: put a TLS proxy in front for anything public.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

pub struct Client {
    host: String,
    token: Option<String>,
}

impl Client {
    pub fn new(server: &str, token: Option<String>) -> Result<Client> {
        let s = server.trim().trim_end_matches('/');
        let host = match s.split_once("://") {
            Some(("http", h)) => h,
            Some((other, _)) => bail!("--server must be plain http:// (got {other}://); use a TLS-terminating proxy for https"),
            None => s,
        };
        if host.is_empty() {
            bail!("--server is empty");
        }
        let host = if host.contains(':') { host.to_string() } else { format!("{host}:80") };
        Ok(Client { host, token: token.filter(|t| !t.is_empty()) })
    }

    /// Returns (status, body as text; binary bodies are converted lossily, use `request_bytes`).
    pub fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<(u16, String)> {
        let (code, bytes) = self.request_bytes(method, path, body)?;
        Ok((code, String::from_utf8_lossy(&bytes).into_owned()))
    }

    /// Returns (status, raw body bytes).
    pub fn request_bytes(&self, method: &str, path: &str, body: Option<&str>) -> Result<(u16, Vec<u8>)> {
        let addr = self.host.to_socket_addrs().with_context(|| format!("resolve {}", self.host))?.next().ok_or_else(|| anyhow!("no address for {}", self.host))?;
        let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).with_context(|| format!("connect to {} (is `r3dsims live` running there?)", self.host))?;
        s.set_read_timeout(Some(Duration::from_secs(15)))?;
        let body = body.unwrap_or("");
        let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/json\r\n", self.host);
        if let Some(t) = &self.token {
            req.push_str(&format!("Authorization: Bearer {t}\r\n"));
        }
        if method == "POST" {
            req.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n", body.len()));
        }
        req.push_str("\r\n");
        s.write_all(req.as_bytes())?;
        s.write_all(body.as_bytes())?;
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).context("read response")?;
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| anyhow!("malformed HTTP response"))?;
        let head = String::from_utf8_lossy(&raw[..split]).into_owned();
        let rest = &raw[split + 4..];
        let status: u16 = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|c| c.parse().ok()).ok_or_else(|| anyhow!("malformed status line"))?;
        let chunked = head.lines().any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding") && l.to_ascii_lowercase().contains("chunked"));
        let body = if chunked { dechunk(rest) } else { rest.to_vec() };
        Ok((status, body))
    }

    /// POST and print the answer; non-2xx becomes an error carrying the server's message.
    pub fn post(&self, path: &str, body: &str) -> Result<String> {
        let (code, text) = self.request("POST", path, Some(body))?;
        if !(200..300).contains(&code) {
            bail!("server answered {code}: {}", text.trim());
        }
        Ok(text)
    }

    pub fn get(&self, path: &str) -> Result<String> {
        let (code, text) = self.request("GET", path, None)?;
        if !(200..300).contains(&code) {
            bail!("server answered {code}: {}", text.trim());
        }
        Ok(text)
    }
}

fn dechunk(mut rest: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(nl) = rest.windows(2).position(|w| w == b"\r\n") {
        let size_line = String::from_utf8_lossy(&rest[..nl]).into_owned();
        let size = usize::from_str_radix(size_line.trim().split(';').next().unwrap_or("0"), 16).unwrap_or(0);
        let tail = &rest[nl + 2..];
        if size == 0 || tail.len() < size {
            break;
        }
        out.extend_from_slice(&tail[..size]);
        rest = tail[size..].strip_prefix(b"\r\n").unwrap_or(&tail[size..]);
    }
    out
}

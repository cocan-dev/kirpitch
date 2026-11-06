// SPDX-License-Identifier: GPL-3.0-or-later

//! How requests reach a server.
//!
//! The real transport runs the system `curl` program, so Kirpitch needs no
//! TLS library of its own. The request body goes through a temporary file and
//! the address and headers (which hold the API key) go to curl's standard
//! input as a config file, so the key never appears in the process list.
//! Tests use [`MockTransport`].

use std::collections::VecDeque;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

/// A POST request with a JSON body.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The address.
    pub url: String,
    /// Headers other than `Content-Type`, which is always `application/json`.
    pub headers: Vec<(String, String)>,
    /// The JSON text.
    pub body: String,
}

/// What the server answered.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    /// HTTP status code.
    pub status: u16,
    /// The body.
    pub body: String,
}

/// Why a request failed before an answer came back.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TransportError {
    /// `curl` could not be started.
    #[error("cannot run curl ({0}); install curl or put it on the PATH")]
    NoCurl(String),
    /// curl ran but the request failed (no connection, timeout, ...).
    #[error("{0}")]
    Failed(String),
    /// A header or address contains a line break.
    #[error("the request has a line break in its address or headers")]
    BadRequest,
}

/// Sends requests.
pub trait Transport: Send + Sync {
    /// Sends `request` and waits at most `timeout_secs`.
    fn post(&self, request: &Request, timeout_secs: u64) -> Result<Reply, TransportError>;
}

/// Sends requests with the system `curl`.
#[derive(Debug, Default)]
pub struct CurlTransport;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Quotes a value for a curl config file.
fn quote(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl Transport for CurlTransport {
    fn post(&self, request: &Request, timeout_secs: u64) -> Result<Reply, TransportError> {
        let has_break = |s: &str| s.contains('\n') || s.contains('\r');
        if has_break(&request.url)
            || request
                .headers
                .iter()
                .any(|(k, v)| has_break(k) || has_break(v))
        {
            return Err(TransportError::BadRequest);
        }
        let file = std::env::temp_dir().join(format!(
            "kirpitch-request-{}-{}.json",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&file, request.body.as_bytes())
            .map_err(|e| TransportError::Failed(format!("{}: {e}", file.display())))?;
        let result = run_curl(request, &file, timeout_secs);
        let _ = std::fs::remove_file(&file);
        result
    }
}

fn run_curl(
    request: &Request,
    body_file: &std::path::Path,
    timeout_secs: u64,
) -> Result<Reply, TransportError> {
    let mut config = String::new();
    config.push_str(&format!("url = {}\n", quote(&request.url)));
    config.push_str("request = \"POST\"\n");
    config.push_str("header = \"Content-Type: application/json\"\n");
    for (k, v) in &request.headers {
        config.push_str(&format!("header = {}\n", quote(&format!("{k}: {v}"))));
    }
    config.push_str(&format!(
        "data-binary = {}\n",
        quote(&format!("@{}", body_file.display()))
    ));
    let mut child = Command::new("curl")
        .args(["--silent", "--show-error", "--max-time"])
        .arg(timeout_secs.to_string())
        .args(["--write-out", "\n%{http_code}", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| TransportError::NoCurl(e.to_string()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(config.as_bytes())
            .map_err(|e| TransportError::Failed(e.to_string()))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| TransportError::Failed(e.to_string()))?;
    if !out.status.success() {
        let message = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(TransportError::Failed(if message.is_empty() {
            format!("curl exited with {:?}", out.status.code())
        } else {
            message
        }));
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, code) = text
        .rsplit_once('\n')
        .ok_or_else(|| TransportError::Failed("curl gave no status code".into()))?;
    let status = code
        .trim()
        .parse::<u16>()
        .map_err(|_| TransportError::Failed(format!("curl gave the status `{code}`")))?;
    Ok(Reply {
        status,
        body: body.to_string(),
    })
}

/// A transport for tests: answers from a list and remembers the requests.
#[derive(Debug, Default)]
pub struct MockTransport {
    replies: Mutex<VecDeque<Result<Reply, TransportError>>>,
    requests: Mutex<Vec<Request>>,
}

impl MockTransport {
    /// A mock with no answers yet.
    pub fn new() -> MockTransport {
        MockTransport::default()
    }

    /// Adds an answer with status 200.
    pub fn push_ok(&self, body: &str) {
        self.push(Ok(Reply {
            status: 200,
            body: body.to_string(),
        }));
    }

    /// Adds an answer.
    pub fn push(&self, reply: Result<Reply, TransportError>) {
        self.replies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(reply);
    }

    /// The requests received so far.
    pub fn requests(&self) -> Vec<Request> {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl Transport for MockTransport {
    fn post(&self, request: &Request, _timeout_secs: u64) -> Result<Reply, TransportError> {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(request.clone());
        self.replies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .unwrap_or_else(|| Err(TransportError::Failed("the mock has no answer left".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_values_are_quoted() {
        assert_eq!(quote(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn line_breaks_in_headers_are_refused() {
        let r = Request {
            url: "http://localhost".into(),
            headers: vec![("X".into(), "a\nb: c".into())],
            body: "{}".into(),
        };
        assert_eq!(CurlTransport.post(&r, 1), Err(TransportError::BadRequest));
    }

    #[test]
    fn the_mock_answers_in_order_and_records() {
        let m = MockTransport::new();
        m.push_ok("one");
        m.push_ok("two");
        let r = Request {
            url: "u".into(),
            headers: vec![],
            body: "{}".into(),
        };
        assert_eq!(m.post(&r, 1).unwrap().body, "one");
        assert_eq!(m.post(&r, 1).unwrap().body, "two");
        assert!(m.post(&r, 1).is_err());
        assert_eq!(m.requests().len(), 3);
    }
}

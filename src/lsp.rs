//! A minimal Language Server Protocol client: JSON-RPC over the server's stdio, synchronous
//! requests, and just enough of the server-to-client traffic (progress, configuration,
//! capability registration) to keep a server happy. Positions are 0-based like LSP's own;
//! columns are UTF-16 units.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

pub struct Client {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    next_id: u64,
    open_progress: usize, // progress tokens begun and not yet ended
    last_progress: Instant,
    quiescent: Option<bool>, // rust-analyzer's own "done indexing" flag, when it sends one
}

/// Full path of `name` on PATH. On Windows also tries PATHEXT, since npm installs `.cmd` shims.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT".into()).split(';').map(str::to_lowercase).collect()
    } else {
        Vec::new()
    };
    for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
        for ext in std::iter::once(String::new()).chain(exts.iter().cloned()) {
            let p = dir.join(format!("{name}{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

pub fn to_uri(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file:///");
    for b in s.trim_start_matches('/').bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The absolute path a `file:` URI names.
pub fn uri_path(uri: &str) -> Option<PathBuf> {
    let d = percent_decode(uri.strip_prefix("file://")?);
    Some(PathBuf::from(if cfg!(windows) { d.trim_start_matches('/').to_owned() } else { d }))
}

/// `path` relative to `root` (forward slashes), or None when it lies outside. Both absolute.
/// Drive letters compare case-insensitively on Windows.
pub fn relative(path: &Path, root: &Path) -> Option<String> {
    let p = path.to_string_lossy().replace('\\', "/");
    let root = root.to_string_lossy().replace('\\', "/");
    let (a, b) = if cfg!(windows) { (p.to_ascii_lowercase(), root.to_ascii_lowercase()) } else { (p.clone(), root) };
    let tail = a.strip_prefix(&b)?.strip_prefix('/')?;
    Some(p[p.len() - tail.len()..].to_owned())
}

fn read_message(r: &mut impl BufRead) -> Option<Value> {
    let mut len = 0;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            if len > 0 {
                break;
            }
            continue;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            len = v.trim().parse().ok()?;
        }
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

impl Client {
    /// Spawns `exe args` with `root` as the workspace and completes the initialize handshake.
    pub fn start(exe: &Path, args: &[&str], root: &Path) -> Option<Client> {
        let mut child = Command::new(exe).args(args).current_dir(root).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
        let stdin = child.stdin.take()?;
        let stdout = child.stdout.take()?;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            while let Some(msg) = read_message(&mut r) {
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        let mut c = Client { child, stdin, rx, next_id: 0, open_progress: 0, last_progress: Instant::now(), quiescent: None };
        let uri = to_uri(root);
        let params = json!({
            "processId": std::process::id(),
            "rootUri": uri,
            "workspaceFolders": [{"uri": uri, "name": root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()}],
            "capabilities": {
                "textDocument": {
                    "documentSymbol": {"hierarchicalDocumentSymbolSupport": true},
                    "callHierarchy": {},
                    "references": {},
                    "hover": {"contentFormat": ["markdown", "plaintext"]},
                    "definition": {}
                },
                "window": {"workDoneProgress": true},
                "workspace": {"configuration": true},
                "experimental": {"serverStatusNotification": true}
            },
            // rust-analyzer computes what it is asked when it is asked instead of priming every
            // crate first, so a large workspace is ready once it is loaded
            "initializationOptions": {"checkOnSave": false, "cachePriming": {"enable": false}}
        });
        c.request("initialize", params).ok()?;
        c.notify("initialized", json!({}));
        Some(c)
    }

    fn send(&mut self, msg: Value) {
        let s = msg.to_string();
        let _ = write!(self.stdin, "Content-Length: {}\r\n\r\n{s}", s.len());
        let _ = self.stdin.flush();
    }

    pub fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    fn reply(&mut self, id: Value, result: Value) {
        self.send(json!({"jsonrpc": "2.0", "id": id, "result": result}));
    }

    /// Sends a request and blocks for its response, servicing whatever else arrives meanwhile.
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let mut msg = self.rx.recv_timeout(Duration::from_secs(120)).map_err(|_| "server went away".to_string())?;
            if msg["id"] == id && msg.get("method").is_none() {
                if let Some(e) = msg.get("error") {
                    return Err(e["message"].as_str().unwrap_or("error").to_owned());
                }
                return Ok(msg["result"].take());
            }
            self.handle(msg);
        }
    }

    /// Sends every request with up to `WINDOW` in flight, so the server answers them on its
    /// own threads, and returns the answers in the order asked. A request the server does not
    /// answer within two minutes of the one before it is an Err, and so is every one after.
    pub fn request_all(&mut self, reqs: Vec<(&str, Value)>) -> Vec<Result<Value, String>> {
        const WINDOW: usize = 64;
        let n = reqs.len();
        let first = self.next_id + 1;
        let mut out: Vec<Option<Result<Value, String>>> = (0..n).map(|_| None).collect();
        let mut reqs = reqs.into_iter();
        let (mut sent, mut got) = (0, 0);
        while got < n {
            while sent < n && sent - got < WINDOW {
                let (method, params) = reqs.next().unwrap();
                self.next_id += 1;
                let id = self.next_id;
                self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
                sent += 1;
            }
            let Ok(mut msg) = self.rx.recv_timeout(Duration::from_secs(120)) else { break };
            if msg.get("method").is_none() {
                if let Some(i) = msg["id"].as_u64().and_then(|id| id.checked_sub(first)).map(|i| i as usize).filter(|&i| i < n && out[i].is_none()) {
                    out[i] = Some(match msg.get("error") {
                        Some(e) => Err(e["message"].as_str().unwrap_or("error").to_owned()),
                        None => Ok(msg["result"].take()),
                    });
                    got += 1;
                    continue;
                }
            }
            self.handle(msg);
        }
        out.into_iter().map(|r| r.unwrap_or_else(|| Err("server went away".into()))).collect()
    }

    /// Notifications and server-to-client requests.
    fn handle(&mut self, msg: Value) {
        match msg["method"].as_str().unwrap_or("") {
            "$/progress" => {
                match msg["params"]["value"]["kind"].as_str() {
                    Some("begin") => self.open_progress += 1,
                    Some("end") => self.open_progress = self.open_progress.saturating_sub(1),
                    _ => {}
                }
                self.last_progress = Instant::now();
            }
            "experimental/serverStatus" => self.quiescent = msg["params"]["quiescent"].as_bool(),
            "workspace/configuration" => {
                let n = msg["params"]["items"].as_array().map_or(0, Vec::len);
                self.reply(msg["id"].clone(), Value::Array(vec![Value::Null; n]));
            }
            _ if msg.get("id").is_some() => self.reply(msg["id"].clone(), Value::Null),
            _ => {}
        }
    }

    /// Blocks until the server has finished its own indexing: it says so, or every progress it
    /// began has ended and nothing new arrived for half a second. A server that reports no
    /// progress at all is assumed ready after three seconds. Gives up after `max`.
    pub fn wait_ready(&mut self, max: Duration) {
        let start = Instant::now();
        let mut seen_progress = false;
        loop {
            match self.rx.recv_timeout(Duration::from_millis(200)) {
                Ok(msg) => self.handle(msg),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            seen_progress |= self.open_progress > 0;
            let quiet = self.open_progress == 0 && self.last_progress.elapsed() > Duration::from_millis(500);
            let done = match self.quiescent {
                Some(q) => q && quiet,
                None => quiet && (seen_progress || start.elapsed() > Duration::from_secs(3)),
            };
            if done || start.elapsed() > max {
                return;
            }
        }
    }

    fn at(path: &Path, line: u32, col: u32) -> Value {
        json!({"textDocument": {"uri": to_uri(path)}, "position": {"line": line, "character": col}})
    }

    /// What the server knows about the thing at a position, as plain lines: the markdown's
    /// code fences are dropped and blank lines never repeat, everything else is kept. None
    /// when it knows nothing.
    pub fn hover(&mut self, path: &Path, line: u32, col: u32) -> Option<String> {
        let v = self.request("textDocument/hover", Self::at(path, line, col)).ok()?;
        let mut out = String::new();
        let mut push = |s: &str| {
            for l in s.lines().filter(|l| !l.trim_start().starts_with("```")) {
                if l.trim().is_empty() && (out.is_empty() || out.ends_with("\n\n")) {
                    continue;
                }
                out.push_str(l);
                out.push('\n');
            }
        };
        match &v["contents"] {
            Value::Array(a) => a.iter().for_each(|x| push(x.as_str().or_else(|| x["value"].as_str()).unwrap_or(""))),
            Value::String(s) => push(s),
            o => push(o["value"].as_str().unwrap_or("")),
        }
        let out = out.trim().to_owned();
        (!out.is_empty()).then_some(out)
    }

    /// Where the thing at a position is defined: the first location the server names, as an
    /// absolute path and 0-based line and column.
    pub fn definition(&mut self, path: &Path, line: u32, col: u32) -> Option<(PathBuf, u32, u32)> {
        let v = self.request("textDocument/definition", Self::at(path, line, col)).ok()?;
        let l = if let Some(a) = v.as_array() { a.first()?.clone() } else { v };
        let (uri, range) = if l.get("targetUri").is_some() { (&l["targetUri"], &l["targetSelectionRange"]) } else { (&l["uri"], &l["range"]) };
        Some((uri_path(uri.as_str()?)?, range["start"]["line"].as_u64()? as u32, range["start"]["character"].as_u64()? as u32))
    }

    pub fn shutdown(mut self) {
        let _ = self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        let _ = self.child.wait();
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip() {
        let root = if cfg!(windows) { Path::new("C:\\Users\\me\\my repo") } else { Path::new("/home/me/my repo") };
        let uri = to_uri(&root.join("src").join("a.rs"));
        assert!(uri.ends_with("/my%20repo/src/a.rs"), "{uri}");
        let rel = |uri: &str| -> Option<String> { relative(&uri_path(uri)?, root) };
        assert_eq!(rel(&uri).as_deref(), Some("src/a.rs"));
        assert_eq!(rel(&uri.to_ascii_lowercase()).is_some(), cfg!(windows));
        assert_eq!(rel("file:///elsewhere/x.rs"), None);
    }

    /// Needs rust-analyzer on PATH: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn rust_analyzer_answers() {
        let root = std::path::absolute(env!("CARGO_MANIFEST_DIR")).unwrap();
        let exe = find_on_path("rust-analyzer").expect("rust-analyzer on PATH");
        let mut c = Client::start(&exe, &[], &root).expect("start");
        c.wait_ready(Duration::from_secs(120));
        let uri = to_uri(&root.join("src/map.rs"));
        let syms = c.request("textDocument/documentSymbol", json!({"textDocument": {"uri": uri}})).unwrap();
        let names: Vec<&str> = syms.as_array().unwrap().iter().filter_map(|s| s["name"].as_str()).collect();
        assert!(names.contains(&"Map"), "{names:?}");
        c.shutdown();
    }
}

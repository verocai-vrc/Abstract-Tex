//! A stand-in language server that speaks just enough JSON-RPC for the bridge tests.
//!
//! Like `fake-lsp-echo` it uses blocking `std::io` and no dependencies — not even `serde_json`,
//! so a bug in our JSON handling cannot be cancelled out by the same bug here. It reads fields
//! with string scanning, which is fragile in general but fine against the handful of shapes
//! these tests send.
//!
//! Methods it understands:
//!   - `slow`    — replies after 300 ms, so a test can have two requests in flight at once.
//!   - `boom`    — exits with code 9 without replying, to test crash-and-restart.
//!   - `fail`    — replies with a JSON-RPC error object.
//!   - `notify`  — sends an unsolicited notification instead of a reply.
//!   - `flood`   — sends 20 notifications of 16 KB each (about five pipes' worth) before reading
//!     anything else, with blocking writes like TexLab's own writer thread: while nobody reads
//!     them, this process reads nothing either (S9.11).
//!   - anything else — replies `{"echo": "<method>"}`.

use std::io::{Read, Write};

fn main() {
    let mut stdin = std::io::stdin().lock();
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    // How many times this process has been started is invisible to it, but the test can tell
    // restarts apart by the pid the reply carries.
    let pid = std::process::id();
    loop {
        let read = match stdin.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        pending.extend_from_slice(&chunk[..read]);
        while let Some(body) = take_frame(&mut pending) {
            let text = String::from_utf8_lossy(&body).to_string();
            let method = field(&text, "\"method\":\"").unwrap_or_default();
            let id = raw_id(&text);

            match method.as_str() {
                "boom" => std::process::exit(9),
                "notify" => {
                    send(&format!(
                        r#"{{"jsonrpc":"2.0","method":"telemetry/event","params":{{"from":{pid}}}}}"#
                    ));
                }
                "flood" => {
                    let fill = "x".repeat(16 * 1024);
                    for _ in 0..20 {
                        send(&format!(r#"{{"jsonrpc":"2.0","method":"telemetry/event","params":{{"fill":"{fill}"}}}}"#));
                    }
                }
                "exit" => return,
                _ => {
                    let Some(id) = id else { continue }; // a notification: nothing to answer
                    if method == "slow" {
                        std::thread::sleep(std::time::Duration::from_millis(300));
                    }
                    if method == "fail" {
                        send(&format!(
                            r#"{{"jsonrpc":"2.0","id":{id},"error":{{"code":-32000,"message":"as requested"}}}}"#
                        ));
                    } else {
                        send(&format!(
                            r#"{{"jsonrpc":"2.0","id":{id},"result":{{"echo":"{method}","pid":{pid}}}}}"#
                        ));
                    }
                }
            }
        }
    }
}

fn send(body: &str) {
    let mut stdout = std::io::stdout().lock();
    write!(stdout, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    stdout.flush().unwrap();
}

/// The value of a string field, e.g. `field(s, "\"method\":\"")` → `Some("initialize")`.
fn field(text: &str, prefix: &str) -> Option<String> {
    let start = text.find(prefix)? + prefix.len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// The `id` exactly as written, so a number stays a number and a string keeps its quotes.
fn raw_id(text: &str) -> Option<String> {
    let start = text.find("\"id\":")? + 5;
    let rest = &text[start..];
    let end = rest.find([',', '}'])?;
    Some(rest[..end].trim().to_string())
}

fn take_frame(pending: &mut Vec<u8>) -> Option<Vec<u8>> {
    let header_end = pending.windows(4).position(|w| w == b"\r\n\r\n")?;
    let headers = std::str::from_utf8(&pending[..header_end]).ok()?;
    let length: usize = headers
        .split("\r\n")
        .find_map(|line| line.strip_prefix("Content-Length:"))?
        .trim()
        .parse()
        .ok()?;
    let start = header_end + 4;
    if pending.len() < start + length {
        return None;
    }
    let body = pending[start..start + length].to_vec();
    pending.drain(..start + length);
    Some(body)
}

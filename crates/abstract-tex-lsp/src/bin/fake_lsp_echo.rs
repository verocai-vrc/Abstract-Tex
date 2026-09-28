//! A stand-in language server for the process tests: reads LSP frames from stdin and writes
//! each one straight back to stdout, until stdin closes. Deliberately written with blocking
//! `std::io` and no dependencies, so it cannot share a bug with the code under test.

use std::io::{Read, Write};

fn main() {
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = match stdin.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        pending.extend_from_slice(&chunk[..read]);
        while let Some(body) = take_frame(&mut pending) {
            // A frame whose body is the word "die" makes the server exit abruptly, so a test
            // can see what a crash looks like from the other side of the pipe.
            if body == b"die" {
                std::process::exit(3);
            }
            let header = format!("Content-Length: {}\r\n\r\n", body.len());
            stdout.write_all(header.as_bytes()).unwrap();
            stdout.write_all(&body).unwrap();
            stdout.flush().unwrap();
        }
    }
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

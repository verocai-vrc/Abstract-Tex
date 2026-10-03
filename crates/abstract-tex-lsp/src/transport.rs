//! The LSP base protocol: how messages are cut out of a byte stream.
//!
//! Every message on the wire is `Content-Length: N\r\n\r\n` followed by exactly `N` bytes of
//! JSON. (The spec also allows a `Content-Type` header; nobody sends it, and we ignore it.)
//! Owns the framing only — this module never parses the JSON it carries.

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::LspError;

/// Wrap one message body in its header, ready to write to the server's stdin.
pub fn encode(body: &[u8]) -> Vec<u8> {
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body);
    out
}

/// Reads whole frames off an async byte stream. The stream hands us chunks of whatever size
/// the pipe happens to deliver — half a header, three messages at once — so `pending` holds
/// what has arrived and `next` returns a message only when all of it is there.
pub struct FrameReader<R> {
    source: R,
    pending: Vec<u8>,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub fn new(source: R) -> Self {
        Self {
            source,
            pending: Vec::with_capacity(8 * 1024),
        }
    }

    /// The next message body, or `Ok(None)` once the stream has ended cleanly. A stream that
    /// ends *inside* a frame is an error: the server died mid-sentence.
    pub async fn next(&mut self) -> Result<Option<Vec<u8>>, LspError> {
        let mut chunk = [0u8; 8 * 1024];
        loop {
            if let Some(frame) = self.take_frame()? {
                return Ok(Some(frame));
            }
            let read = self.source.read(&mut chunk).await?;
            if read == 0 {
                return if self.pending.is_empty() {
                    Ok(None)
                } else {
                    Err(LspError::Protocol("stream ended in the middle of a frame".into()))
                };
            }
            self.pending.extend_from_slice(&chunk[..read]);
        }
    }

    /// If `pending` holds a complete frame, remove and return it. Pure over the buffer.
    fn take_frame(&mut self) -> Result<Option<Vec<u8>>, LspError> {
        // Headers end at the first blank line.
        let Some(header_end) = find(&self.pending, b"\r\n\r\n") else {
            return Ok(None);
        };
        let headers = std::str::from_utf8(&self.pending[..header_end])
            .map_err(|_| LspError::Protocol("header is not ASCII".into()))?;

        let mut length: Option<usize> = None;
        for line in headers.split("\r\n") {
            // Header names are case-insensitive per the spec.
            if let Some(value) = line.strip_prefix_ignore_case("Content-Length:") {
                length = Some(
                    value
                        .trim()
                        .parse()
                        .map_err(|_| LspError::Protocol(format!("bad Content-Length: {value:?}")))?,
                );
            }
        }
        let Some(length) = length else {
            return Err(LspError::Protocol(format!("no Content-Length in {headers:?}")));
        };

        let body_start = header_end + 4;
        let body_end = body_start + length;
        if self.pending.len() < body_end {
            return Ok(None); // the rest of the body has not arrived yet
        }
        let body = self.pending[body_start..body_end].to_vec();
        self.pending.drain(..body_end);
        Ok(Some(body))
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// `str` has `strip_prefix` but not a case-insensitive one; a two-line extension trait is
/// simpler than lower-casing every header line first.
trait StripPrefixIgnoreCase {
    fn strip_prefix_ignore_case<'a>(&'a self, prefix: &str) -> Option<&'a str>;
}

impl StripPrefixIgnoreCase for str {
    fn strip_prefix_ignore_case<'a>(&'a self, prefix: &str) -> Option<&'a str> {
        let head = self.get(..prefix.len())?;
        head.eq_ignore_ascii_case(prefix).then(|| &self[prefix.len()..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader over in-memory bytes, delivered in chunks of `chunk` bytes to imitate a pipe.
    fn reader(bytes: Vec<u8>, chunk: usize) -> FrameReader<tokio::io::BufReader<std::io::Cursor<Vec<u8>>>> {
        FrameReader::new(tokio::io::BufReader::with_capacity(
            chunk,
            std::io::Cursor::new(bytes),
        ))
    }

    #[test]
    fn encode_writes_the_header_and_the_body() {
        assert_eq!(encode(b"{}"), b"Content-Length: 2\r\n\r\n{}");
    }

    #[tokio::test]
    async fn a_frame_round_trips() {
        let mut r = reader(encode(br#"{"id":1}"#), 8192);
        assert_eq!(r.next().await.unwrap().unwrap(), br#"{"id":1}"#);
        assert!(r.next().await.unwrap().is_none(), "clean end of stream");
    }

    #[tokio::test]
    async fn two_frames_in_one_chunk_come_out_one_at_a_time() {
        let mut bytes = encode(b"first");
        bytes.extend(encode(b"second"));
        let mut r = reader(bytes, 8192);
        assert_eq!(r.next().await.unwrap().unwrap(), b"first");
        assert_eq!(r.next().await.unwrap().unwrap(), b"second");
        assert!(r.next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_frame_split_across_tiny_chunks_is_reassembled() {
        // Three bytes at a time: header boundaries and body boundaries all land mid-chunk.
        let body = br#"{"jsonrpc":"2.0","method":"initialized"}"#;
        let mut r = reader(encode(body), 3);
        assert_eq!(r.next().await.unwrap().unwrap(), body);
    }

    #[tokio::test]
    async fn header_names_are_case_insensitive_and_content_type_is_ignored() {
        let bytes = b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}"
            .to_vec();
        let mut r = reader(bytes, 8192);
        assert_eq!(r.next().await.unwrap().unwrap(), b"{}");
    }

    #[tokio::test]
    async fn a_stream_that_dies_mid_frame_is_an_error_not_a_clean_end() {
        let mut bytes = encode(b"0123456789");
        bytes.truncate(bytes.len() - 4);
        let mut r = reader(bytes, 8192);
        assert!(matches!(r.next().await, Err(LspError::Protocol(_))));
    }

    #[tokio::test]
    async fn a_header_without_a_length_is_an_error() {
        let mut r = reader(b"X-Nothing: 1\r\n\r\n{}".to_vec(), 8192);
        assert!(matches!(r.next().await, Err(LspError::Protocol(_))));
    }
}

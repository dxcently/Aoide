//! Blocking HTTP/1.1 framing for the door. Every request says
//! `Connection: close`, so a reply is one status line, its headers, then a body
//! that is `Content-Length`, chunked, or read to the end of the stream.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;

use crate::error::DoorError;

/// The longest status line or header line read. Longer is malformed.
const MAX_LINE: u64 = 16 * 1024;

pub(crate) struct Head {
    pub status: u16,
    pub chunked: bool,
    pub length: Option<u64>,
}

/// Writes one request. A body is JSON, sent with its Content-Type and length; no
/// body sends neither header.
pub(crate) fn write_request(
    w: &mut impl Write,
    method: &str,
    path: &str,
    host: &str,
    token: &str,
    body: Option<&[u8]>,
) -> io::Result<()> {
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n"
    );
    if let Some(body) = body {
        head.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    if let Some(body) = body {
        w.write_all(body)?;
    }
    w.flush()
}

/// Reads a status line and its headers; the body is left unread.
pub(crate) fn read_head(r: &mut impl BufRead) -> Result<Head, DoorError> {
    let status_line = line(r).map_err(head_error)?;
    let mut words = status_line.split(' ');
    let version = words.next().unwrap_or_default();
    let status = words.next().and_then(|code| code.parse::<u16>().ok());
    let Some(status) = status.filter(|_| version.starts_with("HTTP/1.")) else {
        return Err(DoorError::Malformed(format!("not an HTTP/1 reply: {status_line:?}")));
    };
    let mut chunked = false;
    let mut length = None;
    loop {
        let header = line(r).map_err(head_error)?;
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            return Err(DoorError::Malformed(format!("a header with no colon: {header:?}")));
        };
        let (name, value) = (name.trim(), value.trim());
        if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked |= value.split(',').any(|coding| coding.trim().eq_ignore_ascii_case("chunked"));
        } else if name.eq_ignore_ascii_case("content-length") {
            let n = value.parse::<u64>().map_err(|_| DoorError::Malformed(format!("content-length {value:?}")))?;
            length = Some(n);
        }
    }
    if status == 204 || status == 304 {
        return Ok(Head { status, chunked: false, length: Some(0) });
    }
    Ok(Head { status, chunked, length })
}

/// One line, its CRLF or LF taken off. A line over [`MAX_LINE`], or one the
/// reply ends inside, is an error.
fn line(r: &mut impl BufRead) -> io::Result<String> {
    let mut text = String::new();
    let read = (&mut *r).take(MAX_LINE).read_line(&mut text)?;
    if read == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the reply ended"));
    }
    if !text.ends_with('\n') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "a line over 16 KiB, or a reply cut short"));
    }
    Ok(text.trim_end_matches(['\r', '\n']).to_string())
}

fn head_error(e: io::Error) -> DoorError {
    match e.kind() {
        io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData => DoorError::Malformed(e.to_string()),
        _ => DoorError::Io(e),
    }
}

/// A chunk-size line: hex, then optional `;` extensions, which are ignored.
fn chunk_size(r: &mut impl BufRead) -> io::Result<u64> {
    let text = line(r)?;
    let digits = text.split(';').next().unwrap_or_default().trim();
    u64::from_str_radix(digits, 16)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "a chunk size that is not hex"))
}

/// The CRLF that closes a chunk's data.
fn crlf(r: &mut impl BufRead) -> io::Result<()> {
    let mut end = [0u8; 2];
    r.read_exact(&mut end)?;
    if end != *b"\r\n" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "a chunk not closed by CRLF"));
    }
    Ok(())
}

/// The trailer lines after the last chunk, through the blank line that ends them.
fn trailers(r: &mut impl BufRead) -> io::Result<()> {
    while !line(r)?.is_empty() {}
    Ok(())
}

/// A chunked body, decoded as it is read: one chunk's bytes at a time, never a
/// whole chunk buffered.
pub(crate) struct Chunked<R> {
    inner: R,
    left: u64,
    done: bool,
}

impl<R: BufRead> Chunked<R> {
    pub(crate) fn new(inner: R) -> Self {
        Chunked { inner, left: 0, done: false }
    }
}

impl<R: BufRead> Read for Chunked<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.done || buf.is_empty() {
            return Ok(0);
        }
        if self.left == 0 {
            let size = chunk_size(&mut self.inner)?;
            if size == 0 {
                trailers(&mut self.inner)?;
                self.done = true;
                return Ok(0);
            }
            self.left = size;
        }
        let want = buf.len().min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let n = self.inner.read(&mut buf[..want])?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "a chunked body ended inside a chunk"));
        }
        self.left -= n as u64;
        if self.left == 0 {
            crlf(&mut self.inner)?;
        }
        Ok(n)
    }
}

/// The body reader a head's framing calls for.
pub(crate) fn body_reader(head: &Head, reader: BufReader<TcpStream>) -> Box<dyn Read + Send> {
    if head.chunked {
        Box::new(Chunked::new(reader))
    } else if let Some(length) = head.length {
        Box::new(reader.take(length))
    } else {
        Box::new(reader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// One byte per read, so every framing boundary is also a read boundary.
    struct Trickle(Cursor<Vec<u8>>);

    impl Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(1);
            self.0.read(&mut buf[..n])
        }
    }

    #[test]
    fn a_chunked_body_decodes_across_one_byte_reads() {
        let wire = b"5\r\nhello\r\n6;ext=1\r\n world\r\n0\r\nX-Trailer: y\r\n\r\n".to_vec();
        let mut body = Chunked::new(BufReader::new(Trickle(Cursor::new(wire))));
        let mut out = String::new();
        body.read_to_string(&mut out).unwrap();
        assert_eq!(out, "hello world");
    }

    #[test]
    fn a_chunked_body_that_ends_early_is_an_error_not_a_short_body() {
        let mut body = Chunked::new(BufReader::new(Cursor::new(b"5\r\nhel".to_vec())));
        let mut out = Vec::new();
        assert!(body.read_to_end(&mut out).is_err());
    }

    #[test]
    fn a_status_line_and_its_length_are_read() {
        let wire = b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec();
        let head = read_head(&mut BufReader::new(Cursor::new(wire))).unwrap();
        assert_eq!(head.status, 401);
        assert_eq!(head.length, Some(0));
        assert!(!head.chunked);
    }

    #[test]
    fn a_chunked_head_is_read_as_chunked() {
        let wire = b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        let head = read_head(&mut BufReader::new(Cursor::new(wire))).unwrap();
        assert!(head.chunked);
        assert_eq!(head.status, 200);
    }

    #[test]
    fn a_reply_that_is_not_http_is_malformed() {
        let wire = b"SSH-2.0-OpenSSH\r\n\r\n".to_vec();
        assert!(matches!(read_head(&mut BufReader::new(Cursor::new(wire))), Err(DoorError::Malformed(_))));
    }

    #[test]
    fn a_post_carries_host_bearer_close_and_its_json_body() {
        let body = br#"{"text":"hi"}"#;
        let mut out = Vec::new();
        write_request(&mut out, "POST", "/api/say", "127.0.0.1:4477", "tok", Some(body)).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("POST /api/say HTTP/1.1\r\n"));
        assert!(text.contains("\r\nHost: 127.0.0.1:4477\r\n"));
        assert!(text.contains("\r\nAuthorization: Bearer tok\r\n"));
        assert!(text.contains("\r\nConnection: close\r\n"));
        let framing = format!("\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len());
        assert!(text.contains(&framing), "{text}");
        assert!(text.ends_with(r#"{"text":"hi"}"#));
    }

    #[test]
    fn a_get_carries_no_body_and_no_length() {
        let mut out = Vec::new();
        write_request(&mut out, "GET", "/api/events", "[::1]:4477", "tok", None).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\r\nHost: [::1]:4477\r\n"));
        assert!(!text.contains("Content-Length"));
        assert!(text.ends_with("\r\n\r\n"));
    }
}

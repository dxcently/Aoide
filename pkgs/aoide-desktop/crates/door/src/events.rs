//! `GET /api/events` as a blocking iterator of [`Frame`]s. The door writes one
//! `data: <json>` line per frame, then a blank line. The decoder reads the body
//! in pieces and enforces A10's cap while it reads, so no frame over
//! [`MAX_FRAME`] is ever held whole.

use std::collections::VecDeque;
use std::io::Read;

use crate::error::DoorError;
use crate::frame::{decode, Frame};

/// A10: a frame larger than this is dropped and counted, and decoding resumes
/// at the next frame boundary.
pub(crate) const MAX_FRAME: usize = 1 << 20;

/// The SSE state: the line being read, the frame's data, and the frames done.
#[derive(Default)]
pub(crate) struct Sse {
    line: Vec<u8>,
    line_long: bool,
    data: Vec<u8>,
    event_long: bool,
    pub(crate) ready: VecDeque<Vec<u8>>,
    pub(crate) dropped: u64,
}

impl Sse {
    /// Takes the next bytes of the body. Every complete frame lands in `ready`.
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if byte == b'\n' {
                self.end_line();
            } else if self.line.len() < MAX_FRAME {
                self.line.push(byte);
            } else {
                self.line_long = true;
            }
        }
    }

    fn end_line(&mut self) {
        let long = std::mem::take(&mut self.line_long);
        let mut line = std::mem::take(&mut self.line);
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if long {
            if line.starts_with(b"data:") {
                self.event_long = true;
                self.data.clear();
            }
        } else if line.is_empty() {
            self.dispatch();
        } else if let Some(value) = line.strip_prefix(b"data:") {
            self.push_data(value.strip_prefix(b" ").unwrap_or(value));
        }
    }

    fn push_data(&mut self, value: &[u8]) {
        if self.event_long {
            return;
        }
        let joined = if self.data.is_empty() { value.len() } else { value.len() + 1 };
        if self.data.len() + joined > MAX_FRAME {
            self.event_long = true;
            self.data.clear();
            return;
        }
        if !self.data.is_empty() {
            self.data.push(b'\n');
        }
        self.data.extend_from_slice(value);
    }

    fn dispatch(&mut self) {
        let data = std::mem::take(&mut self.data);
        if std::mem::take(&mut self.event_long) {
            self.dropped += 1;
        } else if !data.is_empty() {
            self.ready.push_back(data);
        }
    }
}

/// The frames of an open events stream. Iteration ends when the stream does,
/// or after a `hello` whose protocol is not 1 (that item is the taught error).
pub struct Events {
    body: Box<dyn Read + Send>,
    sse: Sse,
    done: bool,
}

impl Events {
    pub(crate) fn new(body: Box<dyn Read + Send>) -> Events {
        Events { body, sse: Sse::default(), done: false }
    }

    /// Frames dropped for passing the cap, since the stream opened.
    pub fn dropped_oversize(&self) -> u64 {
        self.sse.dropped
    }
}

impl Iterator for Events {
    type Item = Result<Frame, DoorError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.done {
                return None;
            }
            if let Some(payload) = self.sse.ready.pop_front() {
                let frame = decode(&payload);
                if matches!(frame, Err(DoorError::Protocol { .. })) {
                    self.done = true;
                }
                return Some(frame);
            }
            let mut buf = [0u8; 8192];
            match self.body.read(&mut buf) {
                Ok(0) => self.done = true,
                Ok(n) => self.sse.feed(&buf[..n]),
                Err(e) => {
                    self.done = true;
                    return Some(Err(DoorError::Io(e)));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payloads(feeds: &[&[u8]]) -> (Vec<Vec<u8>>, u64) {
        let mut sse = Sse::default();
        for bytes in feeds {
            sse.feed(bytes);
        }
        (sse.ready.drain(..).collect(), sse.dropped)
    }

    #[test]
    fn a_frame_split_at_every_byte_decodes_once_even_inside_a_utf8_char() {
        let frame = "data: {\"type\":\"text-delta\",\"text\":\"é—ok\"}\n\n";
        let expected = r#"{"type":"text-delta","text":"é—ok"}"#.as_bytes().to_vec();
        for at in 0..=frame.len() {
            let (a, b) = frame.as_bytes().split_at(at);
            let (out, dropped) = payloads(&[a, b]);
            assert_eq!(out, vec![expected.clone()], "split at byte {at}");
            assert_eq!(dropped, 0);
        }
    }

    #[test]
    fn comments_and_other_sse_fields_are_not_frames() {
        let (out, dropped) = payloads(&[b": keep-alive\n\nevent: frame\nid: 7\n\n"]);
        assert!(out.is_empty());
        assert_eq!(dropped, 0);
    }

    #[test]
    fn an_oversize_frame_is_dropped_and_counted_and_the_next_one_decodes() {
        let big = format!("data: {}\n\n", "x".repeat(2 * MAX_FRAME));
        let small = "data: {\"type\":\"turn-state\",\"running\":true}\n\n";
        let stream = [big.as_bytes(), small.as_bytes()].concat();
        let mut sse = Sse::default();
        for chunk in stream.chunks(8192) {
            sse.feed(chunk);
            assert!(sse.line.len() <= MAX_FRAME, "the line buffer holds at most the cap");
            assert!(sse.data.len() <= MAX_FRAME, "the frame buffer holds at most the cap");
        }
        assert_eq!(sse.dropped, 1);
        let out: Vec<Vec<u8>> = sse.ready.into_iter().collect();
        assert_eq!(out, vec![b"{\"type\":\"turn-state\",\"running\":true}".to_vec()]);
    }
}

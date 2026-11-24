use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::cmd::Cmd;
use crate::frame::Frame;

const EXPIRY: Duration = Duration::from_secs(5);

struct Partial {
    total: u8,
    next: u8,
    body: Vec<u8>,
    started: Instant,
}

/// Joins fragmented frames. Fragment 1 starts a message and the others must
/// follow in order; anything else drops the partial message.
#[derive(Default)]
pub struct Reassembler {
    partial: HashMap<Cmd, Partial>,
}

impl Reassembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, frame: Frame, now: Instant) -> Option<Frame> {
        if !frame.cmd.is_fragmented() {
            return Some(frame);
        }
        let (&info, chunk) = frame.body.split_first()?;
        let index = info >> 4;
        let total = info & 0x0f;
        if index == 0 || total == 0 || index > total {
            return None;
        }
        if index == 1 {
            self.partial.insert(
                frame.cmd,
                Partial {
                    total,
                    next: 1,
                    body: Vec::new(),
                    started: now,
                },
            );
        }
        let part = self.partial.get_mut(&frame.cmd)?;
        if part.next != index || part.total != total {
            self.partial.remove(&frame.cmd);
            return None;
        }
        part.body.extend_from_slice(chunk);
        part.next += 1;
        if part.next <= part.total {
            return None;
        }
        let part = self.partial.remove(&frame.cmd)?;
        Some(Frame {
            dir: frame.dir,
            function: frame.function,
            cmd: frame.cmd.with_fragmented(false),
            body: part.body,
        })
    }

    pub fn expire(&mut self, now: Instant) {
        self.partial
            .retain(|_, part| now.duration_since(part.started) < EXPIRY);
    }

    pub fn clear(&mut self) {
        self.partial.clear();
    }
}

/// Splits a frame whose body exceeds `max_chunk` into numbered fragments.
pub fn fragment(frame: Frame, max_chunk: usize) -> Vec<Frame> {
    let max_chunk = max_chunk.max(1);
    if frame.body.len() <= max_chunk {
        return vec![frame];
    }
    let chunks: Vec<&[u8]> = frame.body.chunks(max_chunk).collect();
    let total = chunks.len().min(15) as u8;
    chunks
        .into_iter()
        .take(15)
        .enumerate()
        .map(|(i, chunk)| {
            let mut body = Vec::with_capacity(chunk.len() + 1);
            body.push(((i as u8 + 1) << 4) | total);
            body.extend_from_slice(chunk);
            Frame {
                dir: frame.dir,
                function: frame.function,
                cmd: frame.cmd.with_fragmented(true),
                body,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::FUNCTION_SESSION;
    use proptest::prelude::*;

    fn frame(hex: &str) -> Frame {
        Frame::decode(&hex::decode(hex).expect("hex")).expect("frame")
    }

    #[test]
    fn passes_unfragmented_frames() {
        let mut r = Reassembler::new();
        let f = frame("ff091a0003010f484a6e744378c57c16ca8ab3a40bebb6f39807");
        assert_eq!(r.push(f.clone(), Instant::now()), Some(f));
    }

    proptest! {
        #[test]
        fn fragment_then_join(body in prop::collection::vec(any::<u8>(), 0..1200), chunk in 80usize..300) {
            let original = Frame::new(FUNCTION_SESSION, Cmd(0x4405), body);
            let parts = fragment(original.clone(), chunk);
            let mut r = Reassembler::new();
            let now = Instant::now();
            let mut joined = None;
            for p in parts {
                joined = r.push(p, now);
            }
            prop_assert_eq!(joined, Some(original));
        }
    }
}

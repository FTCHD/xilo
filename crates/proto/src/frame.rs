use crate::cmd::Cmd;

pub const MAGIC: [u8; 2] = [0xff, 0x09];
pub const HEADER_LEN: usize = 9;
pub const MIN_LEN: usize = HEADER_LEN + 1;
pub const MAX_LEN: usize = 4096;

pub const FUNCTION_HANDSHAKE: u8 = 0x01;
pub const FUNCTION_SESSION: u8 = 0x0f;

pub const DIR_HOST: u8 = 0x00;
pub const DIR_DEVICE: u8 = 0x01;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame {
    pub dir: u8,
    pub function: u8,
    pub cmd: Cmd,
    pub body: Vec<u8>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FrameError {
    #[error("frame too short ({0} bytes)")]
    TooShort(usize),
    #[error("bad magic")]
    BadMagic,
    #[error("length field {field} does not match {actual} bytes")]
    Length { field: usize, actual: usize },
    #[error("bad checksum")]
    Checksum,
}

pub fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |acc, b| acc ^ b)
}

impl Frame {
    pub fn new(function: u8, cmd: Cmd, body: Vec<u8>) -> Self {
        Self {
            dir: DIR_HOST,
            function,
            cmd,
            body,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let len = HEADER_LEN + self.body.len() + 1;
        let mut out = Vec::with_capacity(len);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&(len as u16).to_le_bytes());
        out.extend_from_slice(&[0x03, self.dir, self.function]);
        out.extend_from_slice(&self.cmd.to_bytes());
        out.extend_from_slice(&self.body);
        out.push(checksum(&out));
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, FrameError> {
        if bytes.len() < MIN_LEN {
            return Err(FrameError::TooShort(bytes.len()));
        }
        if bytes[..2] != MAGIC {
            return Err(FrameError::BadMagic);
        }
        let field = u16::from_le_bytes([bytes[2], bytes[3]]) as usize;
        if field != bytes.len() {
            return Err(FrameError::Length {
                field,
                actual: bytes.len(),
            });
        }
        let (content, sum) = bytes.split_at(bytes.len() - 1);
        if checksum(content) != sum[0] {
            return Err(FrameError::Checksum);
        }
        Ok(Self {
            dir: bytes[5],
            function: bytes[6],
            cmd: Cmd::from_bytes(bytes[7], bytes[8]),
            body: bytes[HEADER_LEN..bytes.len() - 1].to_vec(),
        })
    }
}

/// Splits a byte stream into frames. Handles several frames in one
/// notification, frames split over notifications, and junk between frames.
#[derive(Default)]
pub struct StreamDecoder {
    buf: Vec<u8>,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.buf.clear();
    }

    pub fn pending(&self) -> usize {
        self.buf.len()
    }

    pub fn push(&mut self, chunk: &[u8], out: &mut Vec<Result<Frame, FrameError>>) {
        self.buf.extend_from_slice(chunk);
        loop {
            let Some(start) = find_start(&self.buf) else {
                let keep = usize::from(self.buf.last() == Some(&MAGIC[0]));
                let drop = self.buf.len() - keep;
                self.buf.drain(..drop);
                return;
            };
            self.buf.drain(..start);
            if self.buf.len() < 4 {
                return;
            }
            let len = u16::from_le_bytes([self.buf[2], self.buf[3]]) as usize;
            if !(MIN_LEN..=MAX_LEN).contains(&len) {
                self.buf.drain(..1);
                continue;
            }
            if self.buf.len() < 5 || self.buf[4] != 0x03 {
                if self.buf.len() < 5 {
                    return;
                }
                self.buf.drain(..1);
                continue;
            }
            if self.buf.len() < len {
                if self.buf.len() >= MAX_LEN {
                    self.buf.drain(..1);
                    continue;
                }
                return;
            }
            match Frame::decode(&self.buf[..len]) {
                Ok(frame) => {
                    out.push(Ok(frame));
                    self.buf.drain(..len);
                }
                Err(err) => {
                    out.push(Err(err));
                    self.buf.drain(..1);
                }
            }
        }
    }
}

fn find_start(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == MAGIC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn frame(hex: &str) -> Vec<u8> {
        hex::decode(hex).expect("hex")
    }

    #[test]
    fn decodes_device_handshake_reply() {
        let f = Frame::decode(&frame("ff090e00030001080100a1010152")).expect("frame");
        assert_eq!(f.function, FUNCTION_HANDSHAKE);
        assert_eq!(f.cmd, Cmd(0x0801));
        assert_eq!(f.body, frame("00a10101"));
    }

    #[test]
    fn round_trips() {
        let f = Frame::new(FUNCTION_SESSION, Cmd(0x404b), vec![1, 2, 3]);
        assert_eq!(Frame::decode(&f.encode()), Ok(f));
    }

    #[test]
    fn rejects_bad_checksum() {
        let mut bytes = frame("ff090e00030001080100a1010152");
        bytes[10] ^= 1;
        assert_eq!(Frame::decode(&bytes), Err(FrameError::Checksum));
    }

    #[test]
    fn splits_concatenated_and_split_input() {
        let a = frame("ff090e00030001080100a1010152");
        let b = frame("ff090b00030001080500f2");
        let mut joined = vec![0x00, 0x13];
        joined.extend(&a);
        joined.extend(&b);
        let mut dec = StreamDecoder::new();
        let mut out = Vec::new();
        for chunk in joined.chunks(5) {
            dec.push(chunk, &mut out);
        }
        let frames: Vec<_> = out.into_iter().filter_map(Result::ok).collect();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].cmd, Cmd(0x0805));
        assert_eq!(dec.pending(), 0);
    }

    proptest! {
        #[test]
        fn stream_recovers_every_frame(
            bodies in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..80), 1..6),
            junk in prop::collection::vec(any::<u8>(), 0..20),
            chunk in 1usize..40,
        ) {
            let frames: Vec<Frame> = bodies
                .into_iter()
                .enumerate()
                .map(|(i, body)| Frame::new(FUNCTION_SESSION, Cmd(0x4400 + i as u16), body))
                .collect();
            let mut stream: Vec<u8> = junk.iter().copied().filter(|b| *b != 0xff).collect();
            for f in &frames {
                stream.extend(f.encode());
            }
            let mut dec = StreamDecoder::new();
            let mut out = Vec::new();
            for piece in stream.chunks(chunk) {
                dec.push(piece, &mut out);
            }
            let got: Vec<Frame> = out.into_iter().filter_map(Result::ok).collect();
            prop_assert_eq!(got, frames);
        }

        #[test]
        fn decode_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..300)) {
            let _ = Frame::decode(&bytes);
            let mut dec = StreamDecoder::new();
            dec.push(&bytes, &mut Vec::new());
        }
    }
}

use std::fmt;

const FRAGMENTED: u16 = 0x8000;
const ENCRYPTED: u16 = 0x4000;
const RESPONSE: u16 = 0x0800;
const PUSH: u16 = 0x0400;

/// Command as it appears on the wire, flags included.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cmd(pub u16);

/// Command without the fragment and encryption flags, e.g. `c405` → `0405`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MsgId(pub u16);

impl Cmd {
    pub fn from_bytes(hi: u8, lo: u8) -> Self {
        Self(u16::from_be_bytes([hi, lo]))
    }

    pub fn to_bytes(self) -> [u8; 2] {
        self.0.to_be_bytes()
    }

    pub fn id(self) -> MsgId {
        MsgId(self.0 & !(FRAGMENTED | ENCRYPTED))
    }

    pub fn is_fragmented(self) -> bool {
        self.0 & FRAGMENTED != 0
    }

    pub fn is_encrypted(self) -> bool {
        self.0 & ENCRYPTED != 0
    }

    pub fn is_response(self) -> bool {
        self.0 & RESPONSE != 0
    }

    pub fn is_push(self) -> bool {
        self.0 & PUSH != 0
    }

    pub fn lo(self) -> u8 {
        self.to_bytes()[1]
    }

    pub fn with_fragmented(self, on: bool) -> Self {
        Self(if on {
            self.0 | FRAGMENTED
        } else {
            self.0 & !FRAGMENTED
        })
    }

    pub fn with_encrypted(self, on: bool) -> Self {
        Self(if on {
            self.0 | ENCRYPTED
        } else {
            self.0 & !ENCRYPTED
        })
    }
}

impl MsgId {
    pub const fn new(id: u16) -> Self {
        Self(id)
    }

    pub fn cmd(self, encrypted: bool) -> Cmd {
        Cmd(self.0).with_encrypted(encrypted)
    }

    pub fn response(self) -> MsgId {
        MsgId(self.0 | RESPONSE)
    }

    pub fn request(self) -> MsgId {
        MsgId(self.0 & !(RESPONSE | PUSH))
    }

    pub fn parse(text: &str) -> Option<Self> {
        u16::from_str_radix(text.trim_start_matches("0x"), 16)
            .ok()
            .map(Self)
    }
}

impl fmt::Debug for Cmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}", self.0)
    }
}

impl fmt::Display for Cmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}", self.0)
    }
}

impl fmt::Debug for MsgId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}", self.0)
    }
}

impl fmt::Display for MsgId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_and_ids() {
        let c = Cmd(0xc840);
        assert!(c.is_fragmented() && c.is_encrypted() && c.is_response() && !c.is_push());
        assert_eq!(c.id(), MsgId(0x0840));
        assert_eq!(Cmd(0x4401).id(), MsgId(0x0401));
        assert!(Cmd(0x4401).is_push());
        assert_eq!(MsgId(0x004b).cmd(true), Cmd(0x404b));
        assert_eq!(MsgId(0x004b).response(), MsgId(0x084b));
        assert_eq!(MsgId(0x0840).request(), MsgId(0x0040));
        assert_eq!(MsgId::parse("0405"), Some(MsgId(0x0405)));
        assert_eq!(Cmd(0x404a).to_string(), "404a");
    }
}

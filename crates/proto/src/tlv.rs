pub const TYPE_STR: u8 = 0x00;
pub const TYPE_U8: u8 = 0x01;
pub const TYPE_U16: u8 = 0x02;
pub const TYPE_U32: u8 = 0x03;
pub const TYPE_BYTES: u8 = 0x04;
pub const TYPE_F32: u8 = 0x05;
pub const TYPE_MIXED: u8 = 0x06;

pub const KEY_HEADER: u8 = 0xa1;
pub const KEY_TIMESTAMP: u8 = 0xfe;

/// One parameter. `ty` is the type byte when the payload is typed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Item {
    pub key: u8,
    pub ty: Option<u8>,
    pub data: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Payload {
    pub status: Option<u8>,
    pub items: Vec<Item>,
    pub truncated: bool,
}

impl Item {
    pub fn uint(&self) -> Option<u64> {
        if self.data.is_empty() || self.data.len() > 8 {
            return None;
        }
        let mut buf = [0u8; 8];
        buf[..self.data.len()].copy_from_slice(&self.data);
        Some(u64::from_le_bytes(buf))
    }

    pub fn text(&self) -> String {
        let end = self.data.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
        String::from_utf8_lossy(&self.data[..end]).into_owned()
    }

    /// The raw value bytes including the type byte, as anker-solix-api and
    /// SolixBLE print them.
    pub fn raw(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len() + 1);
        out.extend(self.ty);
        out.extend_from_slice(&self.data);
        out
    }
}

impl Payload {
    pub fn get(&self, key: u8) -> Option<&Item> {
        self.items.iter().find(|i| i.key == key)
    }

    pub fn is_ok(&self) -> bool {
        self.status.is_none_or(|s| s == 0)
    }
}

/// Parses a payload. Responses may start with a status byte. In typed
/// payloads every item except the first (`a1`) carries a type byte.
pub fn parse(bytes: &[u8], response: bool, typed: bool) -> Payload {
    let mut payload = Payload::default();
    let mut rest = bytes;
    if response
        && let Some((&first, tail)) = rest.split_first()
        && first != KEY_HEADER
    {
        payload.status = Some(first);
        rest = tail;
    }
    let mut first = true;
    while let Some((&key, tail)) = rest.split_first() {
        let Some((&len, tail)) = tail.split_first() else {
            payload.items.push(Item {
                key,
                ty: None,
                data: Vec::new(),
            });
            break;
        };
        let len = len as usize;
        let take = len.min(tail.len());
        let value = &tail[..take];
        if take < len {
            payload.truncated = true;
        }
        let item = if typed && !first && !value.is_empty() {
            Item {
                key,
                ty: Some(value[0]),
                data: value[1..].to_vec(),
            }
        } else {
            Item {
                key,
                ty: None,
                data: value.to_vec(),
            }
        };
        payload.items.push(item);
        rest = &tail[take..];
        first = false;
    }
    payload
}

/// A typed request parameter.
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    U8(u8),
    U16(u16),
    U32(u32),
    Str(String),
    Bytes(Vec<u8>),
}

impl Value {
    fn encode(&self) -> (u8, Vec<u8>) {
        match self {
            Value::U8(v) => (TYPE_U8, vec![*v]),
            Value::U16(v) => (TYPE_U16, v.to_le_bytes().to_vec()),
            Value::U32(v) => (TYPE_U32, v.to_le_bytes().to_vec()),
            Value::Str(v) => (TYPE_STR, v.as_bytes().to_vec()),
            Value::Bytes(v) => (TYPE_BYTES, v.clone()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimestampStyle {
    /// `fe 05 03 <u32>` as used by power stations.
    Typed,
    /// `fe 04 <u32>` as used by Prime devices.
    Untyped,
}

pub const SESSION_HEADER: [u8; 3] = [KEY_HEADER, 0x01, 0x21];

/// Builds a business request: session header, parameters in ascending key
/// order, then the timestamp.
pub fn build_request(params: &[(u8, Value)], timestamp: u32, style: TimestampStyle) -> Vec<u8> {
    let mut sorted: Vec<&(u8, Value)> = params.iter().collect();
    sorted.sort_by_key(|(k, _)| *k);
    let mut out = SESSION_HEADER.to_vec();
    for (key, value) in sorted {
        let (ty, data) = value.encode();
        out.push(*key);
        out.push((data.len() + 1) as u8);
        out.push(ty);
        out.extend_from_slice(&data);
    }
    out.push(KEY_TIMESTAMP);
    match style {
        TimestampStyle::Typed => out.extend_from_slice(&[0x05, TYPE_U32]),
        TimestampStyle::Untyped => out.push(0x04),
    }
    out.extend_from_slice(&timestamp.to_le_bytes());
    out
}

/// Builds an untyped payload, as used during the handshake.
pub fn build_untyped(items: &[(u8, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (key, data) in items {
        out.push(*key);
        out.push(data.len() as u8);
        out.extend_from_slice(data);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn h(s: &str) -> Vec<u8> {
        hex::decode(s).expect("hex")
    }

    #[test]
    fn parses_handshake_reply_with_status() {
        let p = parse(&h("00a10102a202fd00a30144a40101a50102"), true, false);
        assert_eq!(p.status, Some(0));
        assert_eq!(p.get(0xa2).and_then(Item::uint), Some(253));
        assert_eq!(p.get(0xa3).and_then(Item::uint), Some(0x44));
    }

    #[test]
    fn parses_typed_push_without_status() {
        let p = parse(&h("a10131a2050300000000a40302ffffc51100415a56"), true, true);
        assert_eq!(p.status, None);
        assert_eq!(p.get(0xa1).map(|i| i.data.clone()), Some(vec![0x31]));
        let a4 = p.get(0xa4).expect("a4");
        assert_eq!((a4.ty, a4.uint()), (Some(TYPE_U16), Some(0xffff)));
        assert!(p.truncated);
    }

    #[test]
    fn ack_is_ok() {
        let p = parse(&h("00a10131"), true, true);
        assert!(p.is_ok());
        assert_eq!(p.items.len(), 1);
    }

    #[test]
    fn builds_dc_on_request() {
        let req = build_request(&[(0xa2, Value::U8(1))], 0x698cad47, TimestampStyle::Typed);
        assert_eq!(req, h("a10121a2020101fe050347ad8c69"));
        let req = build_request(&[(0xa2, Value::U16(30))], 0x698cad47, TimestampStyle::Typed);
        assert_eq!(req, h("a10121a203021e00fe050347ad8c69"));
    }

    proptest! {
        #[test]
        fn parse_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..200), r: bool, t: bool) {
            let _ = parse(&bytes, r, t);
        }

        #[test]
        fn typed_round_trip(values in prop::collection::vec(any::<u32>(), 0..10)) {
            let params: Vec<(u8, Value)> = values.iter().enumerate().map(|(i, v)| (0xa2 + i as u8, Value::U32(*v))).collect();
            let bytes = build_request(&params, 7, TimestampStyle::Typed);
            let p = parse(&bytes, false, true);
            for (i, v) in values.iter().enumerate() {
                prop_assert_eq!(p.get(0xa2 + i as u8).and_then(Item::uint), Some(*v as u64));
            }
            prop_assert_eq!(p.get(KEY_TIMESTAMP).and_then(Item::uint), Some(7));
        }
    }
}

/// Parses `key=type:value`, for example `a2=u8:1` or `a3=hex:0102`.
pub fn parse_param(text: &str) -> Result<(u8, Value), String> {
    let usage = || format!("use key=type:value, not {text}");
    let (key, rest) = text.split_once('=').ok_or_else(usage)?;
    let (ty, value) = rest.split_once(':').ok_or_else(usage)?;
    let key = u8::from_str_radix(key.trim(), 16).map_err(|_| format!("bad key {key}"))?;
    let bad = |_| format!("bad {ty} value {value}");
    let value = match ty {
        "u8" => Value::U8(value.parse().map_err(bad)?),
        "u16" => Value::U16(value.parse().map_err(bad)?),
        "u32" => Value::U32(value.parse().map_err(bad)?),
        "str" => Value::Str(value.into()),
        "hex" => Value::Bytes(hex::decode(value).map_err(|_| format!("bad hex {value}"))?),
        other => return Err(format!("unknown type {other}")),
    };
    Ok((key, value))
}

#[cfg(test)]
mod param_tests {
    use super::*;

    #[test]
    fn parses_params() {
        assert_eq!(parse_param("a2=u8:1"), Ok((0xa2, Value::U8(1))));
        assert_eq!(
            parse_param("a3=hex:0102"),
            Ok((0xa3, Value::Bytes(vec![1, 2])))
        );
        assert!(parse_param("a2=u8:300").is_err());
        assert!(parse_param("a2").is_err());
    }
}

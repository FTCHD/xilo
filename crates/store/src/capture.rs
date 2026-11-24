use std::io::{BufRead, Write};

use serde::{Deserialize, Serialize};
use xilo_proto::{Cmd, Message, Trace, tlv};

use crate::StoreError;

/// One line of a `.xcap` capture file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub ms: u64,
    pub outgoing: bool,
    pub cmd: Option<String>,
    #[serde(default)]
    pub function: Option<u8>,
    pub raw: String,
    pub plain: Option<String>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub xilo_capture: u32,
    pub model: Option<String>,
    pub firmware: Option<String>,
    pub redacted: bool,
}

impl Record {
    pub fn from_trace(ms: u64, trace: &Trace) -> Self {
        Self {
            ms,
            outgoing: trace.outgoing,
            cmd: trace.cmd.map(|c| c.to_string()),
            function: trace.bytes.get(6).copied(),
            raw: hex::encode(&trace.bytes),
            plain: trace.plain.as_ref().map(hex::encode),
            note: trace.note.clone(),
        }
    }

    /// The decoded message, for incoming session frames with plaintext.
    pub fn message(&self) -> Option<Message> {
        if self.outgoing {
            return None;
        }
        let cmd = Cmd(u16::from_str_radix(self.cmd.as_deref()?, 16).ok()?);
        let plain = hex::decode(self.plain.as_deref()?).ok()?;
        let typed = self.function != Some(0x01);
        Some(Message {
            cmd,
            payload: tlv::parse(&plain, cmd.is_response(), typed),
        })
    }
}

/// Replaces every occurrence of the given secrets (serial numbers, MAC
/// addresses) with `X` bytes, in raw and plain hex.
pub fn redact(record: &mut Record, secrets: &[Vec<u8>]) {
    for secret in secrets.iter().filter(|s| s.len() >= 4) {
        let needle = hex::encode(secret);
        let mask = "58".repeat(secret.len());
        if let Some(plain) = &mut record.plain {
            *plain = plain.replace(&needle, &mask);
        }
        record.raw = String::new();
    }
}

pub fn write(out: &mut dyn Write, header: &Header, records: &[Record]) -> Result<(), StoreError> {
    serde_json::to_writer(&mut *out, header).map_err(|e| StoreError::Parse(e.to_string()))?;
    writeln!(out)?;
    for r in records {
        serde_json::to_writer(&mut *out, r).map_err(|e| StoreError::Parse(e.to_string()))?;
        writeln!(out)?;
    }
    Ok(())
}

pub fn read(input: &mut dyn BufRead) -> Result<(Header, Vec<Record>), StoreError> {
    let mut lines = input.lines();
    let first = lines
        .next()
        .ok_or_else(|| StoreError::Parse("empty capture".into()))??;
    let header: Header =
        serde_json::from_str(&first).map_err(|e| StoreError::Parse(e.to_string()))?;
    let mut records = Vec::new();
    for line in lines {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        records.push(serde_json::from_str(&line).map_err(|e| StoreError::Parse(e.to_string()))?);
    }
    Ok((header, records))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_reads_and_redacts() {
        let mut record = Record {
            ms: 5,
            outgoing: false,
            cmd: Some("4401".into()),
            function: Some(0x0f),
            raw: "ff09".into(),
            plain: Some(format!("a10131c51100{}", hex::encode("ALFSBK1G33300077"))),
            note: None,
        };
        redact(&mut record, &[b"ALFSBK1G33300077".to_vec()]);
        assert!(
            !record
                .plain
                .as_deref()
                .unwrap_or_default()
                .contains(&hex::encode("ALFSBK"))
        );
        let header = Header {
            xilo_capture: 1,
            model: Some("A1723".into()),
            firmware: None,
            redacted: true,
        };
        let mut buf = Vec::new();
        write(&mut buf, &header, std::slice::from_ref(&record)).expect("write");
        let (h, records) = read(&mut buf.as_slice()).expect("read");
        assert_eq!((h, records), (header, vec![record]));
    }
}

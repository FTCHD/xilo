use xilo_proto::{KeyPair, TimestampMode, ZxConfig};

/// Creates a client id in the 36-character UUID format devices expect.
pub fn new_client_id() -> String {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        b = *b"xilo-client-id-!";
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// A fresh P-256 key pair for one session.
pub fn new_key_pair() -> KeyPair {
    loop {
        let mut bytes = [0u8; 32];
        if getrandom::fill(&mut bytes).is_ok()
            && let Ok(pair) = KeyPair::from_bytes(&bytes)
        {
            return pair;
        }
    }
}

pub fn session_config(client_id: &str, timestamps: TimestampMode, max_write: usize) -> ZxConfig {
    ZxConfig {
        client_id: client_id.to_string(),
        keys: new_key_pair(),
        timestamps,
        max_write,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn client_id_shape() {
        let id = super::new_client_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
    }
}

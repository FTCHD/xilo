use xilo_proto::{Flavor, KeyPair, TimestampMode, ZxConfig};

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

/// How to open a session with one device.
#[derive(Clone, Debug)]
pub struct SessionOptions {
    pub flavor: Flavor,
    pub probe: bool,
    pub timezone: String,
    pub country: String,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            flavor: Flavor::Zx,
            probe: false,
            timezone: "UTC0".into(),
            country: "US".into(),
        }
    }
}

pub fn session_config(
    client_id: &str,
    timestamps: TimestampMode,
    max_write: usize,
    options: SessionOptions,
) -> ZxConfig {
    ZxConfig {
        client_id: client_id.to_string(),
        keys: new_key_pair(),
        timestamps,
        max_write,
        flavor: options.flavor,
        probe: options.probe,
        timezone: options.timezone,
        country: options.country,
    }
}

/// A POSIX time zone with a fixed offset, for example `<+01>-1`.
pub fn posix_timezone(offset_seconds: i32) -> String {
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let abs = offset_seconds.unsigned_abs();
    let (h, m) = (abs / 3600, abs % 3600 / 60);
    let name = if m == 0 {
        format!("{sign}{h:02}")
    } else {
        format!("{sign}{h:02}{m:02}")
    };
    let posix_sign = if offset_seconds < 0 { "" } else { "-" };
    let value = if m == 0 {
        format!("{h}")
    } else {
        format!("{h}:{m:02}")
    };
    if offset_seconds == 0 {
        return "UTC0".into();
    }
    format!("<{name}>{posix_sign}{value}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn posix_zones() {
        assert_eq!(super::posix_timezone(0), "UTC0");
        assert_eq!(super::posix_timezone(3600), "<+01>-1");
        assert_eq!(super::posix_timezone(-5 * 3600), "<-05>5");
        assert_eq!(super::posix_timezone(19800), "<+0530>-5:30");
    }

    #[test]
    fn client_id_shape() {
        let id = super::new_client_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
    }
}

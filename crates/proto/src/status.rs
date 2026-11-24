#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ok,
    Fail,
    AccountRejected,
    TimestampRejected,
    ParameterRejected,
    NoMoreData,
    LowPower,
    NeedAuthentication,
    WrongPassword,
    PasswordRetriesExceeded,
    WifiError(u8),
    ChecksumFailed,
    FirmwareRejected,
    Other(u8),
}

impl Status {
    pub fn from_code(code: u8) -> Self {
        match code {
            0x00 => Status::Ok,
            0x01 => Status::Fail,
            0x02 => Status::AccountRejected,
            0x03 => Status::TimestampRejected,
            0x04 => Status::ParameterRejected,
            0x05 => Status::NoMoreData,
            0x06 => Status::LowPower,
            0x09 => Status::NeedAuthentication,
            0x0a => Status::WrongPassword,
            0x0b => Status::PasswordRetriesExceeded,
            0x21..=0x25 => Status::WifiError(code),
            0x71 => Status::ChecksumFailed,
            0x72 => Status::FirmwareRejected,
            other => Status::Other(other),
        }
    }

    pub fn description(self) -> String {
        match self {
            Status::Ok => "success".into(),
            Status::Fail => "the device reported a failure".into(),
            Status::AccountRejected => "the device rejected the account".into(),
            Status::TimestampRejected => "the device rejected the timestamp".into(),
            Status::ParameterRejected => "the device rejected the value".into(),
            Status::NoMoreData => "no more data".into(),
            Status::LowPower => "the battery is too low".into(),
            Status::NeedAuthentication => "the device needs authentication".into(),
            Status::WrongPassword => "wrong password".into(),
            Status::PasswordRetriesExceeded => "too many password attempts".into(),
            Status::WifiError(code) => format!("Wi-Fi error {code:#04x}"),
            Status::ChecksumFailed => "checksum verification failed".into(),
            Status::FirmwareRejected => "firmware verification failed".into(),
            Status::Other(code) => format!("status {code:#04x}"),
        }
    }
}

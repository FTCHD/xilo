/// A set of Bluetooth service and characteristic UUIDs used by SOLIX devices.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct GattSet {
    pub name: &'static str,
    pub advertised: &'static str,
    pub service: &'static str,
    pub write: &'static str,
    pub notify: &'static str,
}

pub const G1: GattSet = GattSet {
    name: "g1",
    advertised: "0000ff09-0000-1000-8000-00805f9b34fb",
    service: "8c850001-0302-41c5-b46e-cf057c562025",
    write: "8c850002-0302-41c5-b46e-cf057c562025",
    notify: "8c850003-0302-41c5-b46e-cf057c562025",
};

pub const G2: GattSet = GattSet {
    name: "g2",
    advertised: "00002540-0000-1000-8000-00805f9b34fb",
    service: "25400001-4002-81c5-b46e-cf057c562025",
    write: "25400002-4002-81c5-b46e-cf057c562025",
    notify: "25400003-4002-81c5-b46e-cf057c562025",
};

pub const ALL: [GattSet; 2] = [G1, G2];

/// Advertised services that mark a SOLIX device.
pub fn advertised_services() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for g in ALL {
        if !out.contains(&g.advertised) {
            out.push(g.advertised);
        }
    }
    out
}

/// The set whose GATT service was found, compared case-insensitively.
pub fn by_service(uuid: &str) -> Option<GattSet> {
    ALL.into_iter()
        .find(|g| g.service.eq_ignore_ascii_case(uuid))
}

pub fn by_name(name: &str) -> Option<GattSet> {
    ALL.into_iter().find(|g| g.name == name)
}

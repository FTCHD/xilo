use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gpui_kit::*;

/// Folder with the extracted Anker assets, if any. `XILO_NO_VENDOR` hides them,
/// for screenshots that will be published.
pub fn vendor_dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        if std::env::var_os("XILO_NO_VENDOR").is_some() {
            return None;
        }
        let bundled = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("../Resources/vendor")));
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/vendor");
        [bundled, Some(repo)]
            .into_iter()
            .flatten()
            .find(|p| p.join("shared").is_dir())
            .and_then(|p| p.canonicalize().ok())
    })
    .as_deref()
}

/// Path of a vendor asset such as `a1723/hero_blue_on`, trying common extensions.
pub fn vendor(key: &str) -> Option<PathBuf> {
    let dir = vendor_dir()?;
    ["webp", "png"]
        .iter()
        .map(|ext| dir.join(format!("{key}.{ext}")))
        .find(|p| p.exists())
}

/// Registers MontForAnker when the vendor fonts exist. Returns the family.
pub fn load_fonts(cx: &mut App) -> Option<&'static str> {
    let dir = vendor_dir()?.join("fonts");
    let fonts: Vec<Cow<'static, [u8]>> = ["Regular", "SemiBold", "Bold"]
        .iter()
        .filter_map(|w| std::fs::read(dir.join(format!("MontForAnker-{w}.ttf"))).ok())
        .map(Cow::Owned)
        .collect();
    if fonts.is_empty() {
        return None;
    }
    cx.text_system().add_fonts(fonts).ok()?;
    Some("Mont For Anker")
}

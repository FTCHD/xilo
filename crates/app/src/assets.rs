use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::OnceLock;

use gpui_kit::*;
use rust_embed::RustEmbed;

/// The Anker product, port and screen images in `assets/vendor`, built into the app.
#[derive(RustEmbed)]
#[folder = "../../assets/vendor"]
#[include = "*.png"]
#[include = "*.webp"]
struct Vendor;

const PREFIX: &str = "vendor/";

/// The images in `assets/vendor` and the icons from gpui-kit.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        match path.strip_prefix(PREFIX) {
            Some(file) => Ok(Vendor::get(file).map(|f| f.data)),
            None => gpui_kit::assets::AllAssets.load(path),
        }
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::AllAssets.list(path)?;
        paths.extend(
            Vendor::iter()
                .map(|file| format!("{PREFIX}{file}"))
                .filter(|p| p.starts_with(path))
                .map(SharedString::from),
        );
        Ok(paths)
    }
}

fn files() -> &'static HashSet<String> {
    static FILES: OnceLock<HashSet<String>> = OnceLock::new();
    FILES.get_or_init(|| Vendor::iter().map(|f| f.into_owned()).collect())
}

/// The image for a key such as `a1723/hero_blue_on`, if the app has one.
pub fn vendor(key: &str) -> Option<SharedString> {
    ["webp", "png"]
        .iter()
        .map(|ext| format!("{key}.{ext}"))
        .find(|file| files().contains(file))
        .map(|file| SharedString::from(format!("{PREFIX}{file}")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_built_in_images() {
        assert!(super::vendor("a1723/hero_blue_on").is_some());
        assert!(super::vendor("ports/usb_c_output").is_some());
        assert!(super::vendor("a1723/missing").is_none());
    }
}

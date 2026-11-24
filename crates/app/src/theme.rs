use gpui_kit::component::{Theme, ThemeRegistry};
use gpui_kit::*;

const THEMES: &str = include_str!("../../../assets/builtin/themes/xilo.json");
const LIGHT: &str = "Xilo Light";
const DARK: &str = "Xilo Dark";

pub fn init(cx: &mut App) {
    ThemeRegistry::global_mut(cx)
        .load_themes_from_str(THEMES)
        .expect("built-in theme is valid");

    let themes = ThemeRegistry::global(cx).themes();
    let light = themes.get(LIGHT).cloned().expect("light theme registered");
    let dark = themes.get(DARK).cloned().expect("dark theme registered");

    let theme = Theme::global_mut(cx);
    theme.light_theme = light;
    theme.dark_theme = dark;

    Theme::sync_system_appearance(None, cx);
}

pub fn follow_system(window: &mut Window, cx: &mut App) {
    Theme::sync_system_appearance(Some(window), cx);
}

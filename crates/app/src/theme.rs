use gpui_kit::component::{Theme, ThemeMode, ThemeRegistry};
use gpui_kit::*;
use xilo_store::ThemeChoice;

const THEMES: &str = include_str!("../../../assets/builtin/themes/xilo.json");
const LIGHT: &str = "Xilo Light";
const DARK: &str = "Xilo Dark";

pub fn init(cx: &mut App, choice: ThemeChoice) {
    ThemeRegistry::global_mut(cx)
        .load_themes_from_str(THEMES)
        .expect("built-in theme is valid");

    let themes = ThemeRegistry::global(cx).themes();
    let light = themes.get(LIGHT).cloned().expect("light theme registered");
    let dark = themes.get(DARK).cloned().expect("dark theme registered");

    let theme = Theme::global_mut(cx);
    theme.light_theme = light;
    theme.dark_theme = dark;

    apply_mode(choice, None, cx);
}

fn apply_mode(choice: ThemeChoice, window: Option<&mut Window>, cx: &mut App) {
    match choice {
        ThemeChoice::System => Theme::sync_system_appearance(window, cx),
        ThemeChoice::Light => Theme::change(ThemeMode::Light, window, cx),
        ThemeChoice::Dark => Theme::change(ThemeMode::Dark, window, cx),
    }
}

pub fn apply(choice: ThemeChoice, window: &mut Window, cx: &mut App) {
    apply_mode(choice, Some(window), cx);
    window.refresh();
}

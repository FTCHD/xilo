mod shell;
mod theme;

use gpui_kit::component::TitleBar;
use gpui_kit::*;

actions!(xilo, [Quit]);

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        theme::init(cx);

        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.set_menus(vec![Menu {
            name: "Xilo".into(),
            items: vec![MenuItem::action("Quit Xilo", Quit)],
            disabled: false,
        }]);
        cx.set_quit_mode(QuitMode::LastWindowClosed);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1200.), px(800.)),
                cx,
            ))),
            window_min_size: Some(size(px(900.), px(600.))),
            ..TitleBar::window_options()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| shell::Root::new(window, cx))
        })
        .expect("failed to open window");
        cx.activate(true);
    });
}

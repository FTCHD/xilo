use gpui_kit::component::empty::{Empty, EmptyDescription, EmptyHeader, EmptyTitle};
use gpui_kit::component::label::Label;
use gpui_kit::component::{ActiveTheme as _, TitleBar, v_flex};
use gpui_kit::*;

use crate::theme;

pub struct Root {
    _appearance: Subscription,
}

impl Root {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            theme::follow_system(window, cx);
        });
        Self {
            _appearance: appearance,
        }
    }
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child(Label::new("Xilo")))
            .child(
                div().flex_1().flex().items_center().justify_center().child(
                    Empty::new().header(
                        EmptyHeader::new()
                            .title(EmptyTitle::new().child("No devices yet"))
                            .description(
                                EmptyDescription::new()
                                    .child("Your SOLIX devices will appear here."),
                            ),
                    ),
                ),
            )
    }
}

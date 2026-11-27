//! Compact jumps to the route groups that are currently visible in the page.
use super::{BLUE, FAINT, LINE, SOFT_BLUE, SemanticButton, line_icon};
use gpui_kit::component::{
    Sizable,
    button::Button,
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use meow_paw::i18n::{Locale, Message};
use unicode_segmentation::UnicodeSegmentation;

type GroupAnchor = (String, ScrollAnchor);

fn initial(name: &str) -> &str {
    name.trim_start().graphemes(true).next().unwrap_or("?")
}

fn rail_capacity(viewport_height: Pixels) -> usize {
    // Include a small gap between 28 px targets; keep the rest clear.
    (((viewport_height - px(65.)).as_f32() * 0.7 / 30.) as usize).max(1)
}

pub(super) fn rail(locale: Locale, viewport_height: Pixels, anchors: Vec<GroupAnchor>) -> Div {
    let capacity = rail_capacity(viewport_height);
    let use_menu = capacity < anchors.len();
    let initials = if use_menu {
        capacity - 1
    } else {
        anchors.len()
    };

    div()
        .flex()
        .flex_col()
        .justify_center()
        .items_center()
        .h_full()
        .w(px(32.))
        .flex_shrink_0()
        .pr_1()
        .gap(px(2.))
        .children(anchors.iter().take(initials).map(|(name, anchor)| {
            let hint = name.clone();
            let anchor = anchor.clone();
            div()
                .id(SharedString::from(format!("group-rail-hint:{name}")))
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(hint.clone()).build(window, cx)
                })
                .child(
                    SemanticButton::new(SharedString::from(format!("jump-group:{name}")))
                        .accessibility_label(format!(
                            "{}: {name}",
                            locale.message(Message::JumpToGroup)
                        ))
                        .size(px(28.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(LINE).opacity(0.))
                        .text_xs()
                        .text_color(rgb(FAINT))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(SOFT_BLUE)).text_color(rgb(BLUE)))
                        .focus_visible(|s| {
                            s.bg(rgb(SOFT_BLUE))
                                .border_color(rgb(BLUE))
                                .text_color(rgb(BLUE))
                        })
                        .child(initial(name).to_owned())
                        .on_click(move |event, window, cx| {
                            if matches!(event, ClickEvent::Keyboard(_)) {
                                cx.stop_propagation();
                            }
                            anchor.scroll_to(window, cx);
                        }),
                )
        }))
        .when(use_menu, |rail| rail.child(menu(locale, anchors, true)))
}

pub(super) fn menu(locale: Locale, anchors: Vec<GroupAnchor>, icon_only: bool) -> impl IntoElement {
    let label = locale.message(Message::JumpToGroup);
    Button::new("proxy-jump-menu")
        .accessibility_label(label)
        .small()
        .when(icon_only, |button| {
            button
                .size(px(28.))
                .child(line_icon("icons/list-filter.svg", FAINT, 16.))
                .tooltip(label)
        })
        .when(!icon_only, |button| button.label(label))
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
            anchors.iter().fold(
                menu.max_h(px(280.)).max_w(px(310.)).scrollable(true),
                |menu, (name, anchor)| {
                    let anchor = anchor.clone();
                    menu.item(
                        PopupMenuItem::new(name.clone()).on_click(move |_, window, cx| {
                            anchor.scroll_to(window, cx);
                        }),
                    )
                },
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{initial, rail_capacity};
    use gpui_kit::px;

    #[test]
    fn initials_keep_unicode_graphemes_and_ignore_leading_spaces() {
        assert_eq!(initial(" Daily Proxy"), "D");
        assert_eq!(initial("香港"), "香");
        assert_eq!(initial("🇭🇰 Hong Kong"), "🇭🇰");
        assert_eq!(initial("e\u{301}toile"), "e\u{301}");
        assert_eq!(initial(""), "?");
    }

    #[test]
    fn overflow_moves_to_the_menu_without_creating_offscreen_tab_stops() {
        assert_eq!(rail_capacity(px(738.)), 15);
        assert_eq!(rail_capacity(px(480.)), 9);
        assert_eq!(rail_capacity(px(65.)), 1);
    }
}

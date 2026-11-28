use super::{Locale, Message, Page, open, px};
use gpui_kit::{TestAppContext, test::TestWindowExt};
extern crate gpui_kit as gpui;

#[gpui_kit::test]
fn home_layout_keeps_meaningful_cards_and_rates_above_the_desktop_fold(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    for locale in [Locale::En, Locale::ZhCn] {
        let (_, ui) = cx.add_window_view(move |window, cx| {
            let paw = open(
                None,
                None,
                (Page::Overview, locale),
                false,
                (crate::preferences::Preferences::discover(false).0, None),
                window,
                cx,
            );
            gpui_kit::base::Root::new(paw, window, cx)
        });
        for (width, height, columns) in [(1120., 738., true), (760., 1050., false)] {
            ui.simulate_resize(gpui_kit::size(px(width), px(height)));
            ui.update(|window, cx| {
                window.activate_window();
                window.render_frame(cx);
                let layout = |id| window.find(id).bounds();
                let profile = layout("home-profile-card");
                let route = layout("home-route-card");
                let network = layout("home-network-card");
                let mode = layout("home-mode-card");
                let traffic = layout("home-traffic-card");
                let rates = layout("home-traffic-rates");

                assert_eq!(profile.origin.x, network.origin.x);
                assert!(network.origin.y >= profile.origin.y + profile.size.height);
                if columns {
                    assert_eq!(profile.origin.y, route.origin.y);
                    assert_eq!(network.origin.y, mode.origin.y);
                    assert_eq!(route.origin.x, mode.origin.x);
                    assert!(route.origin.x >= profile.origin.x + profile.size.width);
                    assert!(rates.origin.y + rates.size.height <= px(height));
                } else {
                    assert_eq!(route.origin.x, profile.origin.x);
                    assert_eq!(mode.origin.x, profile.origin.x);
                    assert!(route.origin.y >= profile.origin.y + profile.size.height);
                    assert!(network.origin.y >= route.origin.y + route.size.height);
                    assert!(mode.origin.y >= network.origin.y + network.size.height);
                }
                assert!(traffic.origin.y >= network.origin.y + network.size.height);
                assert!(traffic.origin.y >= mode.origin.y + mode.size.height);
                assert!(traffic.origin.x + traffic.size.width <= px(width));
                for (id, label) in [
                    ("home-mode-rule", Message::ModeRule),
                    ("home-mode-global", Message::ModeGlobal),
                    ("home-mode-direct", Message::ModeDirect),
                ] {
                    let button = window.find(id);
                    assert_eq!(button.label(), Some(locale.message(label)));
                    assert_eq!(button.disabled(), Some(true));
                }
            });
        }
    }
}

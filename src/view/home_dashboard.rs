//! Responsive Home layout. Its controls continue to use the managed session.
use super::*;

const GAP: f32 = 12.;
const MIN_COLUMN: f32 = 344.;
const PAGE_INSET: f32 = 16.;

pub(super) fn sidebar_width(compact: bool) -> Pixels {
    px(if compact { 62. } else { 204. })
}

pub(super) struct Layout {
    columns: bool,
}

impl Layout {
    pub(super) fn from_viewport(viewport: Pixels, compact: bool) -> Self {
        let available = viewport - sidebar_width(compact) - px(PAGE_INSET * 2.);
        Self {
            // Do not briefly enable columns in the icon rail before the
            // wide sidebar appears at 820 px and takes that space back.
            columns: !compact && available >= px(MIN_COLUMN * 2. + GAP),
        }
    }

    pub(super) fn row(&self, first: Stateful<Div>, second: Stateful<Div>) -> Div {
        let column =
            |body: Stateful<Div>| body.min_w_0().w_full().when(self.columns, |el| el.flex_1());
        div()
            .flex()
            .gap(px(GAP))
            .when(!self.columns, |row| row.flex_col())
            .child(column(first).test_support())
            .child(column(second).test_support())
    }
}

pub(super) fn card(id: &'static str, icon: &'static str, title: &'static str) -> Stateful<Div> {
    super::card().id(id).p_0().child(
        div()
            .px_4()
            .pt_3()
            .pb_2()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .size(px(32.))
                    .rounded_md()
                    .bg(rgb(SOFT_BLUE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(line_icon(icon, BLUE, 17.)),
            )
            .child(div().font_weight(FontWeight::BOLD).child(title)),
    )
}

#[cfg(any(target_os = "linux", windows))]
type Mode = meow_paw::engine::controller::RoutingMode;
#[cfg(any(target_os = "linux", windows))]
type ModeReceipt = (u64, u64, Mode);

#[cfg(any(target_os = "linux", windows))]
fn mode_receipt(snapshot: &meow_paw::app::Snapshot) -> Option<ModeReceipt> {
    use meow_paw::app::EngineStatus;
    if !matches!(snapshot.engine, EngineStatus::Running { .. })
        || snapshot.apply_pending()
        || snapshot.inventory_stale
    {
        return None;
    }
    Some((
        snapshot.probe_epoch,
        snapshot.selector_revision,
        snapshot.mode?,
    ))
}

impl Paw {
    pub(super) fn home_mode(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let panel = card(
            "home-mode-card",
            Page::Rules.icon(),
            self.locale.message(Message::OverviewProxyMode),
        );
        #[cfg(any(target_os = "linux", windows))]
        {
            let receipt = self
                .desktop
                .as_ref()
                .filter(|_| !self.quit_pending())
                .and_then(|_| mode_receipt(&self.snapshot));
            let active = receipt.map(|(_, _, mode)| mode);
            let hints = match active {
                Some(Mode::Rule) => Message::OverviewProxyModeRule,
                Some(Mode::Global) => Message::OverviewProxyModeGlobal,
                Some(Mode::Direct) => Message::OverviewProxyModeDirect,
                None => Message::OverviewProxyModeWaiting,
            };
            let choices = Mode::ALL.into_iter().map(|mode| {
                let (id, label) = match mode {
                    Mode::Rule => ("home-mode-rule", Message::ModeRule),
                    Mode::Global => ("home-mode-global", Message::ModeGlobal),
                    Mode::Direct => ("home-mode-direct", Message::ModeDirect),
                };
                let selected = active == Some(mode);
                let label = self.locale.message(label);
                SemanticButton::new(id)
                    .aria_toggled(if selected {
                        gpui_kit::accesskit::Toggled::True
                    } else {
                        gpui_kit::accesskit::Toggled::False
                    })
                    .accessibility_label(label)
                    .disabled(receipt.is_none())
                    .rounded_md()
                    .px_3()
                    .py_2()
                    .border_1()
                    .border_color(rgb(if selected { BLUE } else { LINE }))
                    .bg(rgb(if selected { BLUE } else { BG }))
                    .text_color(rgb(if selected {
                        SURFACE
                    } else if receipt.is_none() {
                        FAINT
                    } else {
                        INK
                    }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label)
                    .focus_visible(|style| style.border_color(rgb(0x3159a1)))
                    .when(receipt.is_some() && !selected, |button| {
                        button
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(SOFT_BLUE)))
                    })
                    .on_click(cx.listener(move |this, event, _, cx| {
                        if matches!(event, ClickEvent::Keyboard(_)) {
                            cx.stop_propagation();
                        }
                        if let Some(receipt) = receipt
                            && receipt.2 != mode
                            && !this.quit_pending()
                            && let Some(desktop) = &this.desktop
                            && mode_receipt(&desktop.snapshot()) == Some(receipt)
                        {
                            this.error = desktop.set_mode(mode).err().map(|err| err.to_string());
                            cx.notify();
                        }
                    }))
            });
            panel.child(
                div()
                    .px_4()
                    .pb_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().flex().flex_wrap().gap_2().children(choices))
                    .child(
                        div()
                            .bg(rgb(BG))
                            .rounded_md()
                            .px_3()
                            .py_2()
                            .text_xs()
                            .text_color(rgb(FAINT))
                            .child(self.locale.message(hints)),
                    ),
            )
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = cx;
            panel.child(
                div()
                    .px_4()
                    .pb_3()
                    .text_sm()
                    .child(self.locale.message(Message::ProxyDesktopUnsupported)),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Layout, px};
    #[cfg(any(target_os = "linux", windows))]
    use super::{Mode, mode_receipt};

    #[test]
    fn grid_does_not_toggle_at_the_sidebar_breakpoint() {
        for (width, compact, columns) in [
            (760., true, false),
            (819., true, false),
            (820., false, false),
            (935., false, false),
            (936., false, true),
            (1120., false, true),
        ] {
            assert_eq!(
                Layout::from_viewport(px(width), compact).columns,
                columns,
                "viewport {width}"
            );
        }
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn mode_requires_a_current_running_session() {
        use meow_paw::app::{EngineStatus, Snapshot};

        let mut snapshot = Snapshot::default();
        snapshot.mode = Some(Mode::Rule);
        assert!(mode_receipt(&snapshot).is_none());
        snapshot.engine = EngineStatus::Running { mixed_port: 17890 };
        assert_eq!(mode_receipt(&snapshot), Some((0, 0, Mode::Rule)));

        snapshot.inventory_stale = true;
        assert!(mode_receipt(&snapshot).is_none());
        snapshot.inventory_stale = false;
        snapshot.applying_profile = Some("Apply.yaml".into());
        assert!(mode_receipt(&snapshot).is_none());
        snapshot.applying_profile = None;
        snapshot.probe_epoch = 5;
        snapshot.selector_revision = 7;
        snapshot.mode = Some(Mode::Global);
        assert_eq!(mode_receipt(&snapshot), Some((5, 7, Mode::Global)));
        snapshot.mode = None;
        assert!(mode_receipt(&snapshot).is_none());
    }
}

#[cfg(all(test, feature = "visual-test", target_os = "linux"))]
#[path = "home_dashboard_ui_tests.rs"]
mod ui_tests;

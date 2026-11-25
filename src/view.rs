use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::PathBuf;
use std::time::Duration;

const BG: u32 = 0xeff6ff;
const SURFACE: u32 = 0xfafcff;
const INK: u32 = 0x10243d;
const FAINT: u32 = 0x536783;
const LINE: u32 = 0xdce7f6;
const BLUE: u32 = 0x2d62db;
const SOFT_BLUE: u32 = 0xe7efff;
const YELLOW: u32 = 0xffdd78;
const STREAM_DOWN: u32 = 0x2c77d2;
const STREAM_UP: u32 = 0xb97019;
#[cfg(any(target_os = "linux", windows))]
const GREEN: u32 = 0x168460;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Proxies,
    Profiles,
    Connections,
    Settings,
}

impl Page {
    const ALL: [Self; 5] = [
        Self::Overview,
        Self::Proxies,
        Self::Profiles,
        Self::Connections,
        Self::Settings,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.title().eq_ignore_ascii_case(name))
    }
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Proxies => "Proxies",
            Self::Profiles => "Profiles",
            Self::Connections => "Connections",
            Self::Settings => "Settings",
        }
    }
    fn mark(self) -> &'static str {
        match self {
            Self::Overview => "◈",
            Self::Proxies => "↔",
            Self::Profiles => "▤",
            Self::Connections => "◎",
            Self::Settings => "⚙",
        }
    }
    fn subtitle(self) -> &'static str {
        match self {
            Self::Overview => "Your proxy session at a glance.",
            Self::Proxies => "Inspect groups and pick the route to use.",
            Self::Profiles => "A local YAML file powers each isolated session.",
            Self::Connections => "See the requests currently handled by Meow.",
            Self::Settings => "Review the boundaries of this desktop build.",
        }
    }
}

pub struct Paw {
    page: Page,
    profile: Option<PathBuf>,
    binary: Option<PathBuf>,
    error: Option<String>,
    #[cfg(any(target_os = "linux", windows))]
    desktop: Option<meow_paw::app::Desktop>,
    #[cfg(any(target_os = "linux", windows))]
    snapshot: meow_paw::app::Snapshot,
}

pub fn open(
    binary: Option<PathBuf>,
    profile: Option<PathBuf>,
    page: Page,
    start: bool,
    cx: &mut App,
) -> Entity<Paw> {
    cx.new(|cx| {
        let mut paw = Paw {
            page,
            profile,
            #[cfg(any(target_os = "linux", windows))]
            desktop: binary
                .as_ref()
                .and_then(|path| meow_paw::app::Desktop::new(path.clone()).ok()),
            binary,
            error: None,
            #[cfg(any(target_os = "linux", windows))]
            snapshot: Default::default(),
        };
        if start {
            paw.start();
        }
        cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                if weak
                    .update(cx, |paw: &mut Paw, cx| {
                        if paw.refresh() {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        paw
    })
}

impl Paw {
    fn start(&mut self) {
        self.error = None;
        #[cfg(any(target_os = "linux", windows))]
        {
            let Some(desktop) = self.desktop.as_ref() else {
                self.error =
                    Some("Set MEOW_BIN to an executable Meow binary before starting Paw.".into());
                return;
            };
            let Some(profile) = self.profile.clone() else {
                self.error = Some("Start Paw with --profile PATH to select a YAML file.".into());
                return;
            };
            self.error = desktop.start(profile).err().map(|e| e.to_string());
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            self.error =
                Some("Managed engine sessions are not enabled on this platform yet.".into());
        }
    }
    fn stop(&mut self) {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(desktop) = &self.desktop {
            desktop.stop();
        }
    }

    fn refresh(&mut self) -> bool {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(desktop) = &self.desktop {
            let snap = desktop.snapshot();
            if self.snapshot.revision != snap.revision {
                self.snapshot = snap;
                return true;
            }
        }
        false
    }

    fn state(&self) -> (&'static str, u32, Option<u16>) {
        #[cfg(any(target_os = "linux", windows))]
        {
            use meow_paw::app::EngineStatus;
            match &self.snapshot.engine {
                EngineStatus::Running { mixed_port } => ("Running", GREEN, Some(*mixed_port)),
                EngineStatus::Starting => ("Starting", BLUE, None),
                EngineStatus::Stopping => ("Stopping", BLUE, None),
                EngineStatus::Error(_) => ("Needs attention", 0xa83a33, None),
                EngineStatus::Stopped => ("Stopped", FAINT, None),
            }
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            ("Not enabled", FAINT, None)
        }
    }

    fn issue(&self) -> Option<String> {
        #[cfg(any(target_os = "linux", windows))]
        {
            if let meow_paw::app::EngineStatus::Error(error) = &self.snapshot.engine {
                return Some(error.clone());
            }
            if self.snapshot.action_error.is_some() {
                return self.snapshot.action_error.clone();
            }
        }
        self.error.clone()
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> Div {
        let nav = Self::pages(self.page, cx);
        div()
            .w(px(216.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .px_5()
            .py_6()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(paw_badge(40.0, YELLOW, INK))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::BOLD)
                                    .child("Meow Paw"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(FAINT))
                                    .child("DESKTOP CLIENT"),
                            ),
                    ),
            )
            .child(
                div()
                    .mt_8()
                    .mb_2()
                    .text_xs()
                    .text_color(rgb(FAINT))
                    .child("WORKSPACE"),
            )
            .children(nav)
            .child(div().flex_grow(1.0))
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .pt_4()
                    .text_xs()
                    .text_color(rgb(FAINT))
                    .child(format!("Paw 0.1  ·  {}", std::env::consts::OS)),
            )
    }

    fn pages(active: Page, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        Page::ALL
            .into_iter()
            .enumerate()
            .map(|(index, page)| {
                let on = active == page;
                div()
                    .id(("paw-nav", index))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .my_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(rgb(if on { SOFT_BLUE } else { SURFACE }))
                    .text_color(rgb(if on { BLUE } else { INK }))
                    .hover(|s| s.bg(rgb(SOFT_BLUE)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.page = page;
                        cx.notify();
                    }))
                    .child(div().w_5().text_lg().child(page.mark()))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(page.title()))
            })
            .collect()
    }

    fn actions(&self, cx: &mut Context<Self>) -> Div {
        let (phase, _, _) = self.state();
        let enabled = self.desktop_available();
        let running = phase == "Running" || phase == "Starting";
        let action = div()
            .id("session-control")
            .rounded_md()
            .px_4()
            .py_2()
            .bg(rgb(if enabled { BLUE } else { LINE }))
            .text_color(rgb(if enabled { 0xffffff } else { FAINT }))
            .font_weight(FontWeight::SEMIBOLD)
            .child(if running {
                "Stop session"
            } else {
                "Start session"
            })
            .when(enabled, |e| {
                e.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if running {
                            this.stop();
                        } else {
                            this.start();
                        }
                        cx.notify();
                    }))
            });
        div().child(action)
    }

    fn desktop_available(&self) -> bool {
        #[cfg(any(target_os = "linux", windows))]
        {
            self.desktop.is_some()
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            false
        }
    }

    fn overview(&self) -> Div {
        let (phase, color, port) = self.state();
        let (up, down, total) = self.traffic();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(0xf6e5b3))
                    .bg(rgb(0xfff8e2))
                    .px_6()
                    .py_5()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_5()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0x76571b))
                                    .child("LOCAL SESSION"),
                            )
                            .child(div().text_2xl().font_weight(FontWeight::BOLD).child(phase))
                            .child(div().text_sm().text_color(rgb(FAINT)).child(
                                port.map_or("No local listener is active.".into(), |port| {
                                    format!("Listening on 127.0.0.1:{port}")
                                }),
                            )),
                    )
                    .child(paw_badge(56.0, 0xffefbd, color)),
            )
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(metric("UPLOAD", up))
                    .child(metric("DOWNLOAD", down))
                    .child(metric("SESSION TOTAL", total)),
            )
            .child(self.traffic_timeline())
            .child(panel("Profile", self.profile_label()))
            .child(panel(
                "System proxy",
                "Unchanged. Use the local listener explicitly.".into(),
            ))
    }

    fn traffic_timeline(&self) -> Div {
        let label = div()
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Traffic activity"),
            )
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(traffic_legend("Download", STREAM_DOWN))
                    .child(traffic_legend("Upload", STREAM_UP)),
            );
        let frame = card().child(label);
        #[cfg(any(target_os = "linux", windows))]
        {
            use meow_paw::app::TRAFFIC_WINDOW_SECS;
            let samples: Vec<_> = self.snapshot.traffic_history.recent().collect();
            if samples.iter().any(Option::is_some) {
                let peak = samples
                    .iter()
                    .flatten()
                    .map(|s| s.up.max(s.down))
                    .max()
                    .unwrap_or(0);
                let pad = TRAFFIC_WINDOW_SECS.saturating_sub(samples.len());
                let columns = std::iter::repeat_n(None, pad).chain(samples).map(|sample| {
                    let (up, down) = sample.map_or((0., 0.), |s| {
                        (traffic_bar(s.up, peak), traffic_bar(s.down, peak))
                    });
                    div()
                        .flex_1()
                        .flex()
                        .items_end()
                        .gap(px(1.))
                        .h_full()
                        .child(div().flex_1().rounded_sm().h(px(down)).bg(rgb(STREAM_DOWN)))
                        .child(div().flex_1().rounded_sm().h(px(up)).bg(rgb(STREAM_UP)))
                });
                return frame
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .mt_3()
                            .text_xs()
                            .text_color(rgb(FAINT))
                            .child("Shared scale")
                            .child(format!("{}/s peak", bytes(peak))),
                    )
                    .child(
                        div()
                            .mt_1()
                            .h(px(61.))
                            .flex()
                            .items_end()
                            .gap(px(2.))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .children(columns),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .mt_2()
                            .text_xs()
                            .text_color(rgb(FAINT))
                            .child("39 seconds ago")
                            .child("Now"),
                    );
            }
        }
        frame.child(
            div()
                .mt_4()
                .h(px(70.))
                .rounded_lg()
                .bg(rgb(SOFT_BLUE))
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(rgb(FAINT))
                .child("Live activity appears when a session sends its first sample."),
        )
    }

    fn traffic(&self) -> (String, String, String) {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(t) = self.snapshot.traffic.as_ref() {
            return (
                format!("{}/s", bytes(t.up)),
                format!("{}/s", bytes(t.down)),
                bytes(t.up_total.saturating_add(t.down_total)),
            );
        }
        ("—".into(), "—".into(), "—".into())
    }

    fn profile_label(&self) -> String {
        self.profile
            .as_ref()
            .map(|p| {
                p.file_name()
                    .unwrap_or(p.as_os_str())
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_else(|| "No profile selected".into())
    }

    fn profiles(&self, cx: &mut Context<Self>) -> Div {
        let box_ = card()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Local YAML"),
            )
            .child(
                div()
                    .mt_2()
                    .text_color(rgb(FAINT))
                    .child(self.profile_label()),
            )
            .child(
                div().mt_4().child(
                    div()
                        .id("select-profile")
                        .rounded_md()
                        .bg(rgb(SOFT_BLUE))
                        .text_color(rgb(BLUE))
                        .cursor_pointer()
                        .px_4()
                        .py_2()
                        .child("Choose YAML file")
                        .on_click(cx.listener(|this, _, _, cx| this.choose_profile(cx))),
                ),
            );
        let page = div().flex().flex_col().gap_4().child(box_);
        #[cfg(any(target_os = "linux", windows))]
        if !self.snapshot.diagnostics.is_empty() {
            return page.child(
                card()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Profile diagnostics"),
                    )
                    .children(
                        self.snapshot
                            .diagnostics
                            .iter()
                            .take(40)
                            .map(|d| div().mt_1().text_sm().child(format!("{d:?}"))),
                    ),
            );
        }
        page
    }

    fn choose_profile(&mut self, cx: &mut Context<Self>) {
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose Meow YAML".into()),
        });
        cx.spawn(async move |weak, cx| {
            let path = selection
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            if let Some(path) = path {
                let _ = weak.update(cx, |paw: &mut Paw, cx| {
                    paw.profile = Some(path);
                    paw.error = None;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn proxies(&self, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(inventory) = self.snapshot.inventory.as_ref() {
            let mut body = div().flex().flex_col().gap_4();
            for (name, group) in inventory.groups.iter().filter(|(_, g)| !g.hidden).take(24) {
                let title = format!("{name}  ·  {}", group.kind);
                let mut group_box =
                    card().child(div().font_weight(FontWeight::SEMIBOLD).child(title));
                if let Some(members) = &group.all {
                    for (index, member) in members.iter().take(48).enumerate() {
                        let (g, n) = (name.clone(), member.clone());
                        let selected = group.now.as_deref() == Some(member.as_str());
                        group_box = group_box.child(
                            div()
                                .id(SharedString::from(format!("proxy:{name}:{index}")))
                                .p_2()
                                .mt_1()
                                .rounded_md()
                                .cursor_pointer()
                                .bg(rgb(if selected { SOFT_BLUE } else { SURFACE }))
                                .child(format!("{} {member}", if selected { "●" } else { "○" }))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.error = this.desktop.as_ref().and_then(|d| {
                                        d.select(g.clone(), n.clone()).err().map(|e| e.to_string())
                                    });
                                    cx.notify();
                                })),
                        );
                    }
                }
                body = body.child(group_box);
            }
            return body;
        }
        let _ = cx;
        panel(
            "No proxy groups",
            "Start a local profile to see available selectors.".into(),
        )
    }

    fn connections(&self) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(list) = self.snapshot.connections.as_ref().filter(|c| !c.is_empty()) {
            return card()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("{} active", list.len())),
                )
                .children(list.iter().take(100).map(|c| {
                    let target = if c.metadata.host.is_empty() {
                        &c.metadata.destination_ip
                    } else {
                        &c.metadata.host
                    };
                    div()
                        .border_t_1()
                        .border_color(rgb(LINE))
                        .py_2()
                        .mt_2()
                        .child(div().font_weight(FontWeight::MEDIUM).child(target.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(FAINT))
                                .child(c.chains.join(" / ")),
                        )
                }));
        }
        panel(
            "No active connections",
            "Use Paw’s local listener to see live requests here.".into(),
        )
    }

    fn settings(&self) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(panel("Platform", std::env::consts::OS.into()))
            .child(panel(
                "Meow binary",
                self.binary
                    .as_ref()
                    .map(|p| {
                        p.file_name()
                            .unwrap_or(p.as_os_str())
                            .to_string_lossy()
                            .into_owned()
                    })
                    .unwrap_or_else(|| "Not configured. Set MEOW_BIN before opening Paw.".into()),
            ))
            .child(panel(
                "System changes",
                "No system proxy changes. No TUN or administrator privileges.".into(),
            ))
    }
}

impl Render for Paw {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.page {
            Page::Overview => self.overview(),
            Page::Proxies => self.proxies(cx),
            Page::Profiles => self.profiles(cx),
            Page::Connections => self.connections(),
            Page::Settings => self.settings(),
        };
        div()
            .size_full()
            .flex()
            .bg(rgb(BG))
            .text_color(rgb(INK))
            .child(self.sidebar(cx))
            .child(
                div()
                    .id("paw-page")
                    .size_full()
                    .flex_1()
                    .overflow_y_scroll()
                    .px_8()
                    .py_7()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .mb_6()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_2xl()
                                            .font_weight(FontWeight::BOLD)
                                            .child(self.page.title()),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(FAINT))
                                            .child(self.page.subtitle()),
                                    ),
                            )
                            .child(self.actions(cx)),
                    )
                    .when_some(self.issue(), |e, issue| {
                        e.child(
                            div()
                                .p_3()
                                .mb_4()
                                .rounded_md()
                                .bg(rgb(0xffece9))
                                .text_color(rgb(0xa83a33))
                                .child(issue),
                        )
                    })
                    .child(body),
            )
    }
}

fn card() -> Div {
    div()
        .rounded_xl()
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(SURFACE))
        .p_5()
}

fn panel(title: &'static str, message: String) -> Div {
    card()
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
        .child(div().mt_2().text_sm().text_color(rgb(FAINT)).child(message))
}

fn metric(label: &'static str, value: String) -> Div {
    card()
        .flex_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(FAINT))
                .child(label),
        )
        .child(
            div()
                .mt_2()
                .text_xl()
                .font_weight(FontWeight::BOLD)
                .child(value),
        )
}

fn traffic_legend(label: &'static str, color: u32) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_xs()
        .text_color(rgb(FAINT))
        .child(div().size(px(7.)).rounded_full().bg(rgb(color)))
        .child(label)
}

#[cfg(any(target_os = "linux", windows))]
fn traffic_bar(value: u64, peak: u64) -> f32 {
    if peak == 0 || value == 0 {
        return 0.;
    }
    (60. * (value.min(peak) as f64 / peak as f64) as f32).max(1.)
}

#[cfg(any(target_os = "linux", windows))]
fn bytes(value: u64) -> String {
    if value < 1024 {
        return format!("{value} B");
    }
    let mut amount = value as f64 / 1024.;
    for unit in ["KiB", "MiB", "GiB", "TiB"] {
        if amount < 1024. || unit == "TiB" {
            return format!("{amount:.1} {unit}");
        }
        amount /= 1024.;
    }
    unreachable!()
}

// A native geometric mark, independent of emoji fonts and platform glyph widths.
fn paw_badge(size: f32, background: u32, foreground: u32) -> Div {
    let dot = |x: f32, y: f32, w: f32, h: f32| {
        div()
            .absolute()
            .left(px(x * size))
            .top(px(y * size))
            .w(px(w * size))
            .h(px(h * size))
            .rounded_full()
            .bg(rgb(foreground))
    };
    div()
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .rounded_xl()
        .bg(rgb(background))
        .child(dot(0.220, 0.315, 0.120, 0.145))
        .child(dot(0.360, 0.210, 0.125, 0.170))
        .child(dot(0.515, 0.210, 0.125, 0.170))
        .child(dot(0.665, 0.315, 0.120, 0.145))
        .child(dot(0.312, 0.446, 0.376, 0.295))
}

#[cfg(test)]
mod tests {
    use super::Page;
    #[test]
    fn startup_pages_match_the_nav() {
        assert_eq!(Page::ALL.len(), 5);
        for expected in Page::ALL {
            assert!(Page::from_name(expected.title()) == Some(expected));
        }
        assert!(Page::from_name("not-a-page").is_none());
    }
}

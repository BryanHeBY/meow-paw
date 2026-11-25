use gpui_kit::{App, Application, Bounds, WindowBounds, WindowOptions, px, size};
use std::path::PathBuf;

mod view;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let mut profile = None;
    let mut page = view::Page::Overview;
    let mut start = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--profile") => profile = args.next().map(PathBuf::from),
            Some("--page") => {
                if let Some(name) = args.next() {
                    page = view::Page::from_name(&name.to_string_lossy()).unwrap_or(page);
                }
            }
            Some("--start") => start = true,
            _ => {
                eprintln!("Usage: meow-paw [--profile FILE] [--start] [--page PAGE]");
                std::process::exit(2);
            }
        }
    }
    let binary = std::env::var_os("MEOW_BIN").map(PathBuf::from);
    let app: Application = gpui_kit::application();
    app.run(move |cx: &mut App| {
        gpui_kit::init(cx);
        let bounds = Bounds::centered(None, size(px(1120.), px(738.)), cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                window.set_window_title("Meow Paw");
                view::open(binary, profile, page, start, cx)
            },
        )
        .expect("could not open Meow Paw");
        cx.activate(true);
    });
}

use gpui_kit::{App, Application, Bounds, WindowBounds, WindowOptions, px, size};
use std::path::PathBuf;

mod view;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let mut profile = None;
    let mut page = view::Page::Overview;
    let mut start = false;
    #[cfg(feature = "visual-test")]
    let mut screenshot = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--profile") => profile = args.next().map(PathBuf::from),
            Some("--page") => {
                if let Some(name) = args.next() {
                    page = view::Page::from_name(&name.to_string_lossy()).unwrap_or(page);
                }
            }
            Some("--start") => start = true,
            #[cfg(feature = "visual-test")]
            Some("--screenshot") => screenshot = args.next().map(PathBuf::from),
            _ => {
                eprintln!("Usage: meow-paw [--profile FILE] [--start] [--page PAGE]");
                std::process::exit(2);
            }
        }
    }
    let binary = std::env::var_os("MEOW_BIN").map(PathBuf::from);
    let app: Application = gpui_kit::application().with_assets(view::PawAssets);
    app.run(move |cx: &mut App| {
        gpui_kit::init(cx);
        let bounds = Bounds::centered(None, size(px(1120.), px(738.)), cx);
        let (_handle, _) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(640.), px(480.))),
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
        #[cfg(feature = "visual-test")]
        if let Some(path) = screenshot {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1_400))
                    .await;
                let rendered = _handle.update(cx, |_, window, _| {
                    window
                        .render_to_image()
                        .and_then(|image| image.save(&path).map_err(Into::into))
                });
                if !matches!(rendered, Ok(Ok(()))) {
                    eprintln!("Paw visual capture failed: {rendered:?}");
                    std::process::exit(1);
                }
                cx.update(|app| app.quit());
            })
            .detach();
        }
    });
}

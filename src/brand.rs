//! One original color mark for the window chrome and platform trays.
use gpui_kit::{AssetSource, Result, SharedString};
use std::borrow::Cow;

gpui_kit::assets::icon_assets!(
    NavAssets,
    [
        House,
        Wifi,
        Server,
        Globe,
        GitBranch,
        List,
        LockOpen,
        Settings,
        ChevronDown,
        ChevronRight,
        CircleCheck,
        ArrowUpRight,
        ArrowDownLeft,
    ]
);

pub(crate) const MARK: &str = "brand/paw-mark.png";

pub(crate) struct PawAssets;

impl AssetSource for PawAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == MARK {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/paw-mark.png"
            ))));
        }
        NavAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = NavAssets.list(path)?;
        if MARK.starts_with(path) {
            assets.push(MARK.into());
        }
        Ok(assets)
    }
}

#[cfg(any(target_os = "linux", windows))]
pub(crate) fn tray_pixel(x: i32, y: i32, size: i32) -> [u8; 4] {
    let pixels: &[u8] = match size {
        16 => include_bytes!("../assets/paw-mark-16.rgba"),
        24 => include_bytes!("../assets/paw-mark-24.rgba"),
        32 => include_bytes!("../assets/paw-mark-32.rgba"),
        _ => unreachable!("unsupported tray icon size"),
    };
    let offset = ((y * size + x) * 4) as usize;
    [
        pixels[offset + 3],
        pixels[offset],
        pixels[offset + 1],
        pixels[offset + 2],
    ]
}

#[cfg(all(test, any(target_os = "linux", windows)))]
mod tests {
    #[test]
    fn tray_assets_have_transparent_corners_and_an_opaque_center() {
        for size in [16, 24, 32] {
            assert!(super::tray_pixel(0, 0, size)[0] <= 1);
            assert_eq!(super::tray_pixel(size / 2, size / 2, size)[0], 255);
        }
    }
}

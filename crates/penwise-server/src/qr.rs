//! Pairing QR: a code plus the server's address, as an app deep link the camera app can open.
//!
//! The payload is `penwise://pair?code=XXXXXX&url=http://<lan>:7420`. Scanning it with any phone or tablet opens
//! Penwise (on the device that has it installed) already pointed at this server; no custom camera screen needed.

use tiny_skia::Pixmap;

use crate::pairing;
use penwise_core::raster::png::encode_png;

/// Pixels per QR module; the default keeps a version-3 code scannable from across a desk.
pub const DEFAULT_MODULE_PX: u32 = 8;
/// Quiet zone, in modules (the standard requires four).
const QUIET_MODULES: u32 = 4;
/// Largest module size accepted, so a request cannot allocate a huge bitmap.
const MAX_MODULE_PX: u32 = 32;

/// The deep link a pairing QR carries.
#[must_use]
pub fn payload(code: &str, url: &str) -> String {
    format!("penwise://pair?code={code}&url={url}")
}

/// Renders `payload` as a PNG.
#[allow(
    clippy::cast_precision_loss,
    reason = "module coordinates are small counts (a QR is at most 177 modules, at most 32 px each)"
)]
pub fn png(payload: &str, module_px: u32) -> Result<Vec<u8>, String> {
    let code = qrcode::QrCode::new(payload.as_bytes()).map_err(|error| error.to_string())?;
    let modules = u32::try_from(code.width()).map_err(|_| "qr too wide")?;
    let module_px = module_px.clamp(1, MAX_MODULE_PX);
    let side = (modules + QUIET_MODULES * 2) * module_px;
    let mut pixmap = Pixmap::new(side, side).ok_or("qr size out of range")?;
    pixmap.fill(tiny_skia::Color::WHITE);
    let dark = tiny_skia::Color::BLACK;
    let colors = code.to_colors();
    for (index, color) in colors.iter().enumerate() {
        if *color != qrcode::types::Color::Dark {
            continue;
        }
        let index = u32::try_from(index).map_err(|_| "qr index")?;
        let x = (index % modules + QUIET_MODULES) * module_px;
        let y = (index / modules + QUIET_MODULES) * module_px;
        if let Some(rect) =
            tiny_skia::Rect::from_xywh(x as f32, y as f32, module_px as f32, module_px as f32)
        {
            pixmap.fill_rect(
                rect,
                &{
                    let mut paint = tiny_skia::Paint::default();
                    paint.set_color(dark);
                    paint
                },
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
    encode_png(&pixmap).map_err(|error| error.to_string())
}

/// Module size from a query value, defaults and clamps included.
#[must_use]
pub fn module_px(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(DEFAULT_MODULE_PX)
        .clamp(1, MAX_MODULE_PX)
}

/// The URL a QR should point the app at: the first reachable address (LAN, then Tailscale).
#[must_use]
pub fn best_url(urls: &[String]) -> String {
    urls.first().cloned().unwrap_or_default()
}

/// Convenience for the route: the current code's deep link.
#[must_use]
pub fn current_payload(pairing: &pairing::Pairing, urls: &[String]) -> String {
    payload(&pairing.current(), &best_url(urls))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_payload_is_an_app_deep_link() {
        let link = payload("6WSGM4", "http://192.168.1.19:7420");
        assert!(link.starts_with("penwise://pair?"));
        assert!(link.contains("code=6WSGM4"));
        assert!(link.ends_with("url=http://192.168.1.19:7420"));
    }

    #[test]
    fn a_qr_renders_as_a_square_png() {
        let png = png(&payload("6WSGM4", "http://192.168.1.19:7420"), 4).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        // A version-3 code is 29 modules; with a 4-module quiet zone that is 37 modules at 4 px = 148 px.
        let size = module_px(Some(4));
        assert_eq!(4, size);
        assert!(png.len() > 200, "a real bitmap, not an empty canvas");
    }

    #[test]
    fn module_size_is_clamped() {
        assert_eq!(DEFAULT_MODULE_PX, module_px(None));
        assert_eq!(1, module_px(Some(0)));
        assert_eq!(MAX_MODULE_PX, module_px(Some(1000)));
    }

    #[test]
    fn the_lan_address_is_preferred() {
        let urls = vec![
            "http://192.168.1.19:7420".to_owned(),
            "http://deskpc.tail000000.ts.net:7420".to_owned(),
        ];
        assert_eq!("http://192.168.1.19:7420", best_url(&urls));
        assert!(best_url(&[]).is_empty());
    }
}

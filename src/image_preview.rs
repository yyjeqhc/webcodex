use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use sha2::{Digest, Sha256};

pub(crate) const ACTION_INLINE_PREVIEW_MAX_BYTES: usize = 48 * 1024;
pub(crate) const MCP_INLINE_PREVIEW_MAX_BYTES: usize = 192 * 1024;
pub(crate) const SCREENSHOT_INLINE_PREVIEW_MAX_EDGE: u32 = 1440;

#[derive(Debug, Clone)]
pub(crate) struct BoundedImagePreview {
    pub(crate) bytes: Vec<u8>,
    pub(crate) mime_type: &'static str,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) sha256: String,
}

pub(crate) fn encode_bounded_preview(
    decoded: &[u8],
    max_bytes: usize,
) -> Result<BoundedImagePreview, String> {
    let image = image::load_from_memory(decoded)
        .map_err(|error| format!("snapshot preview decode failed: {error}"))?;
    let mut rgba = image.to_rgba8();
    let max_edge = rgba.width().max(rgba.height());
    if max_edge > SCREENSHOT_INLINE_PREVIEW_MAX_EDGE {
        let scale = SCREENSHOT_INLINE_PREVIEW_MAX_EDGE as f64 / max_edge as f64;
        rgba = image::imageops::resize(
            &rgba,
            ((rgba.width() as f64 * scale).floor() as u32).max(1),
            ((rgba.height() as f64 * scale).floor() as u32).max(1),
            FilterType::Lanczos3,
        );
    }
    for _ in 0..6 {
        for quality in [84u8, 80, 76] {
            let mut bytes = Vec::new();
            JpegEncoder::new_with_quality(&mut bytes, quality)
                .encode_image(&rgba)
                .map_err(|error| format!("snapshot preview encode failed: {error}"))?;
            if bytes.len() <= max_bytes {
                return Ok(BoundedImagePreview {
                    sha256: format!("{:x}", Sha256::digest(&bytes)),
                    bytes,
                    mime_type: "image/jpeg",
                    width: rgba.width(),
                    height: rgba.height(),
                });
            }
        }
        if rgba.width() <= 320 || rgba.height() <= 240 {
            break;
        }
        rgba = image::imageops::resize(
            &rgba,
            (rgba.width() * 4 / 5).max(1),
            (rgba.height() * 4 / 5).max(1),
            FilterType::Lanczos3,
        );
    }
    Err(format!(
        "snapshot preview could not be encoded within {max_bytes} bytes"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_preview_stays_under_transport_budget() {
        let width = 1280;
        let height = 720;
        let mut rgba = image::RgbaImage::new(width, height);
        for (x, y, pixel) in rgba.enumerate_pixels_mut() {
            let v =
                ((x.wrapping_mul(37) ^ y.wrapping_mul(91) ^ (x + y).wrapping_mul(13)) & 0xff) as u8;
            *pixel = image::Rgba([v, v.rotate_left(2), v.rotate_left(5), 255]);
        }
        let mut full = Vec::new();
        JpegEncoder::new_with_quality(&mut full, 76)
            .encode_image(&rgba)
            .expect("encode fixture");
        let preview = encode_bounded_preview(&full, ACTION_INLINE_PREVIEW_MAX_BYTES)
            .expect("bounded preview");
        assert!(preview.bytes.len() <= ACTION_INLINE_PREVIEW_MAX_BYTES);
        assert!(preview.width <= SCREENSHOT_INLINE_PREVIEW_MAX_EDGE);
        assert!(preview.height <= SCREENSHOT_INLINE_PREVIEW_MAX_EDGE);
        assert_eq!(preview.sha256.len(), 64);
    }

    #[test]
    fn bounded_preview_accepts_png_source_for_browser_snapshots() {
        let mut rgba = image::RgbaImage::new(1024, 576);
        for (x, y, pixel) in rgba.enumerate_pixels_mut() {
            let v = ((x.wrapping_mul(17) ^ y.wrapping_mul(53)) & 0xff) as u8;
            *pixel = image::Rgba([v, 255u8.wrapping_sub(v), v.rotate_left(3), 255]);
        }
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(rgba)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encode PNG fixture");
        let preview = encode_bounded_preview(&png, MCP_INLINE_PREVIEW_MAX_BYTES)
            .expect("browser PNG preview");
        assert_eq!(preview.mime_type, "image/jpeg");
        assert!(preview.bytes.len() <= MCP_INLINE_PREVIEW_MAX_BYTES);
        assert!(preview.width <= SCREENSHOT_INLINE_PREVIEW_MAX_EDGE);
        assert!(preview.height <= SCREENSHOT_INLINE_PREVIEW_MAX_EDGE);
    }
}

//! Product image (and, since Phase 15, business logo) storage.
//!
//! Images are never stored in SQLite (that bloats the database file and every
//! backup of it). Each one is a plain file under the app's data directory,
//! named by us — never by the client's original filename, which sidesteps any
//! path-traversal or collision risk — and `items.image_path`/
//! `app_config.logo_path` store just that generated filename, not a full
//! path, so it stays valid across reinstalls and OS moves.
//!
//! A JPEG or PNG is shrunk to at most `MAX_DIMENSION` pixels on its longer
//! side when it is saved (a phone photo goes from megabytes to a few hundred
//! KB), and every file is named after a hash of its content. Counters of one
//! business share photos (`workspace::media`), so the same photo has the same
//! name everywhere and an upload retried is the same file. Two products may
//! therefore share one file: delete through [`delete_if_unused`].
//!
//! Product photos and the business logo are two different upload contexts
//! (different allowed formats, different size cap, different directory —
//! see `images_dir` vs `logo_dir`) but share every mechanic below.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView, ImageFormat};
use sha2::{Digest, Sha256};

/// Hard cap on an uploaded product photo, applied before it ever touches
/// disk. Large enough for a real product photo, small enough that a shop's
/// image folder (and any backup of it) stays manageable on low-spec hardware.
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const ITEM_ALLOWED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif"];

/// A logo is displayed small (top bar, receipt header) and uploaded far less
/// often than product photos — 2 MB is generous for that and keeps a
/// mis-picked multi-megabyte photo from being used as a "logo" by accident.
pub const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024;
const LOGO_ALLOWED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "svg"];

/// Longer side of a stored JPEG/PNG. Plenty for a product tile or a receipt
/// logo, and it keeps a business's photos small enough to sync.
pub const MAX_DIMENSION: u32 = 1024;
/// A JPEG/PNG already this small and within `MAX_DIMENSION` is kept as is.
const KEEP_AS_IS_BYTES: usize = 300 * 1024;
const JPEG_QUALITY: u8 = 82;
/// The most the business server accepts for one file.
pub const MAX_SHARED_BYTES: usize = 3 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("Unsupported image format \"{0}\" — use JPG, PNG, WEBP or GIF")]
    UnsupportedFormat(String),
    #[error("Image is {:.1} MB, which is over the {:.0} MB limit", *.bytes as f64 / 1_048_576.0, *.max as f64 / 1_048_576.0)]
    TooLarge { bytes: usize, max: usize },
    #[error("Could not read image data: {0}")]
    InvalidBase64(String),
    #[error("Invalid image file name: {0}")]
    InvalidFileName(String),
    #[error("The file is not a readable {0} image")]
    NotAnImage(String),
    #[error("Could not save image: {0}")]
    Io(String),
}

fn mime_for_extension(extension: &str) -> &'static str {
    match extension {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// A generated filename never contains a path separator or `..`, but incoming
/// filenames (from the database, ultimately client-controlled at some point in
/// the past) are re-checked before ever being joined onto a filesystem path.
fn sanitize_file_name(file_name: &str) -> Result<&str, ImageError> {
    let is_safe = !file_name.is_empty()
        && !file_name.contains('/')
        && !file_name.contains('\\')
        && file_name != "."
        && file_name != "..";
    if is_safe {
        Ok(file_name)
    } else {
        Err(ImageError::InvalidFileName(file_name.to_string()))
    }
}

pub fn images_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("product-images")
}

/// Kept in its own directory rather than alongside product photos — one
/// file that changes rarely, versus a folder of many that changes often;
/// separate also makes "wipe product photos, keep the logo" (or vice versa)
/// possible for anyone poking at the data directory by hand.
pub fn logo_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("logo")
}

/// Whether a stored name is one this app could have generated: letters,
/// digits, `-` and `_`, then a known image extension. Names from the
/// database are checked again before touching the filesystem (a name can
/// arrive from another counter).
pub fn is_valid_name(file_name: &str) -> bool {
    let Some((stem, extension)) = file_name.rsplit_once('.') else { return false };
    !stem.is_empty()
        && stem.len() <= 91
        && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && stem.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && ["jpg", "jpeg", "png", "webp", "gif", "svg"].contains(&extension)
}

/// `<prefix>-<first 32 hex of sha256>.<extension>`: the same bytes always get
/// the same name.
fn content_name(prefix: &str, bytes: &[u8], extension: &str) -> String {
    let digest = Sha256::digest(bytes);
    let hex: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
    format!("{prefix}-{hex}.{extension}")
}

/// Shrinks a JPEG or PNG to at most `max_dimension` on its longer side and
/// re-encodes it: PNG if it has see-through pixels, JPEG otherwise. A small
/// image is returned untouched, and so is any other format (WEBP, GIF, SVG).
/// Returns the bytes and their extension.
pub fn shrink(bytes: &[u8], extension: &str, max_dimension: u32) -> Result<(Vec<u8>, String), ImageError> {
    let format = match extension {
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "png" => ImageFormat::Png,
        _ => return Ok((bytes.to_vec(), extension.to_string())),
    };
    let image = image::load_from_memory_with_format(bytes, format).map_err(|_| ImageError::NotAnImage(extension.to_ascii_uppercase()))?;
    let (width, height) = image.dimensions();
    if width.max(height) <= max_dimension && bytes.len() <= KEEP_AS_IS_BYTES {
        return Ok((bytes.to_vec(), extension.to_string()));
    }
    let image = if width.max(height) > max_dimension {
        image.resize(max_dimension, max_dimension, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let see_through = image.color().has_alpha() && image.to_rgba8().pixels().any(|p| p.0[3] < 255);
    let mut out = Cursor::new(Vec::new());
    if see_through {
        image.write_to(&mut out, ImageFormat::Png).map_err(|e| ImageError::Io(e.to_string()))?;
        return Ok((out.into_inner(), "png".into()));
    }
    let rgb = DynamicImage::ImageRgb8(image.to_rgb8());
    rgb.write_with_encoder(JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)).map_err(|e| ImageError::Io(e.to_string()))?;
    Ok((out.into_inner(), "jpg".into()))
}

/// Writes `bytes` to `path` through a temporary file, so a crash never
/// leaves half an image under a real name.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ImageError> {
    let dir = path.parent().ok_or_else(|| ImageError::Io("no directory".into()))?;
    fs::create_dir_all(dir).map_err(|e| ImageError::Io(e.to_string()))?;
    let tmp = path.with_extension("partial");
    fs::write(&tmp, bytes).map_err(|e| ImageError::Io(e.to_string()))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        ImageError::Io(e.to_string())
    })
}

/// Decodes and validates a base64-encoded image against `(allowed, max)`,
/// shrinks it, writes it under `dir` and returns its name. Shared by
/// [`save_image`] and [`save_logo`] — the two only differ in which allowed
/// formats/size cap/filename prefix they call this with.
fn save_image_as(
    dir: &Path,
    data_base64: &str,
    extension: &str,
    allowed: &[&str],
    max_bytes: usize,
    prefix: &str,
) -> Result<String, ImageError> {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    if !allowed.contains(&extension.as_str()) {
        return Err(ImageError::UnsupportedFormat(extension));
    }

    let bytes = BASE64
        .decode(data_base64)
        .map_err(|e| ImageError::InvalidBase64(e.to_string()))?;

    if bytes.len() > max_bytes {
        return Err(ImageError::TooLarge { bytes: bytes.len(), max: max_bytes });
    }
    let (bytes, extension) = shrink(&bytes, &extension, MAX_DIMENSION)?;
    // Only a WEBP/GIF can still be this big: it must fit the business server.
    if bytes.len() > MAX_SHARED_BYTES {
        return Err(ImageError::TooLarge { bytes: bytes.len(), max: MAX_SHARED_BYTES });
    }

    let file_name = content_name(prefix, &bytes, &extension);
    let path = dir.join(&file_name);
    if !path.exists() {
        write_atomically(&path, &bytes)?;
    }
    Ok(file_name)
}

/// Decodes and validates a base64-encoded product photo, writes it under
/// `dir`, and returns the generated filename to store as `items.image_path`.
pub fn save_image(dir: &Path, data_base64: &str, extension: &str) -> Result<String, ImageError> {
    save_image_as(dir, data_base64, extension, ITEM_ALLOWED_EXTENSIONS, MAX_IMAGE_BYTES, "item")
}

/// Decodes and validates a base64-encoded logo, writes it under `dir`, and
/// returns the generated filename to store as `app_config.logo_path`.
/// Narrower format list and smaller cap than [`save_image`] — see the
/// module doc comment.
pub fn save_logo(dir: &Path, data_base64: &str, extension: &str) -> Result<String, ImageError> {
    save_image_as(dir, data_base64, extension, LOGO_ALLOWED_EXTENSIONS, MAX_LOGO_BYTES, "logo")
}

/// Raw file bytes plus lowercased extension — the one place both the
/// data-URL reader (below) and the ESC/POS logo-raster builder
/// (`printer::escpos`) read a stored image from disk, so the sanitize/
/// extension logic isn't duplicated between them.
pub fn read_image_bytes(dir: &Path, file_name: &str) -> Result<(Vec<u8>, String), ImageError> {
    let file_name = sanitize_file_name(file_name)?;
    let extension = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let bytes = fs::read(dir.join(file_name)).map_err(|e| ImageError::Io(e.to_string()))?;
    Ok((bytes, extension))
}

/// Reads an image back as a `data:` URL the webview can render directly in an
/// `<img>` tag with no extra IPC round trip needed to display it.
pub fn read_image_data_url(dir: &Path, file_name: &str) -> Result<String, ImageError> {
    let (bytes, extension) = read_image_bytes(dir, file_name)?;
    let encoded = BASE64.encode(bytes);
    Ok(format!("data:{};base64,{}", mime_for_extension(&extension), encoded))
}

/// Best-effort delete — a missing file (already cleaned up, or from a restored
/// backup that predates it) is not an error worth surfacing to the cashier.
pub fn delete_image(dir: &Path, file_name: &str) {
    let Ok(file_name) = sanitize_file_name(file_name) else { return };
    let _ = fs::remove_file(dir.join(file_name));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pos-image-test-{}-{}-{}",
            label,
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        dir
    }

    fn tiny_png_base64() -> String {
        // A 1x1 transparent PNG — small, valid, real image bytes.
        BASE64.encode([
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ])
    }

    #[test]
    fn saves_and_reads_back_an_image_round_trip() {
        let dir = tmp_dir("round-trip");
        let file_name = save_image(&dir, &tiny_png_base64(), "png").unwrap();
        assert!(file_name.ends_with(".png"));

        let data_url = read_image_data_url(&dir, &file_name).unwrap();
        assert!(data_url.starts_with("data:image/png;base64,"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_unsupported_extensions() {
        let dir = tmp_dir("bad-ext");
        let err = save_image(&dir, &tiny_png_base64(), "exe").unwrap_err();
        assert!(matches!(err, ImageError::UnsupportedFormat(_)));
    }

    #[test]
    fn rejects_images_over_the_size_limit() {
        let dir = tmp_dir("too-large");
        let oversized = BASE64.encode(vec![0u8; MAX_IMAGE_BYTES + 1]);
        let err = save_image(&dir, &oversized, "png").unwrap_err();
        assert!(matches!(err, ImageError::TooLarge { .. }));
    }

    #[test]
    fn rejects_path_traversal_in_a_stored_file_name() {
        let dir = tmp_dir("traversal");
        assert!(matches!(
            read_image_data_url(&dir, "../../etc/passwd").unwrap_err(),
            ImageError::InvalidFileName(_)
        ));
        assert!(matches!(
            read_image_data_url(&dir, "sub/dir.png").unwrap_err(),
            ImageError::InvalidFileName(_)
        ));
    }

    #[test]
    fn delete_is_a_no_op_for_a_missing_file() {
        let dir = tmp_dir("missing");
        // Must not panic even though the file (and directory) never existed.
        delete_image(&dir, "never-existed.png");
    }

    #[test]
    fn the_same_photo_gets_the_same_name_and_a_different_one_does_not() {
        let dir = tmp_dir("content-names");
        let a = save_image(&dir, &tiny_png_base64(), "png").unwrap();
        let b = save_image(&dir, &tiny_png_base64(), "png").unwrap();
        assert_eq!(a, b, "named by content, so every counter names it alike");
        let other = BASE64.encode(png_bytes(3, 3, 255));
        assert_ne!(save_image(&dir, &other, "png").unwrap(), a);
        assert!(is_valid_name(&a));
        fs::remove_dir_all(&dir).ok();
    }

    fn png_bytes(width: u32, height: u32, alpha: u8) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, alpha]));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_large_photo_is_shrunk_and_stored_as_jpeg_unless_it_is_see_through() {
        let dir = tmp_dir("shrink");
        let name = save_image(&dir, &BASE64.encode(png_bytes(2400, 1200, 255)), "png").unwrap();
        assert!(name.ends_with(".jpg"), "an opaque photo becomes a JPEG: {name}");
        let stored = image::open(dir.join(&name)).unwrap();
        assert_eq!(stored.dimensions(), (1024, 512), "longer side capped, aspect kept");

        let name = save_image(&dir, &BASE64.encode(png_bytes(1600, 1600, 128)), "png").unwrap();
        assert!(name.ends_with(".png"), "transparency keeps PNG: {name}");
        assert_eq!(image::open(dir.join(&name)).unwrap().dimensions(), (1024, 1024));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_is_not_the_image_it_claims_is_refused() {
        let dir = tmp_dir("not-an-image");
        let err = save_image(&dir, &BASE64.encode(b"definitely not a png"), "png").unwrap_err();
        assert!(matches!(err, ImageError::NotAnImage(_)));
    }

    #[test]
    fn only_generated_looking_names_are_valid() {
        for good in ["item-0a1b.png", "logo-x_y.svg", "item-1700000000-3.jpeg"] {
            assert!(is_valid_name(good), "{good}");
        }
        for bad in ["../x.png", "a/b.png", ".png", "-x.png", "x.exe", "x.PNG", "", "x"] {
            assert!(!is_valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn save_logo_accepts_svg_which_save_image_does_not() {
        let dir = tmp_dir("logo-svg");
        let svg = BASE64.encode(b"<svg xmlns='http://www.w3.org/2000/svg'></svg>");

        let logo_file = save_logo(&dir, &svg, "svg").unwrap();
        assert!(logo_file.ends_with(".svg"));
        assert!(logo_file.starts_with("logo-"));
        assert!(is_valid_name(&logo_file));

        let err = save_image(&dir, &svg, "svg").unwrap_err();
        assert!(matches!(err, ImageError::UnsupportedFormat(_)), "product photos must not accept svg");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_logo_reads_back_with_the_svg_mime_type() {
        let dir = tmp_dir("logo-mime");
        let svg = BASE64.encode(b"<svg xmlns='http://www.w3.org/2000/svg'></svg>");
        let file_name = save_logo(&dir, &svg, "svg").unwrap();

        let data_url = read_image_data_url(&dir, &file_name).unwrap();
        assert!(data_url.starts_with("data:image/svg+xml;base64,"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_logo_enforces_its_own_smaller_size_cap() {
        let dir = tmp_dir("logo-too-large");
        // Under the 5 MB product-photo cap, but over the 2 MB logo cap.
        let oversized = BASE64.encode(vec![0u8; MAX_LOGO_BYTES + 1]);
        let err = save_logo(&dir, &oversized, "png").unwrap_err();
        assert!(matches!(err, ImageError::TooLarge { max, .. } if max == MAX_LOGO_BYTES));
    }

    #[test]
    fn save_logo_rejects_formats_outside_its_own_allowed_list() {
        let dir = tmp_dir("logo-bad-ext");
        // webp/gif are fine for product photos but not offered for logos.
        let err = save_logo(&dir, &tiny_png_base64(), "webp").unwrap_err();
        assert!(matches!(err, ImageError::UnsupportedFormat(_)));
    }
}

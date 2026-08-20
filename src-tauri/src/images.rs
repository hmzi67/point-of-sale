//! Product image (and, since Phase 15, business logo) storage.
//!
//! Images are never stored in SQLite (that bloats the database file and every
//! backup of it). Each one is a plain file under the app's data directory,
//! named by us — never by the client's original filename, which sidesteps any
//! path-traversal or collision risk — and `items.image_path`/
//! `app_config.logo_path` store just that generated filename, not a full
//! path, so it stays valid across reinstalls and OS moves.
//!
//! Product photos and the business logo are two different upload contexts
//! (different allowed formats, different size cap, different directory —
//! see `images_dir` vs `logo_dir`) but share every mechanic below: base64
//! decode, size/format validation, a collision-proof generated filename,
//! read-back as a `data:` URL, best-effort delete.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

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

#[derive(Debug)]
pub enum ImageError {
    UnsupportedFormat(String),
    TooLarge { bytes: usize, max: usize },
    InvalidBase64(String),
    InvalidFileName(String),
    Io(String),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageError::UnsupportedFormat(ext) => {
                write!(f, "Unsupported image format \"{}\" — use JPG, PNG, WEBP or GIF", ext)
            }
            ImageError::TooLarge { bytes, max } => write!(
                f,
                "Image is {:.1} MB, which is over the {:.0} MB limit",
                *bytes as f64 / 1_048_576.0,
                *max as f64 / 1_048_576.0
            ),
            ImageError::InvalidBase64(msg) => write!(f, "Could not read image data: {}", msg),
            ImageError::InvalidFileName(name) => write!(f, "Invalid image file name: {}", name),
            ImageError::Io(msg) => write!(f, "Could not save image: {}", msg),
        }
    }
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

/// Monotonic-enough unique filename: wall-clock nanoseconds plus a
/// same-process counter, so two uploads in the same nanosecond still differ.
/// `prefix` distinguishes which upload context generated it (purely
/// cosmetic/debugging — nothing parses it back apart).
fn generate_file_name(prefix: &str, extension: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{}.{}", prefix, nanos, seq, extension)
}

/// Decodes and validates a base64-encoded image against `(allowed, max)`,
/// writes it under `dir`, and returns the generated filename. Shared by
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

    fs::create_dir_all(dir).map_err(|e| ImageError::Io(e.to_string()))?;
    let file_name = generate_file_name(prefix, &extension);
    fs::write(dir.join(&file_name), &bytes).map_err(|e| ImageError::Io(e.to_string()))?;

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

/// Reads an image back as a `data:` URL the webview can render directly in an
/// `<img>` tag with no extra IPC round trip needed to display it.
pub fn read_image_data_url(dir: &Path, file_name: &str) -> Result<String, ImageError> {
    let file_name = sanitize_file_name(file_name)?;
    let extension = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let bytes = fs::read(dir.join(file_name)).map_err(|e| ImageError::Io(e.to_string()))?;
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
    fn two_uploads_in_the_same_process_never_collide() {
        let dir = tmp_dir("uniqueness");
        let a = save_image(&dir, &tiny_png_base64(), "png").unwrap();
        let b = save_image(&dir, &tiny_png_base64(), "png").unwrap();
        assert_ne!(a, b);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_logo_accepts_svg_which_save_image_does_not() {
        let dir = tmp_dir("logo-svg");
        let svg = BASE64.encode(b"<svg xmlns='http://www.w3.org/2000/svg'></svg>");

        let logo_file = save_logo(&dir, &svg, "svg").unwrap();
        assert!(logo_file.ends_with(".svg"));
        assert!(logo_file.starts_with("logo-"));

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

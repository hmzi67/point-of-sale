//! Product image storage.
//!
//! Images are never stored in SQLite (that bloats the database file and every
//! backup of it). Each one is a plain file under the app's data directory,
//! named by us — never by the client's original filename, which sidesteps any
//! path-traversal or collision risk — and `items.image_path` stores just that
//! generated filename, not a full path, so it stays valid across reinstalls
//! and OS moves.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

/// Hard cap on an uploaded image, applied before it ever touches disk. Large
/// enough for a real product photo, small enough that a shop's image folder
/// (and any backup of it) stays manageable on low-spec hardware.
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

const ALLOWED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif"];

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

/// Monotonic-enough unique filename: wall-clock nanoseconds plus a
/// same-process counter, so two uploads in the same nanosecond still differ.
fn generate_file_name(extension: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("item-{}-{}.{}", nanos, seq, extension)
}

/// Decodes and validates a base64-encoded image, writes it under `dir`, and
/// returns the generated filename to store as `items.image_path`.
pub fn save_image(dir: &Path, data_base64: &str, extension: &str) -> Result<String, ImageError> {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    if !ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
        return Err(ImageError::UnsupportedFormat(extension));
    }

    let bytes = BASE64
        .decode(data_base64)
        .map_err(|e| ImageError::InvalidBase64(e.to_string()))?;

    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ImageError::TooLarge { bytes: bytes.len(), max: MAX_IMAGE_BYTES });
    }

    fs::create_dir_all(dir).map_err(|e| ImageError::Io(e.to_string()))?;
    let file_name = generate_file_name(&extension);
    fs::write(dir.join(&file_name), &bytes).map_err(|e| ImageError::Io(e.to_string()))?;

    Ok(file_name)
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
}

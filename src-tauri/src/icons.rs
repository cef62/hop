//! App icons for the palette, served through the `icon` URI scheme (`icon://localhost/<url-encoded app path>`), so the
//! webview loads them like images and no icon bytes cross IPC. Rendered once to a 64 px PNG and cached on disk, keyed
//! by bundle path and modification time (bundle or its Info.plist, whichever is newer); a changed app gets a new cache file.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use tauri::http::{Request, Response, StatusCode};
use tauri::{AppHandle, Manager, UriSchemeContext, UriSchemeResponder};

/// Pixel size of the cached PNG: sharp at 2x for the largest icon in the UI (Roomy, 40 pt), smaller ones downscale.
const SIZE: f64 = 64.0;

/// Cache file for the app at `path` with modification time `mtime` (seconds). The hash only names the file: a
/// collision or a Rust upgrade that changes `DefaultHasher` costs a re-render, never a wrong icon for long.
pub fn cache_file(dir: &Path, path: &str, mtime: u64) -> PathBuf {
    let mut h = DefaultHasher::new();
    (path, mtime).hash(&mut h);
    dir.join(format!("{:016x}.png", h.finish()))
}

/// The PNG for `path`, from the cache or freshly rendered (then written to the cache; a failed write still serves it).
fn icon_png(cache_dir: &Path, path: &str) -> Option<Vec<u8>> {
    // an update can rewrite only files inside Contents/, leaving the bundle folder's mtime alone: take the newer of the two
    let secs = |p: &Path| std::fs::metadata(p).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs());
    let mtime = secs(Path::new(path))?.max(secs(&Path::new(path).join("Contents/Info.plist")).unwrap_or(0));
    let file = cache_file(cache_dir, path, mtime);
    if let Ok(png) = std::fs::read(&file) {
        return Some(png);
    }
    let png = render(path)?;
    // shortcut: stale files of updated or removed apps stay in the cache (a few KB each); prune if it ever matters
    if std::fs::create_dir_all(cache_dir).is_ok() {
        let tmp = file.with_extension("tmp");
        if std::fs::write(&tmp, &png).is_ok() {
            std::fs::rename(&tmp, &file).ok();
        }
    }
    Some(png)
}

/// Drains AppKit's autoreleased objects per icon: the worker threads have no run loop pool, so they would pile up.
#[cfg(target_os = "macos")]
fn render(path: &str) -> Option<Vec<u8>> {
    // the bundle itself, not a symlink to it: macOS badges a link's icon with an alias arrow
    let path = std::fs::canonicalize(path).ok()?;
    objc2::rc::autoreleasepool(|_| render_png(&path.to_string_lossy()))
}

#[cfg(target_os = "macos")]
fn render_png(path: &str) -> Option<Vec<u8>> {
    use objc2::AllocAnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSCalibratedRGBColorSpace, NSGraphicsContext, NSWorkspace};
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

    let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path));
    // SAFETY: a null planes pointer makes AppKit allocate the pixel buffer; the arguments describe 8-bit RGBA
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(), std::ptr::null_mut(), SIZE as isize, SIZE as isize, 8, 4, true, false, NSCalibratedRGBColorSpace, 0, 0,
        )
    }?;
    let ctx = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    // the current context is per thread; this runs on a worker thread that draws nothing else
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&ctx));
    image.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(SIZE, SIZE)));
    ctx.flushGraphics();
    NSGraphicsContext::restoreGraphicsState_class();
    // SAFETY: an empty property dictionary is valid for PNG
    let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
    Some(data.to_vec())
}

#[cfg(not(target_os = "macos"))]
fn render(_path: &str) -> Option<Vec<u8>> {
    None
}

/// The `icon` scheme handler. Serves only apps in the index, only to the palette window; renders off the main thread.
/// Anything else, or an app whose icon cannot be rendered, gets a 404 and the UI keeps its placeholder.
pub fn handle(ctx: UriSchemeContext<'_, tauri::Wry>, req: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app: AppHandle = ctx.app_handle().clone();
    let label = ctx.webview_label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let path = percent_decode(req.uri().path().trim_start_matches('/'));
        let png = path
            .filter(|p| label == "main" && crate::apps::known(&app, p))
            .and_then(|p| icon_png(&app.path().app_cache_dir().ok()?.join("icons"), &p));
        let res = match png {
            Some(png) => Response::builder().header("Content-Type", "image/png").header("Cache-Control", "no-cache").body(png),
            None => Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()),
        };
        responder.respond(res.unwrap());
    });
}

/// `%XX` decoding of a URI path (what `convertFileSrc` produces with `encodeURIComponent`). `None` on bad escapes or UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_convert_file_src_paths() {
        assert_eq!(percent_decode("%2FApplications%2FVisual%20Studio%20Code.app").as_deref(), Some("/Applications/Visual Studio Code.app"));
        assert_eq!(percent_decode("caf%C3%A9").as_deref(), Some("café"));
        assert_eq!(percent_decode("bad%2"), None);
        assert_eq!(percent_decode("bad%zz"), None);
    }

    #[test]
    fn cache_key_changes_with_mtime_and_path() {
        let d = Path::new("/c");
        assert_eq!(cache_file(d, "/A.app", 1), cache_file(d, "/A.app", 1));
        assert_ne!(cache_file(d, "/A.app", 1), cache_file(d, "/A.app", 2));
        assert_ne!(cache_file(d, "/A.app", 1), cache_file(d, "/B.app", 1));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn renders_a_64px_png_for_an_app_without_an_icon_and_caches_it() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("Bare.app");
        std::fs::create_dir_all(app.join("Contents")).unwrap(); // no Info.plist, no icns: macOS gives the generic icon
        let cache = tmp.path().join("cache");
        let png = icon_png(&cache, app.to_str().unwrap()).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 64); // IHDR width
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);
        assert_eq!(icon_png(&cache, app.to_str().unwrap()).unwrap(), png);
        assert!(icon_png(&cache, tmp.path().join("Missing.app").to_str().unwrap()).is_none());
    }
}

//! Copying the calculator's result to the clipboard.

use tauri::AppHandle;

/// A calculator result is a few dozen characters; the text comes from the webview, so anything longer is refused.
const MAX_CHARS: usize = 256;

fn check(text: &str) -> Result<(), String> {
    // an empty copy would only wipe what the user had on the clipboard
    if text.is_empty() || text.chars().count() > MAX_CHARS {
        return Err("Nothing hop can copy".into());
    }
    Ok(())
}

/// Puts `text` on the general pasteboard as plain text, then hides the palette so the user can paste in their app. Like
/// `launch`, a failure keeps the palette open with the error. Records no launch.
#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    check(&text)?;
    write(&text)?;
    crate::panel::hide(&app);
    Ok(())
}

#[cfg(target_os = "macos")]
fn write(text: &str) -> Result<(), String> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    // SAFETY: an immutable AppKit constant
    if pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString }) {
        Ok(())
    } else {
        Err("Could not copy the result".into())
    }
}

#[cfg(not(target_os = "macos"))]
fn write(_text: &str) -> Result<(), String> {
    Err("Copying is only supported on macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_only_short_non_empty_text() {
        assert!(check("14").is_ok());
        assert!(check(&"9".repeat(MAX_CHARS)).is_ok());
        assert!(check(&"é".repeat(MAX_CHARS)).is_ok()); // characters, not bytes
        assert!(check(&"9".repeat(MAX_CHARS + 1)).is_err());
        assert!(check("").is_err());
    }
}

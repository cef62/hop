//! Copying the calculator's result to the clipboard, and recording the copy in the calculator history.

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

/// Copies `text` with `write`, then, only if that worked and there is an expression, hands the pair to `record`. The
/// order is the rule: a failed copy records nothing, and `record` has no way to fail the copy.
fn copy_and_record(text: &str, expr: Option<&str>, write: impl FnOnce(&str) -> Result<(), String>, record: impl FnOnce(&str, &str)) -> Result<(), String> {
    check(text)?;
    write(text)?;
    if let Some(expr) = expr {
        record(expr, text);
    }
    Ok(())
}

/// Puts `text` on the general pasteboard as plain text, then hides the palette so the user can paste in their app. Like
/// `run`, a failure keeps the palette open with the error. Records no launch. With `expr` (the sum the result answers)
/// the copy also goes in the calculator history.
#[tauri::command]
pub fn copy_text(app: AppHandle, text: String, expr: Option<String>) -> Result<(), String> {
    copy_and_record(&text, expr.as_deref(), write, |e, r| crate::calc_history::push(&app, e, r))?;
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

    #[test]
    fn only_a_copy_records() {
        let rec = std::cell::RefCell::new(vec![]);
        let record = |e: &str, r: &str| rec.borrow_mut().push((e.to_string(), r.to_string()));
        assert!(copy_and_record("4", Some("2+2"), |_| Ok(()), record).is_ok());
        assert_eq!(*rec.borrow(), [("2+2".to_string(), "4".to_string())]);
        // no expression (a caller that is not the calculator): copies, records nothing
        rec.borrow_mut().clear();
        assert!(copy_and_record("4", None, |_| Ok(()), record).is_ok());
        // the pasteboard refused, or the text is not copyable: nothing recorded
        assert!(copy_and_record("4", Some("2+2"), |_| Err("no".into()), record).is_err());
        assert!(copy_and_record("", Some("2+2"), |_| Ok(()), record).is_err());
        assert!(copy_and_record(&"9".repeat(MAX_CHARS + 1), Some("2+2"), |_| Ok(()), record).is_err());
        assert!(rec.borrow().is_empty());
    }
}

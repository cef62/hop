//! Until M3's Settings window: the palette style, read from `settings.json` in the app config dir on every show and
//! hide. Missing file, missing key, corrupt JSON or an unknown value all read as the default.

use tauri::{AppHandle, Manager};

pub const STYLES: [&str; 2] = ["compact", "roomy"];

/// The `paletteStyle` key of a settings file's text; `"compact"` unless it names a known style.
pub fn palette_style_of(text: &str) -> &'static str {
    let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let s = v.get("paletteStyle").and_then(|s| s.as_str()).unwrap_or_default();
    STYLES.into_iter().find(|k| *k == s).unwrap_or(STYLES[0])
}

pub fn palette_style(app: &AppHandle) -> &'static str {
    let text = app.path().app_config_dir().ok().and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok());
    palette_style_of(&text.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::palette_style_of;

    #[test]
    fn reads_the_style_or_falls_back_to_compact() {
        assert_eq!(palette_style_of(r#"{"paletteStyle":"roomy","other":1}"#), "roomy");
        assert_eq!(palette_style_of(r#"{"paletteStyle":"compact"}"#), "compact");
        for bad in ["", "{", "[]", "null", r#"{"paletteStyle":"huge"}"#, r#"{"paletteStyle":3}"#, "{}"] {
            assert_eq!(palette_style_of(bad), "compact", "{bad}");
        }
    }
}

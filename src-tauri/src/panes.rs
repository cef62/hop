//! System Settings sections: every Settings extension that accepts `x-apple.systempreferences:` links is an item,
//! named from its `InfoPlist.loctable` in the user's languages and English. Scanned once: a macOS update, the only
//! thing that changes them, restarts the Mac.

use std::path::Path;

use crate::apps::{Item, Kind};

pub const ROOT: &str = "/System/Library/ExtensionKit/Extensions";
/// Sections have no icon of their own we can render: rows show this app's.
pub const SETTINGS_APP: &str = "/System/Applications/System Settings.app";
const PREFIX: &str = "pane:";

/// The extension's bundle id in a section's item id, which `x-apple.systempreferences:` takes.
pub fn bundle_id(id: &str) -> Option<&str> {
    id.strip_prefix(PREFIX)
}

/// The sections under `root`, in file-name order, for the preferred languages `langs` (BCP 47, as `NSLocale` gives them).
#[cfg(target_os = "macos")]
pub fn scan(root: &Path, langs: &[String]) -> Vec<Item> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut dirs: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "appex")).collect();
    dirs.sort();
    // a worker thread has no autorelease pool of its own
    objc2::rc::autoreleasepool(|_| dirs.iter().filter_map(|d| mac::section(d, langs)).collect())
}

/// Scanned on the first call (the index's first refresh, off the main thread), then reused.
#[cfg(target_os = "macos")]
pub fn index() -> Vec<Item> {
    static PANES: std::sync::OnceLock<Vec<Item>> = std::sync::OnceLock::new();
    PANES
        .get_or_init(|| {
            let langs = objc2_foundation::NSLocale::preferredLanguages().iter().map(|l| l.to_string()).collect::<Vec<_>>();
            scan(Path::new(ROOT), &langs)
        })
        .clone()
}

#[cfg(not(target_os = "macos"))]
pub fn index() -> Vec<Item> {
    Vec::new()
}

#[cfg(target_os = "macos")]
mod mac {
    use std::path::Path;

    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSDictionary, NSMutableDictionary, NSNumber, NSString};

    use super::{Item, Kind, PREFIX, SETTINGS_APP};

    /// The extension point of System Settings' sections.
    const POINT: &str = "com.apple.Settings.extension.ui";

    /// A plist file (XML or binary, as the loctables are) as a dictionary; `None` when missing or not a dictionary.
    fn read(path: &Path) -> Option<Retained<NSDictionary>> {
        // SAFETY: AnyObject keys and values claim nothing about their classes; every value read is downcast first
        let d = unsafe { NSMutableDictionary::<AnyObject, AnyObject>::dictionaryWithContentsOfFile(&NSString::from_str(&path.to_string_lossy())) }?;
        Some(Retained::into_super(d))
    }

    fn get(d: &NSDictionary, key: &str) -> Option<Retained<AnyObject>> {
        d.objectForKey(&NSString::from_str(key))
    }

    fn dict(d: &NSDictionary, key: &str) -> Option<Retained<NSDictionary>> {
        get(d, key)?.downcast().ok()
    }

    fn string(d: &NSDictionary, key: &str) -> Option<String> {
        Some(get(d, key)?.downcast::<NSString>().ok()?.to_string()).filter(|s| !s.is_empty())
    }

    /// The section's name for a preferred language: the exact region (`it-IT` → `it_IT`, `zh-Hant-TW` → `zh_TW`), the
    /// bare language, then any region of it (some loctables have `en_GB` but no `en`).
    fn localized(loc: &NSDictionary, lang: &str) -> Option<String> {
        let mut parts = lang.split('-');
        let base = parts.next()?;
        let mut keys: Vec<String> = loc.allKeys().iter().filter_map(|k| Some(k.downcast::<NSString>().ok()?.to_string())).collect();
        keys.sort(); // the same region wins on every scan
        let regions = keys.into_iter().filter(|k| k.strip_prefix(base).is_some_and(|r| r.starts_with('_')));
        let tries = parts.last().map(|r| format!("{base}_{r}")).into_iter().chain([base.to_string()]).chain(regions);
        tries.into_iter().find_map(|k| dict(loc, &k).and_then(|d| string(&d, "CFBundleDisplayName")))
    }

    pub fn section(appex: &Path, langs: &[String]) -> Option<Item> {
        let info = read(&appex.join("Contents/Info.plist"))?;
        let ext = dict(&info, "EXAppExtensionAttributes")?;
        let linkable = dict(&ext, "SettingsExtensionAttributes")
            .and_then(|a| get(&a, "allowsXAppleSystemPreferencesURLScheme")?.downcast::<NSNumber>().ok())
            .is_some_and(|n| n.boolValue());
        if string(&ext, "EXExtensionPointIdentifier").as_deref() != Some(POINT) || !linkable {
            return None;
        }
        let id = string(&info, "CFBundleIdentifier")?;
        // Info.plist's own CFBundleDisplayName is often the target name ("PowerPreferences"): only the loctable counts
        let loc = read(&appex.join("Contents/Resources/InfoPlist.loctable"))?;
        let mut names: Vec<String> = Vec::new();
        for n in langs.iter().map(String::as_str).chain(["en"]).filter_map(|l| localized(&loc, l)) {
            if !names.contains(&n) {
                names.push(n);
            }
        }
        if names.is_empty() {
            return None;
        }
        let name = names.remove(0);
        Some(Item { kind: Kind::Pane, id: format!("{PREFIX}{id}"), name, alt_names: names, path: Some(SETTINGS_APP.into()), confirm: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    const PRIVACY: &str = "com.apple.settings.PrivacySecurity.extension";

    /// An `.appex` as macOS ships it: `url` is `allowsXAppleSystemPreferencesURLScheme` (absent when `None`), `names`
    /// the loctable's `CFBundleDisplayName` per language key.
    fn appex(root: &Path, id: &str, point: &str, url: Option<bool>, names: &[(&str, &str)]) {
        let dir = root.join(format!("{id}.appex/Contents"));
        fs::create_dir_all(dir.join("Resources")).unwrap();
        let url = url.map_or(String::new(), |u| format!("<key>allowsXAppleSystemPreferencesURLScheme</key><{u}/>"));
        fs::write(
            dir.join("Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{id}</string><key>CFBundleDisplayName</key><string>Junk</string>
<key>EXAppExtensionAttributes</key><dict><key>EXExtensionPointIdentifier</key><string>{point}</string>
<key>SettingsExtensionAttributes</key><dict>{url}</dict></dict></dict></plist>"#
            ),
        )
        .unwrap();
        let langs: String = names
            .iter()
            .map(|(l, n)| format!("<key>{l}</key><dict><key>CFBundleDisplayName</key><string>{}</string></dict>", n.replace('&', "&amp;")))
            .collect();
        let loctable = format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>LocProvenance</key><dict/>{langs}</dict></plist>"#);
        fs::write(dir.join("Resources/InfoPlist.loctable"), loctable).unwrap();
    }

    const UI: &str = "com.apple.Settings.extension.ui";
    const PRIVACY_NAMES: &[(&str, &str)] = &[("en", "Privacy & Security"), ("it", "Privacy e sicurezza"), ("de", "Datenschutz & Sicherheit")];

    fn langs(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    fn names(items: &[Item]) -> Vec<(String, Vec<String>)> {
        items.iter().map(|i| (i.name.clone(), i.alt_names.clone())).collect()
    }

    #[test]
    fn keeps_settings_sections_with_url_scheme() {
        let tmp = tempfile::tempdir().unwrap();
        appex(tmp.path(), PRIVACY, UI, Some(true), PRIVACY_NAMES);
        let items = scan(tmp.path(), &langs(&["it-IT", "en"]));
        assert_eq!(items, [Item {
            kind: crate::apps::Kind::Pane,
            id: format!("pane:{PRIVACY}"),
            name: "Privacy e sicurezza".into(),
            alt_names: vec!["Privacy & Security".into()],
            path: Some(SETTINGS_APP.into()),
            confirm: None,
        }]);
        assert_eq!(bundle_id(&items[0].id), Some(PRIVACY));
        assert_eq!(bundle_id("/Applications/X.app"), None);
    }

    #[test]
    fn skips_other_extension_points() {
        let tmp = tempfile::tempdir().unwrap();
        appex(tmp.path(), "com.apple.Other", "com.apple.widgetkit-extension", Some(true), PRIVACY_NAMES);
        assert!(scan(tmp.path(), &langs(&["en"])).is_empty());
    }

    #[test]
    fn skips_sections_without_url_scheme() {
        let tmp = tempfile::tempdir().unwrap();
        appex(tmp.path(), "com.apple.Off", UI, Some(false), PRIVACY_NAMES);
        appex(tmp.path(), "com.apple.Home.PrivacySettingsExtension", UI, None, PRIVACY_NAMES); // Home on macOS 27
        assert!(scan(tmp.path(), &langs(&["en"])).is_empty());
    }

    #[test]
    fn skips_sections_without_a_name() {
        let tmp = tempfile::tempdir().unwrap();
        // Battery (PowerPreferences): the loctable has languages but no CFBundleDisplayName; Info.plist's is "Junk"
        appex(tmp.path(), "com.apple.Battery-Settings.extension", UI, Some(true), &[]);
        appex(tmp.path(), "com.apple.Empty", UI, Some(true), &[("en", "")]);
        appex(tmp.path(), PRIVACY, UI, Some(true), PRIVACY_NAMES);
        assert_eq!(names(&scan(tmp.path(), &langs(&["en"]))), [("Privacy & Security".into(), vec![])]);
        // no loctable at all
        fs::remove_file(tmp.path().join(format!("{PRIVACY}.appex/Contents/Resources/InfoPlist.loctable"))).unwrap();
        assert!(scan(tmp.path(), &langs(&["en"])).is_empty());
    }

    #[test]
    fn english_system_has_no_alt_name() {
        let tmp = tempfile::tempdir().unwrap();
        appex(tmp.path(), PRIVACY, UI, Some(true), PRIVACY_NAMES);
        assert_eq!(names(&scan(tmp.path(), &langs(&["en-US"]))), [("Privacy & Security".into(), vec![])]);
    }

    #[test]
    fn language_falls_back_region_then_language_then_english() {
        let tmp = tempfile::tempdir().unwrap();
        // the exact region wins over the bare language
        appex(tmp.path(), "a", UI, Some(true), &[("en", "Sound"), ("pt", "Som"), ("pt_PT", "Som PT")]);
        // Storage on macOS 27 has no `en`, only `en_AU`, `en_GB`…: any region of the language stands in
        appex(tmp.path(), "b", UI, Some(true), &[("en_AU", "Storage"), ("en_GB", "Storage"), ("it", "Spazio")]);
        // a language the loctable lacks: English only
        appex(tmp.path(), "c", UI, Some(true), &[("en", "Keyboard")]);
        // `zh-Hant-TW` looks up `zh_TW`, not the first Chinese region
        appex(tmp.path(), "d", UI, Some(true), &[("en", "Displays"), ("zh_CN", "显示器"), ("zh_TW", "顯示器")]);
        let found = |l: &[&str]| names(&scan(tmp.path(), &langs(l)));
        let s = |v: &str| v.to_string();
        assert_eq!(found(&["pt-PT"]), [(s("Som PT"), vec![s("Sound")]), (s("Storage"), vec![]), (s("Keyboard"), vec![]), (s("Displays"), vec![])]);
        assert_eq!(found(&["pt-BR"])[0], (s("Som"), vec![s("Sound")]));
        // every preferred language is a search key, in order, then English
        assert_eq!(found(&["en-US", "it-IT"])[1], (s("Storage"), vec![s("Spazio")]));
        assert_eq!(found(&["it-IT", "en-US"])[1], (s("Spazio"), vec![s("Storage")]));
        assert_eq!(found(&["zh-Hant-TW"])[3], (s("顯示器"), vec![s("Displays")]));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn real_scan_finds_privacy() {
        if !Path::new(ROOT).exists() {
            return;
        }
        let items = scan(Path::new(ROOT), &langs(&["en"]));
        println!("hop: {} System Settings sections in {ROOT}", items.len());
        let privacy = items.iter().find(|i| i.id == format!("pane:{PRIVACY}")).expect("no Privacy & Security");
        assert_eq!(privacy.name, "Privacy & Security");
        assert!(items.iter().all(|i| !i.name.is_empty()));
    }
}

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
    // one row per id (a linked bundle would give two rows with the same React key)
    let mut seen = std::collections::HashSet::new();
    // a worker thread has no autorelease pool of its own
    objc2::rc::autoreleasepool(|_| dirs.iter().filter_map(|d| mac::section(d, langs)).filter(|i| seen.insert(i.id.clone())).collect())
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
    use objc2_foundation::{NSArray, NSDictionary, NSMutableDictionary, NSNumber, NSString};

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

    /// String `key` of a loctable for a preferred language: the exact region (`it-IT` → `it_IT`, `zh-Hant-TW` →
    /// `zh_TW`), the region the loctables use for Traditional Chinese or Latin America, the bare language, then any
    /// region of it (some loctables have `en_GB` but no `en`; Simplified Chinese lands on `zh_CN`, first in key order).
    fn localized(loc: &NSDictionary, lang: &str, key: &str) -> Option<String> {
        let parts: Vec<&str> = lang.split('-').collect();
        let base = parts[0];
        let alias: &[&str] = match (base, parts.get(1).copied()) {
            ("zh", Some("Hant")) => &["zh_TW", "zh_HK"],
            ("es", Some(r)) if r != "ES" => &["es_419"],
            ("nb" | "nn", _) => &["no"], // the loctables key Norwegian as `no`
            _ => &[],
        };
        let mut keys: Vec<String> = loc.allKeys().iter().filter_map(|k| Some(k.downcast::<NSString>().ok()?.to_string())).collect();
        keys.sort(); // the same region wins on every scan
        let regions = keys.into_iter().filter(|k| k.strip_prefix(base).is_some_and(|r| r.starts_with('_')));
        let exact = parts.last().filter(|_| parts.len() > 1).map(|r| format!("{base}_{r}"));
        let tries = exact.into_iter().chain(alias.iter().map(|a| a.to_string())).chain([base.to_string()]).chain(regions);
        tries.into_iter().find_map(|k| dict(loc, &k).and_then(|d| string(&d, key)))
    }

    /// Each of `keys` in each preferred language, then English, without repeats. A name equal to `junk` is no name.
    fn names_in(loc: Option<Retained<NSDictionary>>, langs: &[String], keys: &[String], junk: Option<&str>) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let Some(loc) = loc else { return names };
        for l in langs.iter().map(String::as_str).chain(["en"]) {
            for n in keys.iter().filter_map(|k| localized(&loc, l, k)) {
                if Some(n.as_str()) != junk && !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        names
    }

    /// The Localizable.loctable keys of the section's sidebar rows (Battery has no bundle name, only these).
    // shortcut: the `powersource.battery == YES` row is put first without evaluating predicates, so a desktop Mac shows
    // "Battery" for its Energy section (both names match, the row opens the right pane); evaluate it if that matters
    fn sidebar_keys(attrs: &NSDictionary) -> Vec<String> {
        let Some(reps) = get(attrs, "representations").and_then(|r| r.downcast::<NSArray>().ok()) else { return Vec::new() };
        let mut keys: Vec<(bool, String)> = reps
            .iter()
            .filter_map(|r| {
                let r = r.downcast::<NSDictionary>().ok()?;
                Some((string(&r, "predicate").as_deref() != Some("powersource.battery == YES"), string(&r, "sidebar-name")?))
            })
            .collect();
        keys.sort_by_key(|k| k.0); // stable: the battery row first, the rest in file order
        keys.into_iter().map(|k| k.1).collect()
    }

    pub fn section(appex: &Path, langs: &[String]) -> Option<Item> {
        let info = read(&appex.join("Contents/Info.plist"))?;
        let ext = dict(&info, "EXAppExtensionAttributes")?;
        let attrs = dict(&ext, "SettingsExtensionAttributes")?;
        let linkable = get(&attrs, "allowsXAppleSystemPreferencesURLScheme").and_then(|f| f.downcast::<NSNumber>().ok()).is_some_and(|n| n.boolValue());
        if string(&ext, "EXExtensionPointIdentifier").as_deref() != Some(POINT) || !linkable {
            return None;
        }
        let id = string(&info, "CFBundleIdentifier")?;
        // Info.plist's own CFBundleDisplayName is often the target name ("PowerPreferences"): only the loctable counts.
        // Headphones' loctable repeats its executable's name ("HeadphoneSettingsExtension"); Bluetooth's real name is
        // its executable's too, so only one ending in "Extension" is junk
        let exec = string(&info, "CFBundleExecutable").filter(|e| e.ends_with("Extension"));
        let res = appex.join("Contents/Resources");
        let mut names = names_in(read(&res.join("InfoPlist.loctable")), langs, &["CFBundleDisplayName".into()], exec.as_deref());
        if names.is_empty() {
            names = names_in(read(&res.join("Localizable.loctable")), langs, &sidebar_keys(&attrs), None);
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
<key>CFBundleIdentifier</key><string>{id}</string><key>CFBundleDisplayName</key><string>Junk</string><!--info-->
<key>EXAppExtensionAttributes</key><dict><key>EXExtensionPointIdentifier</key><string>{point}</string>
<key>SettingsExtensionAttributes</key><dict>{url}<!--attrs--></dict></dict></dict></plist>"#
            ),
        )
        .unwrap();
        let names: Vec<(&str, Vec<(&str, &str)>)> = names.iter().map(|&(l, n)| (l, vec![("CFBundleDisplayName", n)])).collect();
        loctable(root, id, "InfoPlist", &names);
    }

    /// Writes `Contents/Resources/<file>.loctable`: per language key, its strings.
    fn loctable(root: &Path, id: &str, file: &str, langs: &[(&str, Vec<(&str, &str)>)]) {
        let dict = |kv: &[(&str, &str)]| kv.iter().map(|(k, v)| format!("<key>{k}</key><string>{}</string>", v.replace('&', "&amp;"))).collect::<String>();
        let langs: String = langs.iter().map(|(l, kv)| format!("<key>{l}</key><dict>{}</dict>", dict(kv))).collect();
        let xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>LocProvenance</key><dict/>{langs}</dict></plist>"#);
        fs::write(root.join(format!("{id}.appex/Contents/Resources/{file}.loctable")), xml).unwrap();
    }

    /// Adds `xml` to the appex's Info.plist: at the top level (`info`) or in `SettingsExtensionAttributes` (`attrs`).
    fn patch(root: &Path, id: &str, at: &str, xml: &str) {
        let file = root.join(format!("{id}.appex/Contents/Info.plist"));
        let text = fs::read_to_string(&file).unwrap().replace(&format!("<!--{at}-->"), xml);
        fs::write(file, text).unwrap();
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
        // …the first in key order, on every scan
        appex(tmp.path(), "b", UI, Some(true), &[("en_GB", "Storage GB"), ("en_AU", "Storage"), ("it", "Spazio")]);
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

    #[test]
    fn scripts_and_latin_america_map_to_the_loctable_regions() {
        let tmp = tempfile::tempdir().unwrap();
        appex(tmp.path(), "d", UI, Some(true), &[("en", "Displays"), ("zh_CN", "显示器"), ("zh_HK", "顯示器 HK"), ("zh_TW", "顯示器")]);
        appex(tmp.path(), "s", UI, Some(true), &[("en", "Sound"), ("es", "Sonido"), ("es_419", "Sonido LA")]);
        let first = |l: &str| scan(tmp.path(), &langs(&[l])).into_iter().map(|i| i.name).collect::<Vec<_>>();
        assert_eq!(first("zh-Hant"), ["顯示器", "Sound"]);
        assert_eq!(first("zh-Hant-US"), ["顯示器", "Sound"]);
        assert_eq!(first("zh-Hant-HK"), ["顯示器 HK", "Sound"]);
        assert_eq!(first("zh-Hans"), ["显示器", "Sound"]);
        assert_eq!(first("zh-Hans-SG"), ["显示器", "Sound"]);
        assert_eq!(first("es-MX"), ["Displays", "Sonido LA"]);
        assert_eq!(first("es-ES"), ["Displays", "Sonido"]);
        assert_eq!(first("es"), ["Displays", "Sonido"]);
    }

    #[test]
    fn norwegian_bokmal_and_nynorsk_read_the_no_names() {
        let tmp = tempfile::tempdir().unwrap();
        appex(tmp.path(), "k", UI, Some(true), &[("en", "Keyboard"), ("no", "Tastatur")]);
        let first = |l: &str| scan(tmp.path(), &langs(&[l])).into_iter().map(|i| i.name).collect::<Vec<_>>();
        assert_eq!(first("nb-NO"), ["Tastatur"]);
        assert_eq!(first("nn"), ["Tastatur"]);
    }

    #[test]
    fn a_section_reached_twice_is_kept_once() {
        let tmp = tempfile::tempdir().unwrap();
        appex(tmp.path(), PRIVACY, UI, Some(true), PRIVACY_NAMES);
        std::os::unix::fs::symlink(tmp.path().join(format!("{PRIVACY}.appex")), tmp.path().join("z.appex")).unwrap();
        assert_eq!(scan(tmp.path(), &langs(&["en"])).len(), 1);
    }

    #[test]
    fn sections_without_a_bundle_name_use_their_sidebar_names() {
        let tmp = tempfile::tempdir().unwrap();
        let battery = "com.apple.Battery-Settings.extension";
        // macOS 27's Battery: no CFBundleDisplayName, two sidebar rows picked by whether the Mac has a battery
        appex(tmp.path(), battery, UI, Some(true), &[]);
        patch(tmp.path(), battery, "attrs", r#"<key>representations</key><array>
<dict><key>predicate</key><string>powersource.battery != YES</string><key>sidebar-name</key><string>ENERGY_SAVER_PREF_TITLE</string></dict>
<dict><key>predicate</key><string>powersource.battery == YES</string><key>sidebar-name</key><string>BATTERY_PREF_TITLE</string></dict>
<dict><key>sidebar-name</key><string>MISSING_TITLE</string></dict><dict/></array>"#);
        let titles = |b: &'static str, e: &'static str| vec![("BATTERY_PREF_TITLE", b), ("ENERGY_SAVER_PREF_TITLE", e)];
        loctable(tmp.path(), battery, "Localizable", &[("en_GB", titles("Battery", "Energy")), ("it", titles("Batteria", "Energia"))]);
        // a bundle name wins over sidebar names (Mouse and Touch ID have representations too)
        appex(tmp.path(), "mouse", UI, Some(true), &[("en", "Mouse")]);
        patch(tmp.path(), "mouse", "attrs", "<key>representations</key><array><dict><key>sidebar-name</key><string>MOUSE</string></dict></array>");
        loctable(tmp.path(), "mouse", "Localizable", &[("en", vec![("MOUSE", "Rodent")])]);
        let s = |v: &str| v.to_string();
        assert_eq!(names(&scan(tmp.path(), &langs(&["it-IT", "en-US"]))), [
            (s("Batteria"), vec![s("Energia"), s("Battery"), s("Energy")]),
            (s("Mouse"), vec![]),
        ]);
        // no Localizable.loctable: no name, skipped
        fs::remove_file(tmp.path().join(format!("{battery}.appex/Contents/Resources/Localizable.loctable"))).unwrap();
        assert_eq!(scan(tmp.path(), &langs(&["en"])).len(), 1);
    }

    #[test]
    fn a_target_name_is_no_name() {
        let tmp = tempfile::tempdir().unwrap();
        // Headphones on macOS 27: the loctable repeats the executable's name in every language
        let junk = "HeadphoneSettingsExtension";
        appex(tmp.path(), "com.apple.HeadphoneSettings", UI, Some(true), &[("en_GB", junk), ("it", junk)]);
        patch(tmp.path(), "com.apple.HeadphoneSettings", "info", &format!("<key>CFBundleExecutable</key><string>{junk}</string>"));
        // Bluetooth's name is its executable's too, and is a real name
        appex(tmp.path(), "com.apple.BluetoothSettings", UI, Some(true), &[("en_GB", "Bluetooth"), ("it", "Bluetooth")]);
        patch(tmp.path(), "com.apple.BluetoothSettings", "info", "<key>CFBundleExecutable</key><string>Bluetooth</string>");
        // any other target name repeated as the name is dropped too, not only Headphones'
        appex(tmp.path(), "com.apple.TrackpadSettings", UI, Some(true), &[("en", "TrackpadSettingsExtension")]);
        patch(tmp.path(), "com.apple.TrackpadSettings", "info", "<key>CFBundleExecutable</key><string>TrackpadSettingsExtension</string>");
        // a name ending in "Extension" that is not the executable stays
        appex(tmp.path(), "x", UI, Some(true), &[("en", "Some Extension")]);
        patch(tmp.path(), "x", "info", "<key>CFBundleExecutable</key><string>XSettingsExtension</string>");
        let s = |v: &str| v.to_string();
        assert_eq!(names(&scan(tmp.path(), &langs(&["en"]))), [(s("Bluetooth"), vec![]), (s("Some Extension"), vec![])]);
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
        assert!(items.iter().all(|i| !i.name.is_empty() && !i.name.ends_with("Extension")), "{items:?}");
        assert!(items.iter().any(|i| i.id == "pane:com.apple.Battery-Settings.extension"), "no Battery");
    }
}

//! Pure parsing/formatting helpers for settings text fields: combo-box option
//! indices, the comma-separated `SendToApp` and blacklist fields, and Slint
//! key-event normalisation for the hotkey field.

/// Index of `needle` in `haystack` (case-insensitive), for combo boxes whose
/// persisted value came from the same option list. Falls back to index 0 when
/// `needle` is not in the list (a hand-edit or a value removed between
/// versions) but logs a warning so the mismatch is visible instead of silently
/// showing the first option.
pub(super) fn idx_of(needle: &str, haystack: &[&str]) -> i32 {
    match haystack
        .iter()
        .position(|&s| s.eq_ignore_ascii_case(needle))
    {
        Some(i) => i as i32,
        None => {
            tracing::warn!(
                "settings: '{needle}' is not one of the valid values ({haystack:?}); using the default"
            );
            0
        }
    }
}

/// Derive a display name from a bare path (the file name without extension),
/// falling back to the whole path when there is no stem.
fn derive_app_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Parse a comma-separated list of `Name: path` entries into `SendToApp`
/// structs. A bare path (no colon) derives its name from the file name.
pub(super) fn parse_send_to_apps(raw: &str) -> Vec<cliptoo_core::SendToApp> {
    raw.split(',')
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .map(|entry| match entry.split_once(':') {
            Some((name, path)) => {
                let name = name.trim();
                let path = path.trim();
                cliptoo_core::SendToApp {
                    name: if name.is_empty() {
                        derive_app_name(path)
                    } else {
                        name.to_string()
                    },
                    path: path.to_string(),
                }
            }
            None => cliptoo_core::SendToApp {
                name: derive_app_name(entry),
                path: entry.to_string(),
            },
        })
        .collect()
}

/// Render `SendToApp`s back into the comma-separated `Name: path` form the
/// settings text field shows. An entry whose name was derived from its path
/// (a bare path like `gedit` parses to name == path) stays bare so a
/// format+parse round-trip preserves the user's original form instead of
/// rewriting `gedit` into `gedit: gedit`.
pub(super) fn format_send_to_apps(apps: &[cliptoo_core::SendToApp]) -> String {
    apps.iter()
        .map(|a| {
            if a.name == a.path {
                a.path.clone()
            } else {
                format!("{}: {}", a.name, a.path)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse a comma-separated list of app identifiers.
pub(super) fn parse_blacklist(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Decode a Slint key-event text into a readable key name. Slint encodes
/// special keys as control/private-use characters (Backspace → U+0008,
/// F5 → U+F708, …; see Slint's `key_codes` table), and these must be
/// translated before display or portal registration.
fn decode_slint_key(key: char) -> Option<String> {
    match key {
        '\u{0008}' => Some("Backspace".to_string()),
        '\u{0009}' => Some("Tab".to_string()),
        '\u{000a}' => Some("Return".to_string()),
        '\u{0010}' => Some("Shift".to_string()),
        '\u{0011}' => Some("Control".to_string()),
        '\u{0012}' => Some("Alt".to_string()),
        '\u{0013}' => Some("AltGr".to_string()),
        '\u{0014}' => Some("CapsLock".to_string()),
        '\u{0015}' => Some("ShiftR".to_string()),
        '\u{0016}' => Some("ControlR".to_string()),
        '\u{0017}' => Some("Meta".to_string()),
        '\u{0018}' => Some("MetaR".to_string()),
        '\u{0019}' => Some("Backtab".to_string()),
        '\u{0020}' => Some("Space".to_string()),
        '\u{007f}' => Some("Delete".to_string()),
        '\u{f700}' => Some("UpArrow".to_string()),
        '\u{f701}' => Some("DownArrow".to_string()),
        '\u{f702}' => Some("LeftArrow".to_string()),
        '\u{f703}' => Some("RightArrow".to_string()),
        '\u{f704}'..='\u{f71b}' => Some(format!("F{}", key as u32 - 0xf704 + 1)),
        '\u{f727}' => Some("Insert".to_string()),
        '\u{f729}' => Some("Home".to_string()),
        '\u{f72b}' => Some("End".to_string()),
        '\u{f72c}' => Some("PageUp".to_string()),
        '\u{f72d}' => Some("PageDown".to_string()),
        '\u{f72f}' => Some("ScrollLock".to_string()),
        '\u{f730}' => Some("Pause".to_string()),
        '\u{f731}' => Some("SysReq".to_string()),
        '\u{f734}' => Some("Stop".to_string()),
        '\u{f735}' => Some("Menu".to_string()),
        '\u{f748}' => Some("Back".to_string()),
        _ => None,
    }
}

/// Uppercase single-character keys (letters/digits) for display; named keys
/// like `Backspace` or `F5` keep their case.
fn display_key(key: &str) -> String {
    if key.chars().count() == 1 {
        key.to_uppercase()
    } else {
        key.to_string()
    }
}

/// Normalise a hotkey string captured from the settings UI. Slint's
/// special-key encodings are decoded to readable names (`\u{0008}` →
/// `Backspace`, `\u{F708}` → `F5`), and the key token is uppercased when it
/// is a single letter/digit so the assigned key displays uppercase (e.g.
/// `Ctrl+Alt+q` → `Ctrl+Alt+Q`).
///
/// A trailing `+` is the plus key itself (`Ctrl++` = Ctrl + the plus key)
/// when a `+` precedes it; a lone trailing `+` is a dangling separator from
/// capturing a modifier without a key and is dropped.
pub(super) fn clean_hotkey_text(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match decode_slint_key(c) {
            Some(name) => name,
            None => c.to_string(),
        })
        .collect();
    match cleaned.strip_suffix('+') {
        // `before` ends with `+`, so the string was `<mods>+` + the `+` key.
        Some(before) if before.ends_with('+') => {
            let mut out = before.trim_end_matches('+').to_string();
            out.push_str("++");
            out
        }
        // A lone `+` is the bare plus key itself.
        Some("") => cleaned.to_uppercase(),
        // The trailing `+` was a separator with no key after it.
        Some(_) => cleaned.trim_end_matches('+').to_uppercase(),
        None => match cleaned.rsplit_once('+') {
            Some((mods, key)) if !key.is_empty() => format!("{mods}+{}", display_key(key)),
            // Bare key: no `+` to split on.
            _ => display_key(&cleaned),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_send_to_apps_handles_name_path_and_bare_paths() {
        let apps = parse_send_to_apps("code: /usr/bin/code, gedit");
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "code");
        assert_eq!(apps[0].path, "/usr/bin/code");
        assert_eq!(apps[1].name, "gedit");
        assert_eq!(apps[1].path, "gedit");

        // Empty entries and whitespace are dropped; a missing name is derived
        // from the path's file stem.
        let apps = parse_send_to_apps("  ,, /opt/tools/meld, : /usr/bin/kompare ,");
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "meld");
        assert_eq!(apps[1].name, "kompare");
        assert_eq!(apps[1].path, "/usr/bin/kompare");
    }

    #[test]
    fn send_to_apps_round_trip_format_and_parse() {
        let apps = parse_send_to_apps("code: /usr/bin/code, gedit");
        let formatted = format_send_to_apps(&apps);
        // A bare path stays bare (name == path); a named entry keeps its name.
        assert_eq!(formatted, "code: /usr/bin/code, gedit");
        let round = parse_send_to_apps(&formatted);
        assert_eq!(round.len(), 2);
        assert_eq!(round[1].name, "gedit");
        assert_eq!(round[1].path, "gedit");
    }

    #[test]
    fn send_to_apps_keeps_name_for_directory_paths() {
        // A path with a directory derives its name from the stem, so it is
        // rendered back with the explicit `name: path` form.
        let apps = parse_send_to_apps("/opt/tools/meld");
        assert_eq!(format_send_to_apps(&apps), "meld: /opt/tools/meld");
    }

    #[test]
    fn parse_blacklist_trims_and_drops_empty() {
        assert_eq!(parse_blacklist(""), Vec::<String>::new());
        assert_eq!(
            parse_blacklist("  ,, org.kde.dolphin ,, kwrite "),
            vec!["org.kde.dolphin".to_string(), "kwrite".to_string()]
        );
    }

    #[test]
    fn clean_hotkey_text_uppercases_single_key() {
        assert_eq!(clean_hotkey_text("Ctrl+Alt+q"), "Ctrl+Alt+Q");
        assert_eq!(clean_hotkey_text("a"), "A");
        assert_eq!(clean_hotkey_text("F5"), "F5");
    }

    #[test]
    fn clean_hotkey_text_keeps_plus_key() {
        assert_eq!(clean_hotkey_text("Ctrl++"), "Ctrl++");
        assert_eq!(clean_hotkey_text("+"), "+");
    }

    #[test]
    fn clean_hotkey_text_drops_dangling_separator() {
        assert_eq!(clean_hotkey_text("Ctrl+"), "CTRL");
    }

    #[test]
    fn clean_hotkey_text_decodes_slint_special_keys() {
        assert_eq!(clean_hotkey_text("Ctrl+\u{0008}"), "Ctrl+Backspace");
        assert_eq!(clean_hotkey_text("Ctrl+\u{0009}"), "Ctrl+Tab");
        assert_eq!(clean_hotkey_text("\u{F708}"), "F5");
        assert_eq!(clean_hotkey_text("Ctrl+\u{F70f}"), "Ctrl+F12");
        assert_eq!(clean_hotkey_text("Ctrl+\u{F72c}"), "Ctrl+PageUp");
        assert_eq!(clean_hotkey_text("Ctrl+\u{0020}"), "Ctrl+Space");
    }

    #[test]
    fn decode_slint_key_maps_f_keys_and_pass_through() {
        assert_eq!(decode_slint_key('\u{f704}'), Some("F1".to_string()));
        assert_eq!(decode_slint_key('\u{f71b}'), Some("F24".to_string()));
        assert_eq!(decode_slint_key('a'), None);
        assert_eq!(decode_slint_key('\u{0008}'), Some("Backspace".to_string()));
    }

    #[test]
    fn display_key_uppercases_only_single_chars() {
        assert_eq!(display_key("q"), "Q");
        assert_eq!(display_key("F5"), "F5");
        assert_eq!(display_key("PageUp"), "PageUp");
    }

    #[test]
    fn idx_of_matches_case_insensitively() {
        let options = ["Right Alt", "Left Alt", "Control"];
        assert_eq!(idx_of("Left Alt", &options), 1);
        assert_eq!(idx_of("left alt", &options), 1);
        assert_eq!(idx_of("CONTROL", &options), 2);
    }

    #[test]
    fn idx_of_falls_back_to_default_for_unknown_value() {
        assert_eq!(
            idx_of("Super Key", &["Right Alt", "Left Alt", "Control"]),
            0
        );
        assert_eq!(idx_of("", &["Debug", "Info", "Warn", "Error"]), 0);
    }
}

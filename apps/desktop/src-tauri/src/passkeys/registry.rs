//! The Windows plugin's COM entry, `HKCU\Software\Classes\CLSID\{…}`, as
//! read from the registry, and what in it isn't UwULock's (R8 C-3). Pure,
//! so it is tested on every platform; `windows.rs` reads the key and acts.
//!
//! UwULock writes exactly one thing there: the default value of
//! `LocalServer32`, a `REG_SZ` with its own command. A program of the same
//! user can redirect activation in other ways too: another default, a
//! `REG_EXPAND_SZ` one, a `ServerExecutable` value next to it, a `TreatAs`
//! (or `InprocServer32`, `AutoConvertTo`, …) subkey. Anything but UwULock's
//! own value is reported, and the key is then deleted and written anew.

/// A value's data, as far as the comparison needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Data {
    /// `REG_SZ` (`expand: false`) or `REG_EXPAND_SZ`.
    Text { expand: bool, text: String },
    /// Any other type (its `REG_*` number).
    Other(u32),
    /// More than UwULock reads (`ERROR_MORE_DATA`): different by itself.
    TooLong,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Value {
    /// `""` for the default value.
    pub name: String,
    pub data: Data,
}

/// A registry key: its values and subkeys. `cut` when there was more than
/// UwULock reads (too many entries, too deep, or a key it couldn't open).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Key {
    pub values: Vec<Value>,
    pub subkeys: Vec<(String, Key)>,
    pub cut: bool,
}

/// The only subkey UwULock writes.
pub(crate) const SERVER: &str = "LocalServer32";

/// Whether two commands are the same (Windows paths ignore case; a
/// trailing NUL from the registry doesn't count).
pub(crate) fn same_command(a: &str, b: &str) -> bool {
    let clean = |text: &str| text.trim_end_matches('\0').trim().to_string();
    clean(a).eq_ignore_ascii_case(&clean(b))
}

/// What the CLSID key holds that isn't UwULock's own `LocalServer32`
/// default with the command `ours`, one line each (`path = data`). Empty
/// when the key is only ours, or not there yet.
pub(crate) fn unexpected(clsid: &Key, ours: &str) -> Vec<String> {
    let mut found = Vec::new();
    if clsid.cut {
        found.push("(more than UwULock reads)".to_string());
    }
    for value in &clsid.values {
        found.push(describe("", value));
    }
    for (name, key) in &clsid.subkeys {
        if !name.eq_ignore_ascii_case(SERVER) {
            everything(name, key, &mut found);
            continue;
        }
        if key.cut {
            found.push(format!("{name} (more than UwULock reads)"));
        }
        for value in &key.values {
            let ours = value.name.is_empty()
                && matches!(&value.data, Data::Text { expand: false, text } if same_command(text, ours));
            if !ours {
                found.push(describe(name, value));
            }
        }
        for (sub, below) in &key.subkeys {
            everything(&format!("{name}\\{sub}"), below, &mut found);
        }
    }
    found
}

/// Every value under `path`, or the key alone when it has none.
fn everything(path: &str, key: &Key, found: &mut Vec<String>) {
    if key.values.is_empty() && key.subkeys.is_empty() {
        found.push(format!("{path} (key)"));
    }
    if key.cut {
        found.push(format!("{path} (more than UwULock reads)"));
    }
    for value in &key.values {
        found.push(describe(path, value));
    }
    for (sub, below) in &key.subkeys {
        everything(&format!("{path}\\{sub}"), below, found);
    }
}

fn describe(path: &str, value: &Value) -> String {
    let name = if value.name.is_empty() {
        "(default)"
    } else {
        value.name.as_str()
    };
    let at = if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}\\{name}")
    };
    match &value.data {
        Data::Text {
            expand: false,
            text,
        } => format!("{at} = {:?}", text.trim_end_matches('\0')),
        Data::Text { expand: true, text } => {
            format!("{at} = {:?} (REG_EXPAND_SZ)", text.trim_end_matches('\0'))
        }
        Data::Other(kind) => format!("{at} (type {kind})"),
        Data::TooLong => format!("{at} (too long to read)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str =
        r#""C:\Users\nyu\AppData\Local\UwULock\uwulock-desktop.exe" --passkey-plugin"#;

    fn text(name: &str, text: &str) -> Value {
        Value {
            name: name.into(),
            data: Data::Text {
                expand: false,
                text: text.into(),
            },
        }
    }

    fn server(values: Vec<Value>) -> Key {
        Key {
            subkeys: vec![(
                SERVER.into(),
                Key {
                    values,
                    ..Key::default()
                },
            )],
            ..Key::default()
        }
    }

    #[test]
    fn ours_alone_is_fine() {
        assert!(unexpected(&Key::default(), OURS).is_empty());
        assert!(unexpected(&server(vec![]), OURS).is_empty());
        assert!(unexpected(&server(vec![text("", OURS)]), OURS).is_empty());
        // Case and a trailing NUL don't matter, the key name's case neither.
        let mut key = server(vec![text("", &format!("{}\0", OURS.to_uppercase()))]);
        key.subkeys[0].0 = "localserver32".into();
        assert!(unexpected(&key, OURS).is_empty());
    }

    #[test]
    fn another_default_is_found() {
        let found = unexpected(&server(vec![text("", r"C:\evil.exe")]), OURS);
        assert_eq!(found, vec![r#"LocalServer32\(default) = "C:\\evil.exe""#]);
        // Ours, but as REG_EXPAND_SZ: not what UwULock writes.
        let expand = server(vec![Value {
            name: String::new(),
            data: Data::Text {
                expand: true,
                text: OURS.into(),
            },
        }]);
        assert_eq!(unexpected(&expand, OURS).len(), 1);
        assert!(unexpected(&expand, OURS)[0].ends_with("(REG_EXPAND_SZ)"));
        // Too long to read, or another type: different.
        for data in [Data::TooLong, Data::Other(3)] {
            let key = server(vec![Value {
                name: String::new(),
                data,
            }]);
            assert_eq!(unexpected(&key, OURS).len(), 1);
        }
    }

    #[test]
    fn hijacks_next_to_ours_are_found() {
        // ServerExecutable next to our default.
        let key = server(vec![
            text("", OURS),
            text("ServerExecutable", r"C:\evil.exe"),
        ]);
        assert_eq!(
            unexpected(&key, OURS),
            vec![r#"LocalServer32\ServerExecutable = "C:\\evil.exe""#]
        );
        // TreatAs under the CLSID, an InprocServer32, an empty key, a
        // value on the CLSID key itself, something below LocalServer32.
        let mut key = server(vec![text("", OURS)]);
        key.subkeys.push((
            "TreatAs".into(),
            Key {
                values: vec![text("", "{00000000-0000-0000-0000-000000000001}")],
                ..Key::default()
            },
        ));
        key.subkeys.push((
            "InprocServer32".into(),
            Key {
                values: vec![text("", r"C:\evil.dll"), text("ThreadingModel", "Both")],
                ..Key::default()
            },
        ));
        key.subkeys.push(("AutoConvertTo".into(), Key::default()));
        key.values.push(text("AppID", "{x}"));
        key.subkeys[0].1.subkeys.push((
            "deeper".into(),
            Key {
                cut: true,
                ..Key::default()
            },
        ));
        assert_eq!(
            unexpected(&key, OURS),
            vec![
                r#"AppID = "{x}""#,
                r"LocalServer32\deeper (key)",
                r"LocalServer32\deeper (more than UwULock reads)",
                r#"TreatAs\(default) = "{00000000-0000-0000-0000-000000000001}""#,
                r#"InprocServer32\(default) = "C:\\evil.dll""#,
                r#"InprocServer32\ThreadingModel = "Both""#,
                "AutoConvertTo (key)",
            ]
        );
        // More than UwULock read: different.
        let cut = Key {
            cut: true,
            ..server(vec![text("", OURS)])
        };
        assert_eq!(unexpected(&cut, OURS).len(), 1);
    }
}

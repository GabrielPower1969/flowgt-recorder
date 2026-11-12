//! Global shortcuts. A press acts on the recorder directly (the webview may be hidden) and is
//! also broadcast as `shortcut://<name>` so open windows can react.

use recorder_core::Settings;
use std::collections::BTreeMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// What happened to each shortcut at the last registration: "ok", "taken", "invalid", "unset".
static STATUS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

pub fn status() -> BTreeMap<String, String> {
    STATUS.lock().expect("shortcut status lock").clone()
}

/// The four actions and their accelerators, in a fixed order.
pub fn named(s: &Settings) -> [(&'static str, &str); 4] {
    [
        ("toggle", s.shortcuts.toggle.as_str()),
        ("pause", s.shortcuts.pause.as_str()),
        ("scrollCapture", s.shortcuts.scroll_capture.as_str()),
        ("showWindow", s.shortcuts.show_window.as_str()),
    ]
}

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let Ok(settings) = crate::commands::load_settings(app) else {
                return;
            };
            let Some((name, _)) = named(&settings).into_iter().find(|(_, accel)| {
                accel.trim().parse::<Shortcut>().ok().as_ref() == Some(shortcut)
            }) else {
                return;
            };
            let handle = app.clone();
            let _ = handle.emit(&format!("shortcut://{name}"), ());
            match name {
                "toggle" => {
                    tauri::async_runtime::spawn(async move {
                        crate::commands::toggle_recording(&handle).await
                    });
                }
                "pause" => {
                    tauri::async_runtime::spawn(async move {
                        crate::commands::toggle_pause(&handle).await
                    });
                }
                "scrollCapture" => {
                    tauri::async_runtime::spawn(async move {
                        crate::commands::toggle_scroll_capture(&handle).await
                    });
                }
                "showWindow" => crate::windows::toggle_main(&handle),
                _ => {}
            }
        })
        .build()
}

/// (Re)register the shortcuts from the settings. A conflict is not fatal: it is logged and kept
/// in [`status`] so the settings page can say which one another program already holds.
pub fn register(app: &AppHandle, settings: &Settings) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut status = BTreeMap::new();
    for (name, accel) in named(settings) {
        let accel = accel.trim();
        let outcome = if accel.is_empty() {
            "unset"
        } else {
            match accel.parse::<Shortcut>() {
                Ok(sc) => match gs.register(sc) {
                    Ok(()) => "ok",
                    Err(e) => {
                        log::warn!("cannot register the {name} shortcut {accel}: {e}");
                        "taken"
                    }
                },
                Err(e) => {
                    log::warn!("invalid {name} shortcut {accel:?}: {e}");
                    "invalid"
                }
            }
        };
        status.insert(name.to_string(), outcome.to_string());
    }
    *STATUS.lock().expect("shortcut status lock") = status;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_accelerators_parse() {
        for a in [
            "CmdOrCtrl+Shift+R",
            "CmdOrCtrl+Shift+P",
            "Alt+R",
            "Alt+Shift+S",
        ] {
            assert!(a.parse::<Shortcut>().is_ok(), "{a}");
        }
        assert!("Nonsense+".parse::<Shortcut>().is_err());
    }

    #[test]
    fn every_default_shortcut_has_a_name() {
        let s = recorder_core::settings::default_settings(recorder_core::Platform::MacOs);
        let names: Vec<_> = named(&s).iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["toggle", "pause", "scrollCapture", "showWindow"]);
        assert_eq!(named(&s)[2].1, "Alt+Shift+S");
    }
}

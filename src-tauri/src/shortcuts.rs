//! Global shortcuts. A press acts on the recorder directly (the webview may be hidden) and is
//! also broadcast as `shortcut://toggle|pause` so open windows can react.

use recorder_core::Settings;
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let handle = app.clone();
            let toggle = current(&handle).and_then(|(t, _)| t.parse::<Shortcut>().ok());
            let pause = current(&handle).and_then(|(_, p)| p.parse::<Shortcut>().ok());
            if Some(*shortcut) == toggle {
                let _ = handle.emit("shortcut://toggle", ());
                tauri::async_runtime::spawn(async move { crate::commands::toggle_recording(&handle).await });
            } else if Some(*shortcut) == pause {
                let _ = handle.emit("shortcut://pause", ());
                tauri::async_runtime::spawn(async move { crate::commands::toggle_pause(&handle).await });
            }
        })
        .build()
}

fn current(app: &AppHandle) -> Option<(String, String)> {
    let s = crate::commands::load_settings(app).ok()?;
    Some((s.shortcuts.toggle, s.shortcuts.pause))
}

/// (Re)register the two shortcuts from the settings. Conflicts are logged, not fatal.
pub fn register(app: &AppHandle, settings: &Settings) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    for (name, accel) in [("toggle", &settings.shortcuts.toggle), ("pause", &settings.shortcuts.pause)] {
        match accel.parse::<Shortcut>() {
            Ok(sc) => {
                if let Err(e) = gs.register(sc) {
                    log::warn!("cannot register the {name} shortcut {accel}: {e}");
                }
            }
            Err(e) => log::warn!("invalid {name} shortcut {accel:?}: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_accelerators_parse() {
        for a in ["CmdOrCtrl+Shift+R", "CmdOrCtrl+Shift+P", "Alt+R"] {
            assert!(a.parse::<Shortcut>().is_ok(), "{a}");
        }
        assert!("Nonsense+".parse::<Shortcut>().is_err());
    }
}

//! Windows: answer WebView2's camera and microphone requests for the app's own pages.
//!
//! The self-view and the level meter use `getUserMedia`. WebView2 treats that like a web page
//! asking: it shows "http://tauri.localhost wants to use your cameras" with Block / Allow, once
//! per device kind. A desktop recorder asking its own user for its own camera in a browser bubble
//! is confusing, and the bubble ends up in the recording when it is still open. Windows already
//! has the real switch (Settings, Privacy, camera / microphone for desktop apps); this module only
//! removes the second, browser-style question, and only for this app's own origin.

/// True for the pages the app itself serves: the bundled frontend, and the dev server in debug builds.
pub fn is_own_origin(uri: &str, debug_build: bool) -> bool {
    let own = ["http://tauri.localhost", "https://tauri.localhost"];
    let dev = ["http://localhost:1420"];
    let matches = |prefixes: &[&str]| {
        prefixes.iter().any(|p| uri == *p || uri.strip_prefix(p).is_some_and(|rest| rest.starts_with('/')))
    };
    matches(&own) || (debug_build && matches(&dev))
}

/// Allow camera and microphone for the app's own origin in this window; every other request and
/// every other origin keeps WebView2's default behaviour.
#[cfg(windows)]
pub fn allow_own_media(win: &tauri::WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
        COREWEBVIEW2_PERMISSION_STATE_ALLOW,
    };
    use webview2_com::{take_pwstr, PermissionRequestedEventHandler};
    use windows_core::PWSTR;

    let result = win.with_webview(|webview| {
        // SAFETY: called by Tauri on the thread that owns the webview; the COM objects are live
        // for the duration of the call and the handler is kept alive by WebView2 itself.
        unsafe {
            let Ok(core) = webview.controller().CoreWebView2() else {
                log::warn!("webview permissions: no CoreWebView2");
                return;
            };
            let handler = PermissionRequestedEventHandler::create(Box::new(|_sender, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                args.PermissionKind(&mut kind)?;
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let uri = take_pwstr(uri);
                let media = kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA || kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE;
                if media && is_own_origin(&uri, cfg!(debug_assertions)) {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
                }
                Ok(())
            }));
            let mut token = Default::default();
            if let Err(e) = core.add_PermissionRequested(&handler, &mut token) {
                log::warn!("webview permissions: cannot register the handler: {e}");
            }
        }
    });
    if let Err(e) = result {
        log::warn!("webview permissions: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_apps_own_pages_are_answered() {
        assert!(is_own_origin("http://tauri.localhost/", false));
        assert!(is_own_origin("https://tauri.localhost/index.html", false));
        assert!(is_own_origin("http://tauri.localhost", false));
        // The dev server counts in debug builds only.
        assert!(is_own_origin("http://localhost:1420/", true));
        assert!(!is_own_origin("http://localhost:1420/", false));
        // Look-alikes and everything else are left to WebView2.
        assert!(!is_own_origin("http://tauri.localhost.evil.example/", true));
        assert!(!is_own_origin("http://localhost:14200/", true));
        assert!(!is_own_origin("https://example.com/", true));
        assert!(!is_own_origin("", true));
    }
}

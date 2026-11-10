//! Google sign-in and upload to Google Drive (D19).
//!
//! Sign-in is OAuth 2 for desktop apps: PKCE plus a one-shot loopback server, in the user's own
//! browser. The scope is `drive.file`, so the app can only see the files and the folder it made
//! itself. Uploads are resumable, in chunks, and survive transient server errors.

use crate::error::{CmdResult, CommandError};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

pub const FOLDER_NAME: &str = "FlowGT Recorder";
const SCOPES: &str = "openid email https://www.googleapis.com/auth/drive.file";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
/// Must be a multiple of 256 KiB (Drive API rule).
const CHUNK: u64 = 8 * 1024 * 1024;
const MAX_RETRIES: u32 = 8;
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
const ACCOUNT_FILE: &str = "google-account.json";

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub auth: String,
    pub token: String,
    pub userinfo: String,
    pub api: String,
    pub upload: String,
}

impl Endpoints {
    pub fn google() -> Self {
        Endpoints {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            userinfo: "https://openidconnect.googleapis.com/v1/userinfo".into(),
            api: "https://www.googleapis.com/drive/v3".into(),
            upload: "https://www.googleapis.com/upload/drive/v3".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OAuthClient {
    pub id: String,
    /// Google requires it for desktop clients but documents that it is not a secret there.
    pub secret: String,
}

/// The client compiled into this build (`FLOWGT_GOOGLE_CLIENT_ID`, `FLOWGT_GOOGLE_CLIENT_SECRET`).
pub fn oauth_client() -> Option<OAuthClient> {
    match (option_env!("FLOWGT_GOOGLE_CLIENT_ID"), option_env!("FLOWGT_GOOGLE_CLIENT_SECRET")) {
        (Some(id), Some(secret)) if !id.is_empty() => Some(OAuthClient { id: id.to_string(), secret: secret.to_string() }),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub email: String,
    pub refresh_token: String,
}

// ───────────────────────────── account file ─────────────────────────────

fn account_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(ACCOUNT_FILE))
}

pub fn load_account(app: &AppHandle) -> Option<Account> {
    let text = std::fs::read_to_string(account_path(app)?).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_account(app: &AppHandle, account: &Account) -> CmdResult<()> {
    let path = account_path(app).ok_or_else(|| CommandError::io("no config directory"))?;
    write_private(&path, &serde_json::to_string(account)?)?;
    Ok(())
}

pub fn forget_account(app: &AppHandle) {
    if let Some(p) = account_path(app) {
        let _ = std::fs::remove_file(p);
    }
}

/// Readable by this user only.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

// ───────────────────────────── sign-in ─────────────────────────────

/// (verifier, S256 challenge). Two v4 UUIDs give 244 random bits in 64 unreserved characters.
pub fn pkce_pair() -> (String, String) {
    let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

pub fn auth_url(ep: &Endpoints, client: &OAuthClient, redirect: &str, state: &str, challenge: &str) -> String {
    reqwest::Url::parse_with_params(
        &ep.auth,
        &[
            ("client_id", client.id.as_str()),
            ("redirect_uri", redirect),
            ("response_type", "code"),
            ("scope", SCOPES),
            ("state", state),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("access_type", "offline"),
            // Without this Google only returns a refresh token the first time.
            ("prompt", "consent"),
        ],
    )
    .map(|u| u.to_string())
    .unwrap_or_default()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Callback {
    Code(String),
    Denied(String),
    /// Not the OAuth redirect (a browser asking for /favicon.ico, say): keep waiting.
    Other,
}

/// Parse the first line of the HTTP request the browser sends to the loopback server.
pub fn parse_callback(request_line: &str, expected_state: &str) -> Callback {
    let Some(target) = request_line.split_whitespace().nth(1) else { return Callback::Other };
    let Ok(url) = reqwest::Url::parse(&format!("http://localhost{target}")) else { return Callback::Other };
    let get = |k: &str| url.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned());
    if get("state").as_deref() != Some(expected_state) {
        return Callback::Other;
    }
    match (get("code"), get("error")) {
        (Some(code), _) => Callback::Code(code),
        (_, Some(err)) => Callback::Denied(err),
        _ => Callback::Other,
    }
}

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>FlowGT Recorder</title><body style=\"font:16px system-ui;text-align:center;padding:64px\">已登录，可以回到 FlowGT Recorder。<br>Signed in. You can go back to FlowGT Recorder.";

/// Open the consent page and wait for the redirect, then trade the code for tokens.
pub async fn sign_in(ep: &Endpoints, client: &OAuthClient, open: impl FnOnce(&str)) -> CmdResult<Account> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let (verifier, challenge) = pkce_pair();
    let state = uuid::Uuid::new_v4().simple().to_string();
    open(&auth_url(ep, client, &redirect, &state, &challenge));

    let wait = async {
        loop {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await?;
            let head = String::from_utf8_lossy(&buf[..n]).into_owned();
            let parsed = parse_callback(head.lines().next().unwrap_or(""), &state);
            let body = if parsed == Callback::Other { "" } else { DONE_PAGE };
            let reply = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = sock.write_all(reply.as_bytes()).await;
            let _ = sock.shutdown().await;
            if parsed != Callback::Other {
                return Ok::<Callback, std::io::Error>(parsed);
            }
        }
    };
    let callback = tokio::time::timeout(SIGN_IN_TIMEOUT, wait).await.map_err(|_| signin_err("the browser sign-in timed out"))??;
    let code = match callback {
        Callback::Code(c) => c,
        Callback::Denied(e) => return Err(signin_err(&format!("Google refused: {e}"))),
        Callback::Other => return Err(signin_err("no answer from Google")),
    };

    let http = reqwest::Client::new();
    let tokens: serde_json::Value = http
        .post(&ep.token)
        .form(&[
            ("code", code.as_str()),
            ("client_id", client.id.as_str()),
            ("client_secret", client.secret.as_str()),
            ("code_verifier", verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect.as_str()),
        ])
        .send()
        .await
        .map_err(net)?
        .json()
        .await
        .map_err(net)?;
    let refresh = tokens["refresh_token"].as_str().ok_or_else(|| signin_err(&format!("no refresh token: {}", tokens["error"])))?;
    let access = tokens["access_token"].as_str().unwrap_or_default();
    let who: serde_json::Value = http.get(&ep.userinfo).bearer_auth(access).send().await.map_err(net)?.json().await.map_err(net)?;
    Ok(Account { email: who["email"].as_str().unwrap_or("Google").to_string(), refresh_token: refresh.to_string() })
}

fn signin_err(detail: &str) -> CommandError {
    CommandError::new("error.driveSignIn", detail)
}

fn net(e: reqwest::Error) -> CommandError {
    CommandError::new("error.driveNetwork", e.to_string())
}

// ───────────────────────────── upload ─────────────────────────────

/// `bytes 0-8388607/20000000`
pub fn content_range(offset: u64, len: u64, total: u64) -> String {
    format!("bytes {}-{}/{}", offset, offset + len - 1, total)
}

/// From the `Range: bytes=0-8388607` of a 308 answer: where to continue. No header = nothing stored.
pub fn next_offset(range: Option<&str>) -> u64 {
    range
        .and_then(|r| r.rsplit('-').next())
        .and_then(|end| end.trim().parse::<u64>().ok())
        .map_or(0, |end| end + 1)
}

/// Drive's search language needs backslash and single quote escaped.
pub fn folder_query(name: &str) -> String {
    let safe = name.replace('\\', "\\\\").replace('\'', "\\'");
    format!("name = '{safe}' and mimeType = '{FOLDER_MIME}' and 'root' in parents and trashed = false")
}

struct Auth<'a> {
    http: reqwest::Client,
    ep: &'a Endpoints,
    client: Option<&'a OAuthClient>,
    refresh: &'a str,
    access: String,
}

impl Auth<'_> {
    async fn refresh(&mut self) -> Result<(), String> {
        let Some(client) = self.client else { return Err("the access token expired".into()) };
        let v: serde_json::Value = self
            .http
            .post(&self.ep.token)
            .form(&[("client_id", client.id.as_str()), ("client_secret", client.secret.as_str()), ("refresh_token", self.refresh), ("grant_type", "refresh_token")])
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        self.access = v["access_token"].as_str().ok_or_else(|| format!("cannot refresh the Google sign-in: {}", v["error"]))?.to_string();
        Ok(())
    }

    /// The `FlowGT Recorder` folder in the Drive home, created the first time.
    async fn ensure_folder(&self, folder_name: &str) -> Result<String, String> {
        let found: serde_json::Value = self
            .http
            .get(format!("{}/files", self.ep.api))
            .query(&[("q", folder_query(folder_name).as_str()), ("fields", "files(id)"), ("spaces", "drive")])
            .bearer_auth(&self.access)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        if let Some(id) = found["files"][0]["id"].as_str() {
            return Ok(id.to_string());
        }
        let made: serde_json::Value = self
            .http
            .post(format!("{}/files", self.ep.api))
            .query(&[("fields", "id")])
            .bearer_auth(&self.access)
            .json(&serde_json::json!({ "name": folder_name, "mimeType": FOLDER_MIME, "parents": ["root"] }))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        made["id"].as_str().map(str::to_string).ok_or_else(|| "Drive did not return the new folder".to_string())
    }

    async fn open_session(&self, folder: &str, name: &str, total: u64) -> Result<String, String> {
        let resp = self
            .http
            .post(format!("{}/files", self.ep.upload))
            .query(&[("uploadType", "resumable"), ("fields", "id,webViewLink,size")])
            .bearer_auth(&self.access)
            .header("X-Upload-Content-Type", "video/mp4")
            .header("X-Upload-Content-Length", total)
            .json(&serde_json::json!({ "name": name, "parents": [folder] }))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        resp.headers().get("location").and_then(|v| v.to_str().ok()).map(str::to_string).ok_or_else(|| "Drive gave no upload address".to_string())
    }
}

/// A finished upload, with what Drive says it stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uploaded {
    pub link: String,
    pub size: Option<u64>,
}

impl Uploaded {
    /// Drive confirmed exactly this many bytes. A missing size is not a confirmation.
    pub fn confirms(&self, local_bytes: u64) -> bool {
        self.size == Some(local_bytes)
    }
}

enum Step {
    Continue(u64),
    Done(Uploaded),
    Retry,
    Reauth,
}

fn interpret(status: u16, range: Option<String>, body: &str) -> Result<Step, String> {
    match status {
        200 | 201 => {
            let v: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
            let link = v["webViewLink"].as_str().map(str::to_string).or_else(|| v["id"].as_str().map(|id| format!("https://drive.google.com/file/d/{id}/view")));
            // Drive sends `size` as a string.
            let size = v["size"].as_str().and_then(|s| s.parse().ok()).or_else(|| v["size"].as_u64());
            link.map(|link| Step::Done(Uploaded { link, size })).ok_or_else(|| "Drive did not return a link".to_string())
        }
        308 => Ok(Step::Continue(next_offset(range.as_deref()))),
        401 => Ok(Step::Reauth),
        408 | 429 | 500..=599 => Ok(Step::Retry),
        other => Err(format!("Drive answered {other}: {}", body.chars().take(300).collect::<String>())),
    }
}

/// What the app's Drive folder holds. No folder yet means nothing was uploaded.
pub async fn list_recordings(ep: &Endpoints, client: Option<&OAuthClient>, refresh: &str, access: Option<String>) -> Result<Vec<recorder_core::RecordingEntry>, String> {
    let mut auth = Auth { http: reqwest::Client::new(), ep, client, refresh, access: access.unwrap_or_default() };
    if auth.access.is_empty() {
        auth.refresh().await?;
    }
    let get = |q: String, fields: &'static str| {
        let req = auth.http.get(format!("{}/files", ep.api)).query(&[("q", q.as_str()), ("fields", fields), ("spaces", "drive"), ("pageSize", "200"), ("orderBy", "createdTime desc")]).bearer_auth(&auth.access);
        async move { req.send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?.json::<serde_json::Value>().await.map_err(|e| e.to_string()) }
    };
    let found = get(folder_query(FOLDER_NAME), "files(id)").await?;
    let Some(folder) = found["files"][0]["id"].as_str() else { return Ok(Vec::new()) };
    let listed = get(format!("'{folder}' in parents and trashed = false"), "files(id,name,size,webViewLink,createdTime)").await?;
    Ok(entries_from(&listed))
}

fn entries_from(listed: &serde_json::Value) -> Vec<recorder_core::RecordingEntry> {
    listed["files"]
        .as_array()
        .map(|files| {
            files
                .iter()
                .filter_map(|f| {
                    Some(recorder_core::RecordingEntry {
                        name: f["name"].as_str()?.to_string(),
                        bytes: f["size"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0),
                        drive_url: f["webViewLink"].as_str().map(str::to_string),
                        drive_created: f["createdTime"].as_str().map(str::to_string),
                        ..Default::default()
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Upload one file to the app's folder and return its link. `access` skips the first token
/// refresh (tests and one-off verification); `progress` gets 0–100.
pub async fn upload_recording(ep: &Endpoints, client: Option<&OAuthClient>, refresh: &str, access: Option<String>, path: &Path, progress: impl Fn(f64)) -> Result<Uploaded, String> {
    upload_to_folder(ep, client, refresh, access, FOLDER_NAME, path, progress).await
}

async fn upload_to_folder(ep: &Endpoints, client: Option<&OAuthClient>, refresh: &str, access: Option<String>, folder_name: &str, path: &Path, progress: impl Fn(f64)) -> Result<Uploaded, String> {
    let mut auth = Auth { http: reqwest::Client::new(), ep, client, refresh, access: access.unwrap_or_default() };
    if auth.access.is_empty() {
        auth.refresh().await?;
    }
    let total = tokio::fs::metadata(path).await.map_err(|e| e.to_string())?.len();
    if total == 0 {
        return Err("the file is empty".into());
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "recording.mp4".into());
    let folder = auth.ensure_folder(folder_name).await?;
    let session = auth.open_session(&folder, &name, total).await?;

    let mut file = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
    let mut offset = 0u64;
    let mut failures = 0u32;
    loop {
        let len = CHUNK.min(total - offset);
        file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf).await.map_err(|e| e.to_string())?;

        let sent = auth.http.put(&session).bearer_auth(&auth.access).header("Content-Range", content_range(offset, len, total)).body(buf).send().await;
        let step = match sent {
            Ok(r) => {
                let status = r.status().as_u16();
                let range = r.headers().get("range").and_then(|v| v.to_str().ok()).map(str::to_string);
                interpret(status, range, &r.text().await.unwrap_or_default())?
            }
            Err(_) => Step::Retry,
        };
        match step {
            Step::Done(done) => {
                progress(100.0);
                return Ok(done);
            }
            Step::Continue(next) => {
                failures = 0;
                offset = next;
                progress(offset as f64 / total as f64 * 100.0);
            }
            Step::Reauth => {
                failures += 1;
                if failures > MAX_RETRIES {
                    return Err("Google keeps refusing the sign-in".into());
                }
                auth.refresh().await?;
            }
            Step::Retry => {
                failures += 1;
                if failures > MAX_RETRIES {
                    return Err("Drive kept failing, gave up after several tries".into());
                }
                tokio::time::sleep(Duration::from_millis(250u64 << failures.min(6))).await;
                // Ask how much actually arrived before sending more.
                if let Ok(r) = auth.http.put(&session).bearer_auth(&auth.access).header("Content-Range", format!("bytes */{total}")).body(Vec::new()).send().await {
                    let status = r.status().as_u16();
                    let range = r.headers().get("range").and_then(|v| v.to_str().ok()).map(str::to_string);
                    match interpret(status, range, &r.text().await.unwrap_or_default()) {
                        Ok(Step::Continue(next)) => offset = next,
                        Ok(Step::Done(done)) => return Ok(done),
                        _ => {}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn pkce_challenge_is_sha256_of_the_verifier() {
        let (v, c) = pkce_pair();
        assert_eq!(v.len(), 64);
        assert_eq!(c, base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(v.as_bytes())));
        assert!(!c.contains('=') && !c.contains('+') && !c.contains('/'));
        assert_ne!(pkce_pair().0, v);
    }

    #[test]
    fn auth_url_asks_only_for_the_narrow_scope_and_offline_access() {
        let c = OAuthClient { id: "id1".into(), secret: "s".into() };
        let u = auth_url(&Endpoints::google(), &c, "http://127.0.0.1:5000", "st", "ch");
        let url = reqwest::Url::parse(&u).unwrap();
        let get = |k: &str| url.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned()).unwrap();
        assert_eq!(get("client_id"), "id1");
        assert_eq!(get("redirect_uri"), "http://127.0.0.1:5000");
        assert_eq!(get("code_challenge_method"), "S256");
        assert_eq!(get("access_type"), "offline");
        assert!(get("scope").contains("drive.file"));
        assert!(!get("scope").split(' ').any(|s| s.ends_with("/auth/drive")), "never the full-drive scope");
        assert!(!u.contains("client_secret"));
    }

    #[test]
    fn callback_parsing_checks_the_state() {
        assert_eq!(parse_callback("GET /?state=ok&code=abc%2F1 HTTP/1.1", "ok"), Callback::Code("abc/1".into()));
        assert_eq!(parse_callback("GET /?state=ok&error=access_denied HTTP/1.1", "ok"), Callback::Denied("access_denied".into()));
        assert_eq!(parse_callback("GET /?state=forged&code=abc HTTP/1.1", "ok"), Callback::Other);
        assert_eq!(parse_callback("GET /favicon.ico HTTP/1.1", "ok"), Callback::Other);
        assert_eq!(parse_callback("", "ok"), Callback::Other);
    }

    #[test]
    fn chunk_arithmetic() {
        assert_eq!(content_range(0, 8, 20), "bytes 0-7/20");
        assert_eq!(content_range(16, 4, 20), "bytes 16-19/20");
        assert_eq!(next_offset(Some("bytes=0-7")), 8);
        assert_eq!(next_offset(None), 0);
        assert_eq!(next_offset(Some("junk")), 0);
        assert_eq!(CHUNK % (256 * 1024), 0);
    }

    #[test]
    fn folder_query_escapes_and_stays_in_the_home() {
        let q = folder_query("it's");
        assert!(q.contains(r"name = 'it\'s'"));
        assert!(q.contains("'root' in parents"));
        assert!(q.contains("trashed = false"));
    }

    #[test]
    fn account_file_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a/b.json");
        write_private(&p, "{}").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    /// A tiny scripted HTTP server standing in for Drive.
    #[derive(Default)]
    struct Fake {
        log: Vec<String>,
        stored: u64,
        folder_exists: bool,
        fail_next_put: bool,
        unauthorized_once: bool,
    }

    async fn serve(fake: Arc<Mutex<Fake>>) -> String {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let me = base.clone();
        tokio::spawn(async move {
            loop {
                let (mut s, _) = listener.accept().await.unwrap();
                let fake = fake.clone();
                let me = me.clone();
                tokio::spawn(async move {
                    let mut data = Vec::new();
                    let mut buf = [0u8; 65536];
                    loop {
                        let n = s.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        data.extend_from_slice(&buf[..n]);
                        let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                        let head = String::from_utf8_lossy(&data[..end]).into_owned();
                        let header = |k: &str| head.lines().find_map(|l| l.split_once(':').filter(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.trim().to_string()));
                        let want = header("content-length").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
                        if data.len() < end + 4 + want {
                            continue;
                        }
                        let line = head.lines().next().unwrap_or("").to_string();
                        let (status, extra, body) = {
                            let mut f = fake.lock().unwrap();
                            f.log.push(line.split(' ').take(2).collect::<Vec<_>>().join(" "));
                            if line.starts_with("GET /api/files") {
                                let files = if f.folder_exists { r#"{"files":[{"id":"F1"}]}"# } else { r#"{"files":[]}"# };
                                (200, String::new(), files.to_string())
                            } else if line.starts_with("POST /api/files") {
                                f.folder_exists = true;
                                (200, String::new(), r#"{"id":"F1"}"#.to_string())
                            } else if line.starts_with("POST /upload/files") {
                                (200, format!("Location: {me}/session\r\n"), String::new())
                            } else if line.starts_with("POST /token") {
                                (200, String::new(), r#"{"access_token":"fresh"}"#.to_string())
                            } else if line.starts_with("PUT /session") {
                                let cr = header("content-range").unwrap_or_default();
                                let total: u64 = cr.rsplit('/').next().unwrap().parse().unwrap();
                                let range = |st: u64| if st == 0 { String::new() } else { format!("Range: bytes=0-{}\r\n", st - 1) };
                                if cr.starts_with("bytes */") {
                                    (308, range(f.stored), String::new())
                                } else if f.unauthorized_once {
                                    f.unauthorized_once = false;
                                    (401, String::new(), String::new())
                                } else if f.fail_next_put {
                                    f.fail_next_put = false;
                                    (503, String::new(), String::new())
                                } else {
                                    f.stored += want as u64;
                                    if f.stored >= total {
                                        (200, String::new(), format!(r#"{{"id":"X","webViewLink":"https://drive.google.com/file/d/X/view","size":"{}"}}"#, f.stored))
                                    } else {
                                        (308, range(f.stored), String::new())
                                    }
                                }
                            } else {
                                (404, String::new(), String::new())
                            }
                        };
                        let reply = format!("HTTP/1.1 {status} X\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                        let _ = s.write_all(reply.as_bytes()).await;
                        let _ = s.shutdown().await;
                        return;
                    }
                });
            }
        });
        base
    }

    fn ep(base: &str) -> Endpoints {
        Endpoints { auth: String::new(), token: format!("{base}/token"), userinfo: String::new(), api: format!("{base}/api"), upload: format!("{base}/upload") }
    }

    fn file_of(len: usize) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("rec.mp4");
        std::fs::write(&p, vec![7u8; len]).unwrap();
        (dir, p)
    }

    #[tokio::test]
    async fn uploads_in_chunks_creates_the_folder_and_reports_progress() {
        let fake = Arc::new(Mutex::new(Fake::default()));
        let base = serve(fake.clone()).await;
        let (_d, p) = file_of((CHUNK * 2 + 5) as usize);
        let seen = Mutex::new(Vec::new());
        let done = upload_recording(&ep(&base), None, "", Some("tok".into()), &p, |x| seen.lock().unwrap().push(x)).await.unwrap();
        assert_eq!(done.link, "https://drive.google.com/file/d/X/view");
        assert!(done.confirms(CHUNK * 2 + 5));
        let f = fake.lock().unwrap();
        assert_eq!(f.stored, CHUNK * 2 + 5);
        assert_eq!(f.log.iter().filter(|l| l.starts_with("POST /api/files")).count(), 1, "folder created once: {:?}", f.log);
        assert_eq!(f.log.iter().filter(|l| l.starts_with("PUT /session")).count(), 3);
        let seen = seen.into_inner().unwrap();
        assert_eq!(seen.last(), Some(&100.0));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn drive_listing_becomes_history_entries() {
        let v = serde_json::json!({ "files": [
            { "id": "a", "name": "one.mp4", "size": "12", "webViewLink": "https://drive.google.com/file/d/a/view", "createdTime": "2026-11-05T01:00:00Z" },
            { "id": "b" }
        ] });
        let e = entries_from(&v);
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].name.as_str(), e[0].bytes), ("one.mp4", 12));
        assert!(e[0].path.is_none());
        assert!(entries_from(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn only_an_exact_size_confirms_the_upload() {
        let up = |size| Uploaded { link: String::new(), size };
        assert!(up(Some(10)).confirms(10));
        assert!(!up(Some(9)).confirms(10));
        assert!(!up(None).confirms(10));
    }

    #[tokio::test]
    async fn reuses_an_existing_folder() {
        let fake = Arc::new(Mutex::new(Fake { folder_exists: true, ..Fake::default() }));
        let base = serve(fake.clone()).await;
        let (_d, p) = file_of(10);
        upload_recording(&ep(&base), None, "", Some("tok".into()), &p, |_| {}).await.unwrap();
        assert!(!fake.lock().unwrap().log.iter().any(|l| l.starts_with("POST /api/files")));
    }

    #[tokio::test]
    async fn a_server_error_is_retried_from_where_drive_says() {
        let fake = Arc::new(Mutex::new(Fake { fail_next_put: true, ..Fake::default() }));
        let base = serve(fake.clone()).await;
        let (_d, p) = file_of(CHUNK as usize + 5);
        let done = upload_recording(&ep(&base), None, "", Some("tok".into()), &p, |_| {}).await.unwrap();
        assert!(done.link.contains("/file/d/X/"));
        assert_eq!(fake.lock().unwrap().stored, CHUNK + 5, "nothing sent twice, nothing lost");
    }

    #[tokio::test]
    async fn an_expired_token_is_refreshed_once() {
        let fake = Arc::new(Mutex::new(Fake { unauthorized_once: true, ..Fake::default() }));
        let base = serve(fake.clone()).await;
        let (_d, p) = file_of(10);
        let c = OAuthClient { id: "i".into(), secret: "s".into() };
        upload_recording(&ep(&base), Some(&c), "r", Some("old".into()), &p, |_| {}).await.unwrap();
        assert!(fake.lock().unwrap().log.iter().any(|l| l.starts_with("POST /token")));
    }

    #[tokio::test]
    async fn empty_and_missing_files_are_refused() {
        let fake = Arc::new(Mutex::new(Fake::default()));
        let base = serve(fake).await;
        let (_d, p) = file_of(0);
        assert!(upload_recording(&ep(&base), None, "", Some("t".into()), &p, |_| {}).await.is_err());
        assert!(upload_recording(&ep(&base), None, "", Some("t".into()), Path::new("/nope.mp4"), |_| {}).await.is_err());
    }

    /// Real Drive, by hand, into a throwaway folder (a token from another OAuth client must not
    /// create the real folder: `drive.file` would hide it from the app, which would make a second one).
    /// `FLOWGT_DRIVE_ACCESS_TOKEN=… FLOWGT_DRIVE_TEST_FILE=… FLOWGT_DRIVE_TEST_FOLDER=… cargo test real_drive -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "uploads to a real Google Drive"]
    async fn real_drive() {
        let token = std::env::var("FLOWGT_DRIVE_ACCESS_TOKEN").expect("token");
        let file = std::env::var("FLOWGT_DRIVE_TEST_FILE").expect("file");
        let folder = std::env::var("FLOWGT_DRIVE_TEST_FOLDER").expect("folder");
        let done = upload_to_folder(&Endpoints::google(), None, "", Some(token), &folder, Path::new(&file), |p| eprintln!("{p:.0}%")).await.unwrap();
        println!("LINK {} SIZE {:?}", done.link, done.size);
    }
}

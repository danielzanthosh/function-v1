use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CALLBACK_PORT: u16 = 1455;
const CALLBACK_PATH: &str = "/auth/callback";
const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatGptOAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone)]
pub struct OAuthAuthorization {
    pub url: String,
    verifier: String,
    state: String,
}

impl OAuthAuthorization {
    pub fn state(&self) -> &str { &self.state }
}

pub fn save_tokens(tokens: &ChatGptOAuthTokens) -> Result<(), String> {
    let dir = function_config::AppConfig::function_dir();
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join("chatgpt-oauth.json");
    let data = serde_json::to_vec_pretty(tokens).map_err(|error| error.to_string())?;
    std::fs::write(path, data).map_err(|error| error.to_string())
}

pub fn load_tokens() -> Option<ChatGptOAuthTokens> {
    let path = function_config::AppConfig::function_dir().join("chatgpt-oauth.json");
    let data = std::fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

pub fn install_access_token_in_codex(access_token: &str) -> Result<(), String> {
    let executable = crate::codex::resolve_codex_executable().ok_or("Codex CLI was not found")?;
    let mut child = Command::new(executable)
        .args(["login", "--with-access-token"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start Codex login: {error}"))?;
    child.stdin.take().ok_or("Could not open Codex login stdin")?
        .write_all(access_token.as_bytes()).map_err(|error| error.to_string())?;
    let output = child.wait_with_output().map_err(|error| error.to_string())?;
    if output.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&output.stderr).trim().to_string()) }
}

pub fn begin_authorization() -> OAuthAuthorization {
    let verifier: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(64)
        .map(char::from)
        .collect();
    let state: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let redirect_uri = format!("http://localhost:{CALLBACK_PORT}{CALLBACK_PATH}");
    let url = format!(
        "{AUTHORIZE_URL}?response_type=code&client_id={CLIENT_ID}&redirect_uri={redirect_uri}&scope=openid%20profile%20email%20offline_access&code_challenge={challenge}&code_challenge_method=S256&state={state}&id_token_add_organizations=true&codex_cli_simplified_flow=true&originator=function"
    );
    OAuthAuthorization { url, verifier, state }
}

pub fn wait_for_callback(expected_state: String) -> Result<String, String> {
    let listener = TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .map_err(|error| format!("Could not bind OAuth callback on port {CALLBACK_PORT}: {error}"))?;
    listener.set_nonblocking(true).map_err(|error| error.to_string())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    let (mut stream, _) = loop {
        match listener.accept() {
            Ok(connection) => break connection,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(format!("OAuth callback timed out: {error}")),
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(10))).map_err(|error| error.to_string())?;
    let mut request = [0_u8; 8192];
    let bytes = stream.read(&mut request).map_err(|error| error.to_string())?;
    let request_text = String::from_utf8_lossy(&request[..bytes]);
    let first_line = request_text.lines().next().unwrap_or_default();
    let target = first_line.split_whitespace().nth(1).ok_or("Invalid OAuth callback request")?;
    let query = target.split('?').nth(1).unwrap_or_default();
    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        match (parts.next(), parts.next()) {
            (Some("code"), Some(value)) => code = Some(value.to_string()),
            (Some("state"), Some(value)) => state = Some(value.to_string()),
            _ => {}
        }
    }
    let success = state.as_deref() == Some(expected_state.as_str()) && code.is_some();
    let body = if success { "Authentication complete. You may close this window." } else { "Authentication failed. Return to Function for details." };
    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
    let _ = stream.write_all(response.as_bytes());
    if !success { return Err("OAuth callback state validation failed".into()); }
    Ok(code.unwrap())
}

pub fn exchange_code(authorization: OAuthAuthorization, code: String) -> Result<ChatGptOAuthTokens, String> {
    let redirect_uri = format!("http://localhost:{CALLBACK_PORT}{CALLBACK_PATH}");
    let body = format!(
        "grant_type=authorization_code&client_id={CLIENT_ID}&code={}&code_verifier={}&redirect_uri={redirect_uri}",
        percent_encode(&code),
        percent_encode(&authorization.verifier)
    );
    request_token(&body, None)
}

pub fn refresh_token(tokens: &ChatGptOAuthTokens) -> Result<ChatGptOAuthTokens, String> {
    let body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={CLIENT_ID}",
        percent_encode(&tokens.refresh_token)
    );
    request_token(&body, Some(&tokens.refresh_token))
}

fn request_token(body: &str, existing_refresh: Option<&str>) -> Result<ChatGptOAuthTokens, String> {
    let mut child = Command::new("curl")
        .args(["-sS", "--fail-with-body", "-X", "POST", TOKEN_URL, "-H", "Content-Type: application/x-www-form-urlencoded", "--data-binary", "@-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start curl for OAuth token exchange: {error}"))?;
    child.stdin.take().unwrap().write_all(body.as_bytes()).map_err(|error| error.to_string())?;
    let output = child.wait_with_output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| format!("Invalid OAuth token response: {error}"))?;
    let access_token = response.get("access_token").and_then(|v| v.as_str()).ok_or("OAuth response did not contain access_token")?.to_string();
    let refresh_token = response.get("refresh_token").and_then(|v| v.as_str()).map(str::to_string).or_else(|| existing_refresh.map(str::to_string)).ok_or("OAuth response did not contain refresh_token")?;
    let expires_in = response.get("expires_in").and_then(|v| v.as_u64()).ok_or("OAuth response did not contain expires_in")?;
    let expires_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() + expires_in;
    Ok(ChatGptOAuthTokens { access_token, refresh_token, expires_at })
}

pub fn open_authorization_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = { let mut command = Command::new("cmd"); command.args(["/C", "start", ""]); command };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = Command::new("xdg-open");
    command.arg(url).status().map_err(|error| error.to_string()).and_then(|status| if status.success() { Ok(()) } else { Err(format!("Browser exited with {status}")) })
}

fn percent_encode(value: &str) -> String {
    value.bytes().flat_map(|byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            vec![byte as char]
        } else {
            format!("%{byte:02X}").chars().collect()
        }
    }).collect()
}


use anyhow::{Context, Result};
use reqwest;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Chrome writes its active debug port and WebSocket URL to this file
/// inside each profile directory.
const DEVTOOLS_ACTIVE_PORT: &str = "DevToolsActivePort";

/// Common Chrome/Chromium profile directories to search.
const CHROME_SEARCH_PATHS: &[&str] = &[
    "/tmp/.org.chromium.Chromium",
    "/tmp/.com.google.Chrome",
    "/tmp/chromium-profile",
    ".config/google-chrome",
    ".config/chromium",
    ".config/microsoft-edge",
    ".config/BraveSoftware/Brave-Browser",
    // Flatpak
    ".var/app/com.google.Chrome/config/google-chrome",
    ".var/app/org.chromium.Chromium/config/chromium",
    // Snap
    "snap/chromium/common/chromium",
    "snap/google-chrome/common/google-chrome",
];

#[derive(Debug, Deserialize)]
struct ChromeVersion {
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: Option<String>,
    #[serde(rename = "Browser")]
    browser: Option<String>,
}

/// Search for Chrome's DevToolsActivePort file in common locations and extract
/// the Unix domain socket path (or localhost port) for CDP connection.
///
/// Returns the path to the DevToolsActivePort file and the socket/port info.
pub fn find_chrome_socket() -> Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());

    for search_path in CHROME_SEARCH_PATHS {
        let expanded = if search_path.starts_with('/') {
            PathBuf::from(search_path)
        } else {
            PathBuf::from(&home).join(search_path)
        };

        if !expanded.exists() {
            debug!("Search path does not exist: {}", expanded.display());
            continue;
        }

        // Search recursively for DevToolsActivePort
        if let Some(port_file) = find_devtools_active_port(&expanded) {
            info!("Found DevToolsActivePort at {}", port_file.display());
            return Ok(port_file);
        }
    }

    anyhow::bail!(
        "Could not find Chrome DevToolsActivePort file. \
         Is Chrome running with --remote-debugging-port or --remote-debugging-pipe? \
         Searched: {:?}",
        CHROME_SEARCH_PATHS
    )
}

/// Read the DevToolsActivePort file and extract the debug port and WebSocket URL.
///
/// File format (2 lines):
///   line 1: port number (e.g. "9222")
///   line 2: WebSocket URL path (e.g. "/devtools/browser/abc123")
pub fn parse_devtools_active_port(path: &Path) -> Result<(u16, String)> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let mut lines = contents.lines();
    let port_str = lines
        .next()
        .context("DevToolsActivePort: missing port line")?
        .trim();
    let ws_path = lines
        .next()
        .context("DevToolsActivePort: missing WebSocket path line")?
        .trim();

    let port: u16 = port_str
        .parse()
        .with_context(|| format!("Invalid port number: '{}'", port_str))?;

    Ok((port, ws_path.to_string()))
}

/// Get the full WebSocket debugger URL from Chrome's HTTP debug endpoint.
/// This is the URL you pass to `CdpConnection::connect()`.
pub async fn get_debug_ws_url(port: u16) -> Result<String> {
    let url = format!("http://127.0.0.1:{}/json/version", port);
    info!("Fetching debug WebSocket URL from {}", url);

    let resp = reqwest::get(&url)
        .await
        .with_context(|| format!("Failed to fetch {}", url))?;

    let version: ChromeVersion = resp
        .json()
        .await
        .context("Failed to parse Chrome version JSON")?;

    if let Some(ws_url) = version.web_socket_debugger_url {
        info!("Chrome debug WebSocket URL: {}", ws_url);
        Ok(ws_url)
    } else {
        anyhow::bail!(
            "Chrome version response missing webSocketDebuggerUrl. Browser: {:?}",
            version.browser
        )
    }
}

/// Recursively search for DevToolsActivePort under `dir`, up to 3 levels deep.
fn find_devtools_active_port(dir: &Path) -> Option<PathBuf> {
    find_devtools_active_port_inner(dir, 0)
}

fn find_devtools_active_port_inner(dir: &Path, depth: u32) -> Option<PathBuf> {
    if depth > 3 {
        return None;
    }

    let port_file = dir.join(DEVTOOLS_ACTIVE_PORT);
    if port_file.exists() {
        return Some(port_file);
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_devtools_active_port_inner(&path, depth + 1) {
                    return Some(found);
                }
            }
        }
    }

    None
}

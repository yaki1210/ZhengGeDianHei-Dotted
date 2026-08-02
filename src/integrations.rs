//! Safe, reversible browser and Windows Terminal preference integration.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

#[derive(Debug)]
pub enum IntegrationError {
    Io(io::Error),
    Json(serde_json::Error),
    NotFound(&'static str),
    InvalidConfig(String),
}

impl std::fmt::Display for IntegrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::NotFound(name) => write!(f, "{name} configuration was not found"),
            Self::InvalidConfig(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for IntegrationError {}
impl From<io::Error> for IntegrationError { fn from(value: io::Error) -> Self { Self::Io(value) } }
impl From<serde_json::Error> for IntegrationError { fn from(value: serde_json::Error) -> Self { Self::Json(value) } }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Browser { Chrome, Edge }

impl Browser {
    fn executable(self) -> &'static str { match self { Self::Chrome => "chrome", Self::Edge => "msedge" } }
    pub fn display_name(self) -> &'static str { match self { Self::Chrome => "Chrome", Self::Edge => "Edge" } }
    fn profile_root(self) -> Option<PathBuf> {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        Some(local.join(match self { Self::Chrome => "Google\\Chrome\\User Data", Self::Edge => "Microsoft\\Edge\\User Data" }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserChangeReport { pub browser: Browser, pub changed_files: Vec<PathBuf> }

pub fn browser_is_running(browser: Browser) -> bool {
    #[cfg(windows)]
    {
        let filter = format!("IMAGENAME eq {}.exe", browser.executable());
        std::process::Command::new("tasklist").args(["/FI", filter.as_str()]).output().map(|o| String::from_utf8_lossy(&o.stdout).contains(browser.executable())).unwrap_or(false)
    }
    #[cfg(not(windows))]
    { let _ = browser; false }
}

pub fn apply_browser_fonts(browser: Browser, family: &str) -> Result<BrowserChangeReport, IntegrationError> {
    let root = browser.profile_root().ok_or(IntegrationError::NotFound(browser.display_name()))?;
    let profiles = discover_profiles(&root)?;
    if profiles.is_empty() { return Err(IntegrationError::NotFound(browser.display_name())); }
    if browser_is_running(browser) { return Err(IntegrationError::InvalidConfig(format!("{} is running; close it before applying settings", browser.display_name()))); }
    let mut changed_files = Vec::new();
    for preferences in profiles {
        let original = fs::read(&preferences)?;
        let mut root: Value = serde_json::from_slice(&original)?;
        let webprefs = root.pointer_mut("/webkit/webprefs/fonts").and_then(Value::as_object_mut)
            .ok_or_else(|| IntegrationError::InvalidConfig(format!("missing webkit.webprefs.fonts in {}", preferences.display())))?;
        for key in ["standard", "serif", "sansserif", "fixed"] { webprefs.insert(key.to_owned(), Value::String(family.to_owned())); }
        let backup = backup_path(&preferences, ".zgd16-backup");
        if !backup.exists() { fs::write(&backup, &original)?; }
        atomic_write_json(&preferences, &root)?;
        changed_files.push(preferences);
    }
    Ok(BrowserChangeReport { browser, changed_files })
}

pub fn restore_browser(browser: Browser) -> Result<BrowserChangeReport, IntegrationError> {
    let root = browser.profile_root().ok_or(IntegrationError::NotFound(browser.display_name()))?;
    let profiles = discover_profiles(&root)?;
    if browser_is_running(browser) { return Err(IntegrationError::InvalidConfig(format!("{} is running; close it before restoring settings", browser.display_name()))); }
    let mut restored = Vec::new();
    for preferences in profiles {
        let backup = backup_path(&preferences, ".zgd16-backup");
        if backup.is_file() { atomic_copy(&backup, &preferences)?; restored.push(preferences); }
    }
    if restored.is_empty() { return Err(IntegrationError::NotFound("browser backup")); }
    Ok(BrowserChangeReport { browser, changed_files: restored })
}

pub fn terminal_settings_path() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let store = local.join("Packages\\Microsoft.WindowsTerminal_8wekyb3d8bbwe\\LocalState\\settings.json");
    if store.is_file() { Some(store) } else {
        let legacy = local.join("Microsoft\\Windows Terminal\\settings.json");
        legacy.is_file().then_some(legacy)
    }
}

pub fn apply_terminal_font(family: &str, size: u32) -> Result<PathBuf, IntegrationError> {
    let path = terminal_settings_path().ok_or(IntegrationError::NotFound("Windows Terminal"))?;
    let original_text = fs::read_to_string(&path)?;
    let clean_text = strip_jsonc(&original_text);
    let mut root: Value = serde_json::from_str(&clean_text)?;
    let defaults = root.pointer_mut("/profiles/defaults").and_then(Value::as_object_mut)
        .ok_or_else(|| IntegrationError::InvalidConfig("missing profiles.defaults in Windows Terminal settings".to_owned()))?;
    let font = defaults.entry("font").or_insert_with(|| json!({}));
    let font = font.as_object_mut().ok_or_else(|| IntegrationError::InvalidConfig("profiles.defaults.font is not an object".to_owned()))?;
    font.insert("face".to_owned(), Value::String(family.to_owned()));
    font.insert("size".to_owned(), Value::Number(serde_json::Number::from(size)));
    let backup = backup_path(&path, ".zgd16-backup");
    if !backup.exists() { fs::write(&backup, &original_text)?; }
    atomic_write_json(&path, &root)?;
    Ok(path)
}

pub fn restore_terminal() -> Result<PathBuf, IntegrationError> {
    let path = terminal_settings_path().ok_or(IntegrationError::NotFound("Windows Terminal"))?;
    let backup = backup_path(&path, ".zgd16-backup");
    if !backup.is_file() { return Err(IntegrationError::NotFound("Windows Terminal backup")); }
    atomic_copy(&backup, &path)?;
    Ok(path)
}

fn discover_profiles(root: &Path) -> Result<Vec<PathBuf>, IntegrationError> {
    let mut profiles = Vec::new();
    for name in ["Default".to_owned()] {
        let path = root.join(name).join("Preferences");
        if path.is_file() { profiles.push(path); }
    }
    if root.is_dir() {
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if path.is_dir() && path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("Profile ")) {
                let preferences = path.join("Preferences");
                if preferences.is_file() { profiles.push(preferences); }
            }
        }
    }
    Ok(profiles)
}

fn backup_path(path: &Path, suffix: &str) -> PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("settings");
    path.with_file_name(format!("{name}{suffix}"))
}

fn atomic_copy(source: &Path, destination: &Path) -> Result<(), IntegrationError> {
    let temp = destination.with_extension("zgd16.tmp");
    fs::copy(source, &temp)?;
    #[cfg(windows)]
    if destination.exists() { fs::remove_file(destination)?; }
    fs::rename(temp, destination)?;
    Ok(())
}

/// Strip JSONC features (comments and trailing commas) from a JSON text.
/// Handles string literals properly to avoid stripping inside strings.
fn strip_jsonc(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // String literal: copy verbatim, handling escape sequences
        if chars[i] == '"' {
            result.push('"');
            i += 1;
            while i < len {
                result.push(chars[i]);
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                    result.push(chars[i]);
                } else if chars[i] == '"' {
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }

        // Single-line comment: // ... \n
        if chars[i] == '/' && i + 1 < len && chars[i + 1] == '/' {
            i += 2;
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        // Block comment: /* ... */
        if chars[i] == '/' && i + 1 < len && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2; // skip */
            continue;
        }

        // Trailing comma: skip if followed by whitespace then } or ]
        if chars[i] == ',' {
            let mut j = i + 1;
            while j < len && chars[j].is_whitespace() {
                j += 1;
            }
            if j < len && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

fn atomic_write_json(path: &Path, value: &Value) -> Result<(), IntegrationError> {
    let temp = path.with_extension("zgd16.tmp");
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(&temp, bytes)?;
    #[cfg(windows)]
    if path.exists() { fs::remove_file(path)?; }
    fs::rename(temp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_jsonc_removes_comments_and_trailing_commas() {
        let input = r#"{
            // line comment
            "a": 1, /* block comment */
            "b": "http://example.com",
            "c": 3,
        }"#;
        let output = strip_jsonc(input);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["a"], 1);
        assert_eq!(parsed["c"], 3);
    }
}

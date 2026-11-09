//! A small parser for the AWS shared config (`~/.aws/config`) and credentials
//! (`~/.aws/credentials`) files.
//!
//! The two files share one INI dialect: `[section]` headers, `key = value` lines, `#` / `;`
//! comment lines, and indented continuation lines under a key with an empty value (nested
//! settings such as `s3 =` blocks). The config file names profiles `[profile x]` (and `[default]`)
//! and also holds `[sso-session x]` sections; the credentials file names profiles `[x]`.

use std::collections::HashMap;
use std::path::PathBuf;

use super::Env;

pub(crate) type Props = HashMap<String, String>;

/// Every profile and `sso-session` from both files, with the credentials file winning per key.
#[derive(Debug, Default, Clone)]
pub(crate) struct ParsedFiles {
    pub(crate) profiles: HashMap<String, Props>,
    pub(crate) sso_sessions: HashMap<String, Props>,
}

/// One parsed file: section header (verbatim, trimmed) to its properties.
pub(crate) fn parse_sections(text: &str) -> Vec<(String, Props)> {
    let mut sections: Vec<(String, Props)> = Vec::new();
    // Set while the previous key had an empty value: the indented lines that follow it are nested
    // settings, not top-level properties.
    let mut skip_indented = false;

    for raw in text.lines() {
        // A UTF-8 BOM can precede the first header.
        let raw = raw.trim_start_matches('\u{feff}');
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        let indented = raw.starts_with(' ') || raw.starts_with('\t');
        if indented && skip_indented {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('[') {
            let name = match rest.find(']') {
                Some(end) => rest[..end].trim().to_string(),
                None => continue,
            };
            skip_indented = false;
            sections.push((name, Props::new()));
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let Some((_, props)) = sections.last_mut() else {
            // A property before any header is ignored.
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = strip_inline_comment(value.trim()).to_string();
        skip_indented = value.is_empty();
        if !key.is_empty() {
            props.insert(key, value);
        }
    }
    sections
}

/// Drops a trailing ` # ...` / ` ; ...` comment (the comment marker must follow whitespace).
fn strip_inline_comment(value: &str) -> &str {
    let bytes = value.as_bytes();
    for i in 1..bytes.len() {
        if (bytes[i] == b'#' || bytes[i] == b';') && bytes[i - 1].is_ascii_whitespace() {
            return value[..i].trim_end();
        }
    }
    value
}

impl ParsedFiles {
    pub(crate) fn parse(config: &str, credentials: &str) -> Self {
        let mut files = ParsedFiles::default();
        for (header, props) in parse_sections(config) {
            if let Some(name) = header.strip_prefix("sso-session") {
                if name.starts_with(char::is_whitespace) {
                    files
                        .sso_sessions
                        .entry(name.trim().to_string())
                        .or_default()
                        .extend(props);
                    continue;
                }
            }
            let name = if header == "default" {
                "default".to_string()
            } else if let Some(name) = header.strip_prefix("profile") {
                if !name.starts_with(char::is_whitespace) {
                    continue;
                }
                name.trim().to_string()
            } else {
                // Other config sections (`[services x]`, ...) are not credential sources.
                continue;
            };
            files.profiles.entry(name).or_default().extend(props);
        }
        for (header, props) in parse_sections(credentials) {
            files.profiles.entry(header).or_default().extend(props);
        }
        files
    }

    /// Reads `AWS_CONFIG_FILE` / `AWS_SHARED_CREDENTIALS_FILE`, or the files under `~/.aws`.
    /// A missing or unreadable file is an empty one.
    pub(crate) fn load(env: &Env) -> Self {
        let read = |path: Option<PathBuf>| -> String {
            path.and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default()
        };
        let home = env.home_dir();
        let config_path = env
            .get("AWS_CONFIG_FILE")
            .map(|p| expand_home(p, home.as_ref()))
            .or_else(|| home.as_ref().map(|h| h.join(".aws").join("config")));
        let credentials_path = env
            .get("AWS_SHARED_CREDENTIALS_FILE")
            .map(|p| expand_home(p, home.as_ref()))
            .or_else(|| home.as_ref().map(|h| h.join(".aws").join("credentials")));
        Self::parse(&read(config_path), &read(credentials_path))
    }
}

fn expand_home(path: &str, home: Option<&PathBuf>) -> PathBuf {
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

#[cfg(test)]
#[path = "ini_tests.rs"]
mod tests;

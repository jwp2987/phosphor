//! The "Report a problem" diagnostics bundle (Wave 1.5 #1).
//!
//! Packages the last few minutes of the app log, the app/OS/build facts and the settings with
//! secrets removed into ONE local zip. **This module never touches the network** and has no
//! "send" path: the user decides what to do with the file. `report_bundle_tests.rs` pins that
//! (no network type or crate is named here) and `script/check_outbound_redaction` repeats the check.
//!
//! What is scrubbed (everything is replaced by a `<marker>`, the line shape survives so the
//! timing lines `hop=... ms=...`, per-key / ssh timings stay readable):
//! the home directory, the user name and the host name of THIS machine (literal match first);
//! every absolute path (`/a/b`, `~/x`, `C:\x`, `\\server\x`); e-mail addresses and `user@host`;
//! IPv4 / IPv6 addresses and dotted host names; URLs; `Authorization` / `Bearer` values and any
//! `key=value` / `key: value` whose key names a secret (token, password, api_key, cookie, ...) or
//! typed / command content (cmd, command, text, input, prompt, query, argv, host, user, path, ...);
//! every double-quoted (and apostrophe / backtick quoted) string that is not a short enum-like word;
//! well-known key prefixes (`sk-`, `ghp_`, `AKIA`, `eyJ` JWTs, ...) and long mixed letter-digit tokens.
//! The app layer may add a second pass (the terminal's secret detector) through [`ReportInputs::extra_scrub`].

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Duration, Local, Utc};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// How far back the bundle looks (owner: ~15 minutes).
pub const REPORT_WINDOW_MINUTES: i64 = 15;
/// Hard cap on the scrubbed log text in the bundle.
const MAX_LOG_BYTES: usize = 20 * 1024 * 1024;
/// Only the newest log files are read (the window is short).
const MAX_LOG_FILES: usize = 4;

/// Facts about THIS machine that must never appear in the bundle, matched literally.
#[derive(Debug, Clone, Default)]
pub struct Identity {
    pub home: Option<String>,
    pub username: Option<String>,
    pub hostname: Option<String>,
}

impl Identity {
    /// Home from `$HOME` / `%USERPROFILE%`, user from `$USER` / `%USERNAME%` (else the home's
    /// last component); the host name is supplied by the caller.
    pub fn from_env(hostname: Option<String>) -> Self {
        let home = std::env::var("HOME")
            .ok()
            .or_else(|| std::env::var("USERPROFILE").ok())
            .or_else(|| dirs::home_dir().map(|p| p.display().to_string()))
            .filter(|h| h.len() > 1);
        let username = std::env::var("USER")
            .ok()
            .or_else(|| std::env::var("USERNAME").ok())
            .or_else(|| {
                home.as_deref()
                    .and_then(|h| h.rsplit(['/', '\\']).find(|s| !s.is_empty()))
                    .map(str::to_string)
            })
            .filter(|u| !u.is_empty());
        Self {
            home,
            username,
            hostname: hostname.filter(|h| !h.is_empty()),
        }
    }
}

/// Keys whose value is a credential: the value is removed (substring match, lower-cased, `-` = `_`).
const SECRET_KEY_PARTS: &[&str] = &[
    "password",
    "passwd",
    "passphrase",
    "secret",
    "token",
    "apikey",
    "api_key",
    "authorization",
    "auth",
    "credential",
    "cookie",
    "bearer",
    "private",
    "session_id",
    "regex",
];

/// Keys whose value is typed / command / identity / location content: the value is removed (exact).
const CONTENT_KEYS: &[&str] = &[
    "cmd",
    "command",
    "cmdline",
    "command_line",
    "commandline",
    "text",
    "input",
    "typed",
    "prompt",
    "query",
    "line",
    "argv",
    "args",
    "arg",
    "buffer",
    "content",
    "contents",
    "body",
    "output",
    "stdout",
    "stderr",
    "host",
    "hostname",
    "user",
    "username",
    "remote",
    "target",
    "dest",
    "destination",
    "addr",
    "address",
    "ip",
    "email",
    "path",
    "cwd",
    "dir",
    "file",
    "filename",
    "home",
    "url",
    "uri",
    "endpoint",
    "base_url",
    "pwd",
    "key",
    "title",
    "label",
    "session_name",
    "preedit",
    "composed",
    "chars",
    "string",
    "data",
    "payload",
];

/// Content keys whose unquoted value may contain spaces (a command line, typed text): the value
/// runs to the end of the line.
const FREE_TEXT_KEYS: &[&str] = &[
    "cmd",
    "command",
    "cmdline",
    "command_line",
    "commandline",
    "text",
    "input",
    "typed",
    "prompt",
    "query",
    "line",
    "argv",
    "args",
    "buffer",
    "content",
    "contents",
    "body",
    "output",
    "stdout",
    "stderr",
];

const NAME_PREFIXES: &[&str] = &[
    "sk-",
    "sk_",
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "AKIA",
    "ASIA",
    "AIza",
    "eyJ",
    "pk_live",
    "rk_live",
];

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn norm_key(key: &str) -> String {
    key.to_ascii_lowercase().replace('-', "_")
}

fn is_secret_key(key: &str) -> bool {
    let k = norm_key(key);
    SECRET_KEY_PARTS.iter().any(|p| k.contains(p))
}

fn is_content_key(key: &str) -> bool {
    let k = norm_key(key);
    CONTENT_KEYS.contains(&k.as_str())
}

/// A short, word-like string that is safe to keep inside quotes (an enum variant, a state name).
fn is_enum_like(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 16
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'.' | b'-'))
        && !looks_like_secret(s)
}

fn is_marker(s: &str) -> bool {
    s.len() >= 3 && s.starts_with('<') && s.ends_with('>')
}

fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && [8usize, 4, 4, 4, 12]
            .iter()
            .zip(&parts)
            .all(|(n, p)| p.len() == *n && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn looks_like_secret(s: &str) -> bool {
    if is_uuid(s) {
        return false;
    }
    if s.len() >= 10 && NAME_PREFIXES.iter().any(|p| s.starts_with(p)) {
        return true;
    }
    s.len() >= 20
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b'/' | b'=' | b'.')
        })
        && s.bytes().any(|b| b.is_ascii_alphabetic())
        && (s.bytes().filter(u8::is_ascii_digit).count() >= 3
            || (s.bytes().any(|b| b.is_ascii_digit())
                && s.bytes().any(|b| b.is_ascii_uppercase())
                && s.bytes().any(|b| b.is_ascii_lowercase())))
}

/// Replace every occurrence of `needle` (>= 2 bytes). With `word`, only where it is not part of a
/// longer alphanumeric word.
fn replace_literal(text: &str, needle: &str, with: &str, word: bool) -> String {
    if needle.len() < 2 || !text.contains(needle) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut prev: Option<char> = None;
    while let Some(pos) = rest.find(needle) {
        let before = rest[..pos].chars().next_back().or(prev);
        let after = rest[pos + needle.len()..].chars().next();
        let bounded = !word
            || (!before.is_some_and(|c| c.is_alphanumeric())
                && !after.is_some_and(|c| c.is_alphanumeric()));
        out.push_str(&rest[..pos]);
        if bounded {
            out.push_str(with);
        } else {
            out.push_str(needle);
        }
        prev = needle.chars().next_back();
        rest = &rest[pos + needle.len()..];
    }
    out.push_str(rest);
    out
}

/// Replace `needle` and the rest of the path that follows it (up to whitespace, a quote or a bracket).
fn replace_path_run(text: &str, needle: &str, with: &str) -> String {
    if needle.len() < 2 || !text.contains(needle) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(needle) {
        out.push_str(&rest[..pos]);
        out.push_str(with);
        let after = &rest[pos + needle.len()..];
        let run = after
            .find(|c: char| {
                c.is_whitespace()
                    || matches!(c, '"' | '\'' | '`' | ',' | ';' | ')' | ']' | '}' | '>')
            })
            .unwrap_or(after.len());
        rest = &after[run..];
    }
    out.push_str(rest);
    out
}

fn scrub_identity(line: &str, id: &Identity) -> String {
    let mut s = line.to_string();
    if let Some(home) = &id.home {
        let trimmed = home.trim_end_matches(['/', '\\']);
        // JSON / Debug output doubles the backslashes of a Windows path (longest spelling first).
        if trimmed.contains('\\') {
            s = replace_path_run(&s, &trimmed.replace('\\', "\\\\"), "<path>");
            s = replace_path_run(&s, &trimmed.replace('\\', "/"), "<path>");
        }
        s = replace_path_run(&s, trimmed, "<path>");
    }
    if let Some(host) = &id.hostname {
        s = replace_literal(&s, host, "<host>", true);
    }
    if let Some(user) = &id.username {
        s = replace_literal(&s, user, "<user>", true);
    }
    s
}

/// Index just past the end of a value starting at byte `start` of `s`.
fn value_end(s: &str, start: usize) -> usize {
    let rest = &s[start..];
    let mut chars = rest.char_indices();
    let Some((_, first)) = chars.next() else {
        return start;
    };
    match first {
        '"' | '\'' | '`' => {
            let mut escaped = false;
            for (i, c) in rest.char_indices().skip(1) {
                if escaped {
                    escaped = false;
                } else if c == '\\' && first == '"' {
                    escaped = true;
                } else if c == first {
                    return start + i + c.len_utf8();
                }
            }
            s.len()
        }
        '[' | '(' | '{' => {
            let close = match first {
                '[' => ']',
                '(' => ')',
                _ => '}',
            };
            let mut depth = 0usize;
            for (i, c) in rest.char_indices() {
                if c == first {
                    depth += 1;
                } else if c == close {
                    depth -= 1;
                    if depth == 0 {
                        return start + i + c.len_utf8();
                    }
                }
            }
            s.len()
        }
        _ => {
            let end = rest
                .find(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ')' | ']' | '}'))
                .unwrap_or(rest.len());
            start + end
        }
    }
}

/// `key=value` / `key: value` where the key names a secret or typed/identity content.
fn scrub_keyed(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < line.len() {
        let c = line[i..].chars().next().unwrap_or(' ');
        let at_ident_start = bytes[i].is_ascii_alphabetic() || bytes[i] == b'_';
        let prev_ident = i > 0 && is_ident_byte(bytes[i - 1]);
        if at_ident_start && !prev_ident {
            let mut j = i;
            while j < line.len() && is_ident_byte(bytes[j]) {
                j += 1;
            }
            let key = &line[i..j];
            let sep_len = if line[j..].starts_with('=') {
                1
            } else if line[j..].starts_with(": ") {
                2
            } else {
                0
            };
            if sep_len > 0 && (is_secret_key(key) || is_content_key(key)) {
                let vstart = j + sep_len;
                let mut vend = value_end(line, vstart);
                let bare = !line[vstart..].starts_with(['"', '\'', '`', '[', '(', '{']);
                if bare && FREE_TEXT_KEYS.contains(&norm_key(key).as_str()) {
                    vend = line.len();
                }
                let value = &line[vstart..vend];
                if vend > vstart && !is_marker(value) {
                    // `Authorization: Bearer abc` has the credential one token later.
                    if is_secret_key(key)
                        && matches!(
                            value.to_ascii_lowercase().as_str(),
                            "bearer" | "basic" | "token"
                        )
                        && line[vend..].starts_with(' ')
                    {
                        vend = value_end(line, vend + 1);
                    }
                    out.push_str(&line[i..vstart]);
                    out.push_str("<redacted>");
                    i = vend;
                    continue;
                }
            }
            out.push_str(&line[i..j]);
            i = j;
            continue;
        }
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// Quoted strings that are not short enum-like words.
fn scrub_quoted(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut prev: Option<char> = None;
    let mut i = 0;
    while i < line.len() {
        let c = line[i..].chars().next().unwrap_or(' ');
        let opens = match c {
            '"' => true,
            // an apostrophe / backtick opens a string only at a word boundary ("can't" does not)
            '\'' | '`' => prev.is_none_or(|p| {
                p.is_whitespace() || matches!(p, '(' | '[' | '{' | '=' | ':' | ',')
            }),
            _ => false,
        };
        if opens {
            let end = value_end(line, i);
            let closed = end > i + 1 && line[i + 1..end].ends_with(c) && end - i >= 2;
            let inner_end = if closed { end - 1 } else { end };
            let inner = &line[i + 1..inner_end.max(i + 1)];
            // an apostrophe "string" must close at a boundary to count
            let plausible = c == '"' || closed;
            if plausible {
                if inner.is_empty() || is_enum_like(inner) || is_marker(inner) {
                    out.push_str(&line[i..end]);
                } else {
                    out.push(c);
                    out.push_str("<redacted>");
                    out.push(c);
                }
                prev = Some(c);
                i = end;
                continue;
            }
        }
        out.push(c);
        prev = Some(c);
        i += c.len_utf8();
    }
    out
}

fn is_ipv4(core: &str) -> bool {
    let core = core.split(['/', ':']).next().unwrap_or(core);
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 3
                && p.bytes().all(|b| b.is_ascii_digit())
                && p.parse::<u16>().is_ok_and(|n| n <= 255)
        })
}

fn is_ipv6(core: &str) -> bool {
    let core = core.trim_matches(['[', ']']);
    let colons = core.matches(':').count();
    colons >= 2
        && (core.contains("::") || colons >= 5)
        && core.len() >= 6
        && core
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.' || b == b'%')
        && core.bytes().any(|b| b.is_ascii_hexdigit())
}

fn is_hostname(core: &str) -> bool {
    let labels: Vec<&str> = core.split('.').collect();
    if labels.len() < 3 {
        return false;
    }
    let last = labels[labels.len() - 1];
    labels
        .iter()
        .all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        && last.len() >= 2
        && last.bytes().all(|b| b.is_ascii_alphabetic())
        && labels
            .iter()
            .any(|l| l.bytes().any(|b| b.is_ascii_alphabetic()))
}

fn is_path(core: &str) -> bool {
    let b = core.as_bytes();
    if core.starts_with("~/") || core.starts_with("\\\\") || core.starts_with("$HOME") {
        return true;
    }
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
    {
        return true;
    }
    if core.starts_with('/') && core.len() > 1 {
        let inner = &core[1..];
        return inner.contains('/')
            || matches!(
                inner.split('/').next().unwrap_or(""),
                "home"
                    | "Users"
                    | "tmp"
                    | "var"
                    | "etc"
                    | "usr"
                    | "opt"
                    | "mnt"
                    | "root"
                    | "dev"
                    | "proc"
                    | "run"
                    | "private"
                    | "Volumes"
                    | "nix"
            );
    }
    false
}

fn scrub_core(core: &str) -> String {
    if core.is_empty() || is_marker(core) {
        return core.to_string();
    }
    // `a=/b/c` (a key that is not in the secret / content lists)
    if let Some((k, v)) = core.split_once('=')
        && !k.is_empty()
        && k.bytes().all(is_ident_byte)
    {
        return format!("{k}={}", scrub_core(v));
    }
    if core.contains("://") {
        return "<url>".to_string();
    }
    if core.contains('@') {
        let (l, r) = core.split_once('@').unwrap_or((core, ""));
        if !l.is_empty() && !r.is_empty() {
            return if r.contains('.') {
                "<email>"
            } else {
                "<user@host>"
            }
            .to_string();
        }
    }
    if is_ipv4(core) || is_ipv6(core) {
        return "<ip>".to_string();
    }
    if is_path(core) {
        return "<path>".to_string();
    }
    if is_hostname(core) {
        return "<host>".to_string();
    }
    if looks_like_secret(core) {
        return "<token>".to_string();
    }
    core.to_string()
}

fn scrub_tokens(line: &str) -> String {
    const EDGE: &[char] = &[
        '"', '\'', '`', '(', ')', '[', ']', '{', '}', ',', ';', '<', '>',
    ];
    let mut out = String::with_capacity(line.len());
    let mut prev_token = String::new();
    let mut rest = line;
    while !rest.is_empty() {
        let ws = rest
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(rest.len());
        out.push_str(&rest[..ws]);
        rest = &rest[ws..];
        if rest.is_empty() {
            break;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let token = &rest[..end];
        rest = &rest[end..];
        let lead_len = token.len() - token.trim_start_matches(EDGE).len();
        let core_full = token.trim_start_matches(EDGE);
        let core_len = core_full.trim_end_matches(EDGE).len();
        // a leading `<` of a marker like `<redacted>` is an edge char; keep markers whole
        let (lead, core, trail) = if is_marker(token) {
            ("", token, "")
        } else {
            (
                &token[..lead_len],
                &core_full[..core_len],
                &core_full[core_len..],
            )
        };
        let replaced = if matches!(prev_token.to_ascii_lowercase().as_str(), "bearer" | "basic")
            && !core.is_empty()
        {
            "<token>".to_string()
        } else {
            scrub_core(core)
        };
        out.push_str(lead);
        out.push_str(&replaced);
        out.push_str(trail);
        prev_token = core.to_string();
    }
    out
}

/// Scrub one log line (see the module docs for what goes).
pub fn scrub_line(line: &str, id: &Identity) -> String {
    let s = scrub_identity(line, id);
    let s = scrub_keyed(&s);
    let s = scrub_quoted(&s);
    scrub_tokens(&s)
}

/// Scrub a settings.toml: secrets and content keys lose their values; only bool / number / short
/// enum-like string values survive; comments and free-form array lines go; deep table names are cut.
pub fn scrub_settings_toml(text: &str, id: &Identity) -> String {
    let mut out = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        let kept = if line.is_empty() {
            String::new()
        } else if line.starts_with('#') {
            "# <comment removed>".to_string()
        } else if line.starts_with('[') {
            let name = line.trim_matches(['[', ']']);
            let segs: Vec<&str> = name.split('.').collect();
            let shown = if segs.len() > 2 {
                format!("{}.{}.<redacted>", segs[0], segs[1])
            } else {
                name.to_string()
            };
            let dbl = if line.starts_with("[[") {
                ("[[", "]]")
            } else {
                ("[", "]")
            };
            format!("{}{}{}", dbl.0, scrub_line(&shown, id), dbl.1)
        } else if let Some((k, v)) = line.split_once('=') {
            let key = k.trim();
            let value = v.trim();
            let keep_value = !is_secret_key(key)
                && !is_content_key(key)
                && (matches!(value, "true" | "false")
                    || value.parse::<f64>().is_ok()
                    || value
                        .strip_prefix('"')
                        .and_then(|x| x.strip_suffix('"'))
                        .is_some_and(is_enum_like));
            if keep_value {
                format!("{key} = {}", scrub_line(value, id))
            } else {
                format!("{key} = \"<redacted>\"")
            }
        } else {
            "<redacted>".to_string()
        };
        out.push_str(&kept);
        out.push('\n');
    }
    out
}

/// Timestamp at the start of an `env_logger` file line (`2026-11-08T12:34:56.789012Z [INFO] ...`).
pub fn line_timestamp(line: &str) -> Option<DateTime<Utc>> {
    let first = line.split_whitespace().next()?;
    DateTime::parse_from_rfc3339(first)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Keep only lines within `[now - window, now + 1 day]`. A line without a timestamp (a wrapped
/// message) follows the decision of the line before it; lines before the first timestamp are dropped.
pub fn filter_window<I: IntoIterator<Item = String>>(
    lines: I,
    now: DateTime<Utc>,
    window: Duration,
) -> (Vec<String>, usize) {
    let cutoff = now - window;
    let limit = now + Duration::days(1);
    let mut keep = false;
    let mut kept = Vec::new();
    let mut dropped = 0usize;
    for line in lines {
        if let Some(ts) = line_timestamp(&line) {
            keep = ts >= cutoff && ts <= limit;
        }
        if keep {
            kept.push(line);
        } else {
            dropped += 1;
        }
    }
    (kept, dropped)
}

/// Everything the builder needs; the app layer fills it in.
pub struct ReportInputs {
    /// Log files, any order of age (the newest [`MAX_LOG_FILES`] by modification time are read).
    pub log_files: Vec<PathBuf>,
    /// Version / OS / build / feature-flag text (scrubbed again by the builder).
    pub manifest: String,
    /// Raw settings.toml text, if the file exists.
    pub settings_toml: Option<String>,
    pub identity: Identity,
    pub now: DateTime<Utc>,
    pub window: Duration,
    /// Optional extra pass over every kept line (the terminal's secret detector).
    pub extra_scrub: Option<fn(&mut String)>,
}

/// What the builder wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportSummary {
    pub log_lines: usize,
    pub dropped_out_of_window: usize,
}

const README: &str = "Phosphor problem report (local file)\n\n\
This file was created on your machine and was NOT sent anywhere. Phosphor never uploads it and\n\
has no send button: you decide whether to attach it to an issue or e-mail.\n\n\
Contents: manifest.txt (version, OS, build, enabled feature flags), app-log-window.txt (the last\n\
minutes of the app log), settings-scrubbed.toml (settings with secrets and free text removed).\n\
Scrubbed from every file: home directory, user name, host name, paths, e-mail addresses, IP\n\
addresses, URLs, tokens / API keys / Authorization values, typed text and command lines, and\n\
quoted prompt text. Please skim it before sharing anyway.\n";

fn read_scrubbed_log(inputs: &ReportInputs) -> (String, ReportSummary) {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = inputs
        .log_files
        .iter()
        .filter_map(|p| Some((std::fs::metadata(p).ok()?.modified().ok()?, p.clone())))
        .collect();
    files.sort_by_key(|(m, _)| std::cmp::Reverse(*m));
    files.truncate(MAX_LOG_FILES);
    files.reverse(); // oldest first, so the text reads chronologically

    let mut text = String::new();
    let mut summary = ReportSummary {
        log_lines: 0,
        dropped_out_of_window: 0,
    };
    'files: for (_, path) in files {
        let Ok(file) = File::open(&path) else {
            continue;
        };
        let mut lines = Vec::new();
        for raw in BufReader::new(file).split(b'\n') {
            let Ok(raw) = raw else { break };
            lines.push(
                String::from_utf8_lossy(&raw)
                    .trim_end_matches('\r')
                    .to_string(),
            );
        }
        let (kept, dropped) = filter_window(lines, inputs.now, inputs.window);
        summary.dropped_out_of_window += dropped;
        for line in kept {
            let mut scrubbed = scrub_line(&line, &inputs.identity);
            if let Some(extra) = inputs.extra_scrub {
                extra(&mut scrubbed);
            }
            if text.len() + scrubbed.len() + 1 > MAX_LOG_BYTES {
                text.push_str("# log truncated: size cap reached\n");
                break 'files;
            }
            text.push_str(&scrubbed);
            text.push('\n');
            summary.log_lines += 1;
        }
    }
    (text, summary)
}

/// Build the zip at `out`. Local file only; see the module docs.
pub fn build_report_bundle(out: &Path, inputs: &ReportInputs) -> Result<ReportSummary> {
    let (log_text, summary) = read_scrubbed_log(inputs);
    let manifest: String = inputs
        .manifest
        .lines()
        .map(|l| {
            let mut s = scrub_line(l, &inputs.identity);
            if let Some(extra) = inputs.extra_scrub {
                extra(&mut s);
            }
            s + "\n"
        })
        .collect();
    let settings = match &inputs.settings_toml {
        Some(t) => scrub_settings_toml(t, &inputs.identity),
        None => "# no settings file\n".to_string(),
    };

    let mut zip = ZipWriter::new(File::create(out)?);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let manifest = format!(
        "{manifest}Log window: last {} minutes\nLog lines: {}\nLines outside the window (left out): {}\n",
        inputs.window.num_minutes(),
        summary.log_lines,
        summary.dropped_out_of_window
    );
    for (name, body) in [
        ("README.txt", README),
        ("manifest.txt", manifest.as_str()),
        ("app-log-window.txt", log_text.as_str()),
        ("settings-scrubbed.toml", settings.as_str()),
    ] {
        zip.start_file(name, opts)?;
        zip.write_all(body.as_bytes())?;
    }
    zip.finish()?;
    Ok(summary)
}

/// `phosphor-problem-report-YYYYmmdd-HHMMSS.zip`
pub fn default_report_filename() -> String {
    format!(
        "phosphor-problem-report-{}.zip",
        Local::now().format("%Y%m%d-%H%M%S")
    )
}

#[cfg(test)]
#[path = "report_bundle_tests.rs"]
mod tests;

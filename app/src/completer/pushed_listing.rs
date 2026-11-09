//! Remote cwd listings pushed by the shell's own precmd hook (#1801).
//!
//! A Phosphorized ssh session without the remote helper cannot list a directory per keystroke
//! (that is a command on the shared ssh connection, #1295), so path detection read a 2 s cache
//! that only a Tab filled. The remote bootstrap (`bash_body.sh`, `zsh_body.sh`, `fish.sh`)
//! instead appends a bounded listing of its cwd to the Precmd payload when the cwd or the
//! listing changed (`cwd_listing`). This module parses it and keeps the latest one per session.
//!
//! Wire format of `cwd_listing`, all ASCII:
//!
//! * `T`: the directory has more entries than the shell lists at all ("too large"): no entries.
//! * `L<entries>`: the complete listing.
//! * `P<entries>`: a truncated listing (the entry or byte cap was reached); what is present is
//!   true, what is absent proves nothing.
//!
//! `<entries>` is zero or more tokens, each preceded by one space: a type letter (`d` directory,
//! `f` anything else) then the name, with every byte outside `[A-Za-z0-9._+@,=~-]` written as
//! `%HH`. A name therefore cannot contain a space, a quote, a backslash or a control byte, and
//! the JSON/OSC framing around the payload cannot be broken by a filename.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use lazy_static::lazy_static;
use typed_path::TypedPathBuf;
use warp_completer::completer::{EngineDirEntry, EngineFileType};

use crate::terminal::model::session::SessionId;

/// Sessions whose latest listing is kept. A listing is small (the shell caps it at ~32 KB) and a
/// slot is replaced by the session's next one; this bounds the leak of sessions that never
/// close cleanly.
const MAX_SESSIONS: usize = 64;

/// What the shell said about its cwd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PushedListing {
    /// Every entry of the directory.
    Complete(Vec<EngineDirEntry>),
    /// The first entries only.
    Partial(Vec<EngineDirEntry>),
    /// Too many entries to list.
    TooLarge,
}

struct Slot {
    directory: String,
    listing: PushedListing,
    entries: Arc<Vec<EngineDirEntry>>,
    sequence: u64,
    /// When `hop=path_detect` was last written for this slot (at most one per
    /// [`HOP_LOG_INTERVAL`]: lookups run per keystroke).
    last_hop: Option<Instant>,
}

/// Minimum spacing of the `hop=path_detect` line.
const HOP_LOG_INTERVAL: Duration = Duration::from_millis(250);

lazy_static! {
    static ref PUSHED: parking_lot::Mutex<(u64, HashMap<SessionId, Slot>)> =
        parking_lot::Mutex::new((0, HashMap::new()));
}

fn percent_decode(token: &str) -> Option<Vec<u8>> {
    let bytes = token.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = token.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(out)
}

/// Parses a `cwd_listing` value. `None` for anything malformed: a malformed listing is dropped
/// whole, never half-trusted.
pub(super) fn parse_pushed_listing(raw: &str) -> Option<PushedListing> {
    let mut chars = raw.chars();
    let kind = chars.next()?;
    let rest = chars.as_str();
    if kind == 'T' {
        return rest.is_empty().then_some(PushedListing::TooLarge);
    }
    if kind != 'L' && kind != 'P' {
        return None;
    }
    let mut entries = Vec::new();
    if !rest.is_empty() {
        for token in rest.strip_prefix(' ')?.split(' ') {
            let mut parts = token.chars();
            let file_type = match parts.next()? {
                'd' => EngineFileType::Directory,
                'f' => EngineFileType::File,
                _ => return None,
            };
            let name_bytes = percent_decode(parts.as_str())?;
            if name_bytes.is_empty() || name_bytes.contains(&b'/') {
                return None;
            }
            // A name that is not UTF-8 cannot be typed or completed; that entry alone is left
            // out (the rest of the listing is still true).
            let Ok(name) = String::from_utf8(name_bytes) else {
                continue;
            };
            entries.push(EngineDirEntry {
                file_name: name,
                file_type,
            });
        }
    }
    Some(if kind == 'L' {
        PushedListing::Complete(entries)
    } else {
        PushedListing::Partial(entries)
    })
}

fn normalize(directory: &str) -> &str {
    if directory.len() > 1 {
        directory.trim_end_matches('/')
    } else {
        directory
    }
}

/// Stores the listing a session's precmd carried for `pwd`. A malformed listing, or one without
/// a pwd to attach it to, is ignored (and the previous one for another directory stays keyed to
/// its own directory, so it can never answer for this one).
pub(super) fn store(session_id: SessionId, pwd: Option<&str>, raw: &str) {
    let Some(pwd) = pwd else { return };
    let Some(listing) = parse_pushed_listing(raw) else {
        log::warn!(
            "ignored a malformed cwd listing from the shell ({} bytes)",
            raw.len()
        );
        return;
    };
    // `hop=cwd_listing`: counts and the session id only, never a name or a path (the appci
    // lab case reads it).
    let (kind, count) = match &listing {
        PushedListing::Complete(entries) => ('L', entries.len()),
        PushedListing::Partial(entries) => ('P', entries.len()),
        PushedListing::TooLarge => ('T', 0),
    };
    log::info!(
        "hop=cwd_listing session={} kind={kind} entries={count}",
        session_id.as_u64()
    );
    let entries = match &listing {
        PushedListing::Complete(entries) | PushedListing::Partial(entries) => entries.clone(),
        PushedListing::TooLarge => Vec::new(),
    };
    let mut guard = PUSHED.lock();
    guard.0 += 1;
    let sequence = guard.0;
    if guard.1.len() >= MAX_SESSIONS
        && !guard.1.contains_key(&session_id)
        && let Some(oldest) = guard
            .1
            .iter()
            .min_by_key(|(_, slot)| slot.sequence)
            .map(|(id, _)| *id)
    {
        guard.1.remove(&oldest);
    }
    guard.1.insert(
        session_id,
        Slot {
            directory: normalize(pwd).to_owned(),
            listing,
            entries: Arc::new(entries),
            sequence,
            last_hop: None,
        },
    );
}

/// The pushed entries for `directory`, if the session's latest listing is of that directory.
/// `complete` is false for a truncated listing: only a consumer for which a missing name means
/// "no highlight" (not "no such file") may use one.
pub(super) fn lookup(
    session_id: SessionId,
    directory: &TypedPathBuf,
) -> Option<(Arc<Vec<EngineDirEntry>>, bool)> {
    let mut guard = PUSHED.lock();
    let slot = guard.1.get_mut(&session_id)?;
    let wanted = directory.to_path().to_string_lossy().into_owned();
    if normalize(&wanted) != slot.directory {
        return None;
    }
    let complete = match &slot.listing {
        PushedListing::Complete(_) => true,
        PushedListing::Partial(_) => false,
        PushedListing::TooLarge => return None,
    };
    let now = Instant::now();
    if slot
        .last_hop
        .is_none_or(|at| now.saturating_duration_since(at) >= HOP_LOG_INTERVAL)
    {
        slot.last_hop = Some(now);
        log::info!(
            "hop=path_detect source=pushed session={} entries={} complete={complete}",
            session_id.as_u64(),
            slot.entries.len()
        );
    }
    Some((slot.entries.clone(), complete))
}

#[cfg(test)]
#[path = "pushed_listing_tests.rs"]
mod tests;

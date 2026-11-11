//! The agent-context snapshot (parity family h): the host's skills and global rule files, as the
//! `RemoteAgentContextSnapshot` the app's `remote_agent_context.rs` reconciles.
//!
//! The helper (`daemon/agent_context.rs`) reads its own filesystem and pushes the snapshot; the
//! shell backend (`backend/shell/agent_context.rs`) runs [`skills_command`] / [`rules_command`]
//! over the out-of-band channel. Both feed [`assemble`], so the filtering (UTF-8 only, size and
//! count caps, sorted by path) is one function, and the directory list is one constant.
//!
//! Bundled skills are never part of it (owner ruling R3): the app supplies its own, the host
//! reports only what is on the host.
//!
//! What is scanned is exactly what the app's `SkillManager` scans for home skills: for each
//! provider in `SKILL_PROVIDER_DEFINITIONS`, `~/<provider dir>/skills/<name>/SKILL.md` (one
//! level, like `read_skills`), and the one global rule file `~/.agents/AGENTS.md`. The app pins
//! [`HOME_SKILL_DIRS`] against `SKILL_PROVIDER_DEFINITIONS` in a test.
//!
//! The old full daemon published home skills only, never project skills, so there is nothing to
//! refresh when the cwd moves into another project.

use std::collections::{BTreeMap, BTreeSet};

use super::frame::ParsedSection;
use super::shell_kind::ShellKind;
use super::tree::{TreeChild, join_path};
use crate::proto::{
    HomeSkillMetadata, RemoteAgentContextSnapshot, RemoteContextFileProto, RemoteSkillProto,
    StandingQueryContent, StandingQueryResultsDelta, remote_skill_proto,
};

/// Home-relative skill roots, in the app's provider precedence order.
pub const HOME_SKILL_DIRS: &[&str] = &[
    ".agents/skills",
    ".warp/skills",
    ".claude/skills",
    ".codex/skills",
    ".cursor/skills",
    ".gemini/skills",
    ".copilot/skills",
    ".factory/skills",
    ".github/skills",
    ".opencode/skills",
];

/// Home-relative global rule files.
pub const HOME_RULE_FILES: &[&str] = &[".agents/AGENTS.md"];

/// A skill or rule file bigger than this is left out.
pub const MAX_FILE_BYTES: usize = 256 * 1024;
/// The most bytes of file content one snapshot carries.
pub const MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024;
/// The most skills one snapshot carries.
pub const MAX_SKILLS: usize = 400;

/// One file as read from the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// The first path component under the home directory of every watched location (`.agents`, ...).
pub fn top_level_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = HOME_SKILL_DIRS
        .iter()
        .chain(HOME_RULE_FILES)
        .filter_map(|p| p.split('/').next())
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// Whether a change at `path` (absolute) under `home` can change the snapshot.
pub fn is_relevant_change(home: &str, path: &str) -> bool {
    let Some(rest) = path.strip_prefix(home).and_then(|r| r.strip_prefix('/')) else {
        return false;
    };
    let mut parts = rest.split('/');
    let Some(top) = parts.next() else {
        return false;
    };
    if !top_level_names().contains(&top) {
        return false;
    }
    match parts.next() {
        // The provider directory itself appeared or vanished.
        None => true,
        Some("skills") => true,
        Some(name) => HOME_RULE_FILES
            .iter()
            .any(|r| r.split_once('/') == Some((top, name))),
    }
}

/// Whether a finished command can have changed the skills or rules on the host, judged from its
/// text: it names a skill root's provider directory (`.claude`, `.agents`, ...), `SKILL.md` or
/// `AGENTS.md`. The shell arm has no watcher, so this decides which prompts re-read the context.
pub fn command_may_change_agent_context(command: &str) -> bool {
    command.contains("SKILL.md")
        || command.contains("AGENTS.md")
        || top_level_names().iter().any(|name| command.contains(name))
}

/// Builds the snapshot (at `revision`) from the files read. Non-UTF-8 files are dropped, as the
/// app's `read_to_string` drops them; the rest is sorted by path and capped.
pub fn assemble(
    home_dir: &str,
    skill_files: Vec<RawFile>,
    rule_files: Vec<RawFile>,
    revision: u64,
) -> RemoteAgentContextSnapshot {
    let mut budget = MAX_TOTAL_BYTES;
    let mut take = |file: RawFile| -> Option<(String, String)> {
        if file.bytes.len() > MAX_FILE_BYTES || file.bytes.len() > budget {
            return None;
        }
        let content = String::from_utf8(file.bytes).ok()?;
        budget -= content.len();
        Some((file.path, content))
    };
    let mut skills: Vec<RemoteSkillProto> = skill_files
        .into_iter()
        .filter_map(&mut take)
        .map(|(path, content)| RemoteSkillProto {
            path,
            content,
            source: Some(remote_skill_proto::Source::Home(HomeSkillMetadata {})),
        })
        .collect();
    skills.sort_by(|a, b| a.path.cmp(&b.path));
    skills.dedup_by(|a, b| a.path == b.path);
    skills.truncate(MAX_SKILLS);
    let mut global_rules: Vec<RemoteContextFileProto> = rule_files
        .into_iter()
        .filter_map(&mut take)
        .map(|(path, content)| RemoteContextFileProto { path, content })
        .collect();
    global_rules.sort_by(|a, b| a.path.cmp(&b.path));
    global_rules.dedup_by(|a, b| a.path == b.path);
    RemoteAgentContextSnapshot {
        revision,
        home_dir: home_dir.to_owned(),
        skills,
        global_rules,
    }
}

/// Whether two snapshots say the same thing (everything but the revision).
pub fn same_context(a: &RemoteAgentContextSnapshot, b: &RemoteAgentContextSnapshot) -> bool {
    a.home_dir == b.home_dir && a.skills == b.skills && a.global_rules == b.global_rules
}

/// Section `home`: the host's home directory. Every shell kind has one now (`Option` is kept
/// for a kind added later that cannot).
pub fn home_command(kind: ShellKind) -> Option<String> {
    Some(if kind.is_powershell() {
        "[Console]::Out.Write($HOME)".to_owned()
    } else {
        "printf '%s' \"$HOME\"".to_owned()
    })
}

/// The PowerShell `Emit` function and the final write. Records are the same
/// `<path> NUL <size> NUL <bytes>` as the POSIX ones, but built as ONE string (UTF-8 text only:
/// a file that is not valid UTF-8 is dropped here, as [`assemble`] drops it on the POSIX path)
/// and written once, so nothing interleaves with the framing markers.
fn pwsh_emit_prelude() -> String {
    format!(
        "[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)\n\
         $enc = New-Object System.Text.UTF8Encoding($false, $true)\n\
         $sb = New-Object System.Text.StringBuilder\n\
         $script:total = 0\n\
         function Emit($p) {{\n\
         if (-not (Test-Path -LiteralPath $p -PathType Leaf)) {{ return }}\n\
         $n = (Get-Item -LiteralPath $p).Length\n\
         if ($n -gt {MAX_FILE_BYTES}) {{ return }}\n\
         if ($script:total + $n -gt {MAX_TOTAL_BYTES}) {{ return }}\n\
         try {{ $b = [System.IO.File]::ReadAllBytes($p); $t = $enc.GetString($b) }} catch {{ return }}\n\
         $script:total += $b.Length\n\
         [void]$sb.Append($p).Append([char]0).Append($b.Length).Append([char]0).Append($t)\n\
         }}\n\
         if (-not $HOME) {{ exit 0 }}\n"
    )
}

/// The shared `emit` function: `<path> NUL <size> NUL <size bytes>` for a regular file within the
/// caps, nothing otherwise.
fn emit_function() -> String {
    format!(
        "total=0\n\
         emit() {{\n\
         [ -f \"$1\" ] || return 0\n\
         n=$(wc -c < \"$1\" 2>/dev/null | tr -d ' \\t')\n\
         case \"$n\" in ''|*[!0-9]*) return 0;; esac\n\
         [ \"$n\" -le {MAX_FILE_BYTES} ] || return 0\n\
         total=$((total + n))\n\
         [ \"$total\" -le {MAX_TOTAL_BYTES} ] || return 0\n\
         printf '%s\\0%s\\0' \"$1\" \"$n\"\n\
         head -c \"$n\" \"$1\" 2>/dev/null\n\
         }}\n\
         [ -n \"$HOME\" ] || exit 0\n"
    )
}

/// Section `skills`: every `~/<root>/<name>/SKILL.md` as records (see [`parse_records`]).
pub fn skills_command(kind: ShellKind) -> Option<String> {
    if kind.is_powershell() {
        let roots: Vec<String> = HOME_SKILL_DIRS.iter().map(|d| format!("'{d}'")).collect();
        return Some(format!(
            "{}foreach ($d in @({})) {{ foreach ($f in (Get-ChildItem -Path (Join-Path $HOME $d) -Directory -ErrorAction SilentlyContinue)) {{ Emit (Join-Path $f.FullName 'SKILL.md') }} }}\n\
             if ($sb.Length -gt 0) {{ Write-Output $sb.ToString() }}\nexit 0",
            pwsh_emit_prelude(),
            roots.join(", ")
        ));
    }
    let roots = HOME_SKILL_DIRS.join(" ");
    Some(format!(
        // No glob on purpose: zsh aborts the script on an unmatched glob ("no matches found",
        // #2152). `ls` of an existing root feeds a here-document, so `emit` runs in this shell
        // (its running byte total survives) and a missing or empty root is simply no entries.
        "{}for d in {roots}; do\n\
         [ -d \"$HOME/$d\" ] || continue\n\
         while IFS= read -r n; do emit \"$HOME/$d/$n/SKILL.md\"; done <<PHOSPHOR_SKILLS_EOF\n\
         $(ls -1A \"$HOME/$d\" 2>/dev/null)\n\
         PHOSPHOR_SKILLS_EOF\n\
         done\nexit 0",
        emit_function()
    ))
}

/// Section `rules`: the global rule files as records.
pub fn rules_command(kind: ShellKind) -> Option<String> {
    if kind.is_powershell() {
        let files: Vec<String> = HOME_RULE_FILES.iter().map(|f| format!("'{f}'")).collect();
        return Some(format!(
            "{}foreach ($r in @({})) {{ Emit (Join-Path $HOME $r) }}\n\
             if ($sb.Length -gt 0) {{ Write-Output $sb.ToString() }}\nexit 0",
            pwsh_emit_prelude(),
            files.join(", ")
        ));
    }
    let files = HOME_RULE_FILES.join(" ");
    Some(format!(
        "{}for r in {files}; do emit \"$HOME/$r\"; done\nexit 0",
        emit_function()
    ))
}

/// Reads the records a `skills` or `rules` section printed. A record cut short (the file shrank
/// while it was read) ends the list: that snapshot is simply refreshed on the next event.
pub fn parse_records(section: &ParsedSection) -> Vec<RawFile> {
    let data = &section.output;
    let mut files = Vec::new();
    let mut at = 0usize;
    while at < data.len() {
        let Some(path_end) = data[at..].iter().position(|b| *b == 0).map(|i| at + i) else {
            break;
        };
        let Some(len_end) = data[path_end + 1..]
            .iter()
            .position(|b| *b == 0)
            .map(|i| path_end + 1 + i)
        else {
            break;
        };
        let (Ok(path), Ok(len)) = (
            std::str::from_utf8(&data[at..path_end]),
            std::str::from_utf8(&data[path_end + 1..len_end])
                .unwrap_or("")
                .parse::<usize>(),
        ) else {
            break;
        };
        let start = len_end + 1;
        let Some(end) = start.checked_add(len).filter(|e| *e <= data.len()) else {
            break;
        };
        files.push(RawFile {
            path: path.to_owned(),
            bytes: data[start..end].to_vec(),
        });
        at = end;
    }
    files
}

/// Project skills: the `SKILL.md` files of a repository's loaded tree that sit at
/// `<anything>/<provider dir>/<name>/SKILL.md` (the same provider directories as the home
/// skills). Ignored paths are not part of the tree, so they are not found. These are the
/// standing-query results the old server published with the file tree; the app reads their
/// content through the file-context family.
pub fn project_skill_files(dirs: &BTreeMap<String, Vec<TreeChild>>) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for (dir, children) in dirs {
        let has_skill = children
            .iter()
            .any(|c| !c.is_dir && !c.ignored && c.name == "SKILL.md");
        if !has_skill {
            continue;
        }
        // `dir` is `<provider dir>/<name>`: its parent must end with a provider skills path.
        let Some((parent, _name)) = dir.trim_end_matches('/').rsplit_once('/') else {
            continue;
        };
        if HOME_SKILL_DIRS
            .iter()
            .any(|root| parent == *root || parent.ends_with(&format!("/{root}")))
        {
            found.insert(join_path(dir, "SKILL.md"));
        }
    }
    found
}

fn content(path: &String) -> StandingQueryContent {
    StandingQueryContent {
        path: path.clone(),
        is_directory: false,
    }
}

/// The standing results for a first snapshot: every project skill file as an upsert. `None`
/// when there are none (the message then carries no standing results, like a repo without any).
pub fn standing_snapshot(files: &BTreeSet<String>) -> Option<StandingQueryResultsDelta> {
    (!files.is_empty()).then(|| StandingQueryResultsDelta {
        upserted_project_skills: files.iter().map(content).collect(),
        ..Default::default()
    })
}

/// What changed between two project skill sets. `None` when nothing did. A skill file edited in
/// place does not change the tree, so it is not reported (the app rereads on the next snapshot).
pub fn standing_delta(
    old: &BTreeSet<String>,
    new: &BTreeSet<String>,
) -> Option<StandingQueryResultsDelta> {
    let upserted: Vec<_> = new.difference(old).map(content).collect();
    let removed: Vec<_> = old.difference(new).map(content).collect();
    (!upserted.is_empty() || !removed.is_empty()).then_some(StandingQueryResultsDelta {
        upserted_project_skills: upserted,
        removed_project_skills: removed,
        ..Default::default()
    })
}

#[cfg(test)]
#[path = "agent_context_tests.rs"]
mod tests;

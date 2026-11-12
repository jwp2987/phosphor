//! The repo file tree, host side: command builders, output parsers and the proto assembly that
//! BOTH the helper (`daemon/repo_meta.rs`) and the shell backend (`backend/shell/tree.rs`) use,
//! so the two answer `NavigatedToDirectory` / `LoadRepoMetadataDirectory` with the same shapes.
//!
//! No I/O, no warpui. Wave H parity, P3 (family `b`).
//!
//! # Shapes
//!
//! * A directory's children are [`TreeChild`]ren, sorted by [`sort_children`] (directories first,
//!   then by name) so both backends order them the same.
//! * A directory is *loaded* when its children are known. A child directory with no listing yet is
//!   sent `loaded = false` and the client asks for it with `LoadRepoMetadataDirectory`.
//! * Git-ignored entries are listed with `ignored = true`; ignored directories are never loaded
//!   eagerly. `.git` is always listed as an ignored directory. A symlink to a directory is
//!   omitted (the old server's tree omitted them).
//!
//! # Commands
//!
//! The builders return source for one framed section; `kind` picks POSIX or PowerShell. The
//! directory listing prefers GNU `find -printf`, and falls back to `ls -1Ap` where `find` has no
//! `-printf` (BSD, macOS, busybox). Both print records of the form `<y><Y> <name>` terminated by
//! NUL (`y` the entry's type, `Y` its target's type for a symlink), which [`parse_list_records`]
//! reads.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::frame::Frame;
use super::{ShellKind, sh_quote};
use crate::proto;

/// Bytes of `git ls-files` output read for a repository snapshot. A repository over this is
/// served lazily, one directory at a time.
pub const MAX_REPO_LIST_BYTES: usize = 4 * 1024 * 1024;
/// Bytes of the ignored-paths listing read for a snapshot.
pub const MAX_IGNORED_LIST_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeChild {
    pub name: String,
    pub is_dir: bool,
    pub ignored: bool,
}

/// Directories first, then by name.
pub fn sort_children(children: &mut [TreeChild]) {
    children.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
}

/// The separator a host path uses: `\` for a Windows-style path, `/` otherwise.
fn separator_of(path: &str) -> char {
    if path.contains('\\') && !path.contains('/') {
        '\\'
    } else {
        '/'
    }
}

/// `parent` + `name` with the host's separator, without doubling one at the end of `parent`.
pub fn join_path(parent: &str, name: &str) -> String {
    let sep = separator_of(parent);
    if parent.ends_with(['/', '\\']) {
        format!("{parent}{name}")
    } else {
        format!("{parent}{sep}{name}")
    }
}

/// The parent of an absolute host path (`None` at a root).
pub fn parent_of(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let idx = trimmed.rfind(['/', '\\'])?;
    if idx == 0 {
        // "/name" -> "/"
        return (trimmed.len() > 1).then(|| &trimmed[..1]);
    }
    Some(&trimmed[..idx])
}

/// Whether `path` is `dir` or below it.
pub fn is_within(dir: &str, path: &str) -> bool {
    let dir = dir.trim_end_matches(['/', '\\']);
    path == dir
        || path
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with(['/', '\\']))
}

fn extension_of(name: &str) -> Option<String> {
    std::path::Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
}

pub fn node_of(parent: &str, child: &TreeChild, loaded: bool) -> proto::RepoNodeMetadata {
    let path = join_path(parent, &child.name);
    let node = if child.is_dir {
        proto::repo_node_metadata::Node::Directory(proto::DirectoryNodeMetadata {
            path,
            ignored: child.ignored,
            loaded,
        })
    } else {
        proto::repo_node_metadata::Node::File(proto::FileNodeMetadata {
            path,
            extension: extension_of(&child.name),
            ignored: child.ignored,
        })
    };
    proto::RepoNodeMetadata { node: Some(node) }
}

/// One directory's children as the entry update that replaces/inserts them under `parent`.
/// `is_loaded(child_dir_path)` says whether that child directory's own listing is known.
pub fn dir_update(
    parent: &str,
    children: &[TreeChild],
    is_loaded: &dyn Fn(&str) -> bool,
) -> proto::RepoMetadataEntryUpdate {
    proto::RepoMetadataEntryUpdate {
        parent_path_to_replace: parent.to_owned(),
        subtree_metadata: children
            .iter()
            .map(|c| {
                let loaded = c.is_dir && !c.ignored && is_loaded(&join_path(parent, &c.name));
                node_of(parent, c, loaded)
            })
            .collect(),
    }
}

/// Whether `dir` sits inside an ignored directory: its parent's listing flags it ignored.
/// Everything inside such a directory is ignored whatever the rules say.
pub fn dir_is_ignored(dirs: &BTreeMap<String, Vec<TreeChild>>, dir: &str) -> bool {
    let Some(parent) = parent_of(dir) else {
        return false;
    };
    let name = dir[parent.trim_end_matches(['/', '\\']).len()..].trim_start_matches(['/', '\\']);
    dirs.get(parent)
        .and_then(|cs| cs.iter().find(|c| c.name == name))
        .is_some_and(|c| c.ignored)
}

/// The snapshot entries for the known directory listings in `dirs` (keyed by absolute path),
/// breadth-first from `root`, one update per non-empty directory. Directories whose listing is
/// not in `dirs` are sent unloaded.
pub fn snapshot_entries(
    root: &str,
    dirs: &BTreeMap<String, Vec<TreeChild>>,
) -> Vec<proto::RepoMetadataEntryUpdate> {
    let mut out = Vec::new();
    let mut queue = VecDeque::from([root.to_owned()]);
    let loaded = |p: &str| dirs.contains_key(p);
    while let Some(dir) = queue.pop_front() {
        let Some(children) = dirs.get(&dir) else {
            continue;
        };
        if children.is_empty() {
            continue;
        }
        for c in children {
            if c.is_dir && !c.ignored {
                queue.push_back(join_path(&dir, &c.name));
            }
        }
        out.push(dir_update(&dir, children, &loaded));
    }
    out
}

/// Groups repo-relative paths into directory listings. `files` are the non-ignored tracked and
/// untracked files; `ignored` are ignored paths, a trailing `/` marking a collapsed directory.
/// Every ancestor directory of a file is listed and loaded; an ignored directory is listed
/// unloaded. `root` is the absolute repo root.
pub fn tree_from_paths(
    root: &str,
    files: &[String],
    ignored: &[String],
) -> BTreeMap<String, Vec<TreeChild>> {
    type Dir = BTreeMap<String, (bool, bool)>; // name -> (is_dir, ignored)
    let mut dirs: BTreeMap<String, Dir> = BTreeMap::new();
    dirs.entry(root.to_owned()).or_default();

    let mut add = |rel: &str, last_is_dir: bool, last_ignored: bool| {
        let comps: Vec<&str> = rel.split('/').filter(|c| !c.is_empty()).collect();
        let mut dir = root.to_owned();
        for (i, comp) in comps.iter().enumerate() {
            let last = i + 1 == comps.len();
            let (is_dir, ign) = if last {
                (last_is_dir, last_ignored)
            } else {
                (true, false)
            };
            dirs.entry(dir.clone())
                .or_default()
                .entry((*comp).to_owned())
                .and_modify(|e| {
                    // A directory seen again keeps its first flags; ignored wins for a leaf.
                    if last {
                        *e = (is_dir, ign);
                    }
                })
                .or_insert((is_dir, ign));
            if !last {
                dir = join_path(&dir, comp);
                dirs.entry(dir.clone()).or_default();
            }
        }
    };
    for f in files {
        add(f, false, false);
    }
    for i in ignored {
        let is_dir = i.ends_with('/');
        add(i.trim_end_matches('/'), is_dir, true);
    }

    dirs.into_iter()
        .map(|(dir, entries)| {
            let mut children: Vec<TreeChild> = entries
                .into_iter()
                .map(|(name, (is_dir, ignored))| TreeChild {
                    name,
                    is_dir,
                    ignored,
                })
                .collect();
            sort_children(&mut children);
            (dir, children)
        })
        .collect()
}

/// The difference between a directory's old and new listing, as the pieces of a
/// `RepoMetadataUpdatePush`: paths to remove and the nodes to insert under `dir`.
pub fn diff_dir(
    dir: &str,
    old: &[TreeChild],
    new: &[TreeChild],
) -> (Vec<String>, Vec<proto::RepoNodeMetadata>) {
    let old_by_name: BTreeMap<&str, &TreeChild> =
        old.iter().map(|c| (c.name.as_str(), c)).collect();
    let new_by_name: BTreeMap<&str, &TreeChild> =
        new.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut removed = Vec::new();
    let mut added = Vec::new();
    for c in old {
        match new_by_name.get(c.name.as_str()) {
            None => removed.push(join_path(dir, &c.name)),
            Some(n) if n.is_dir != c.is_dir || (n.is_dir && n.ignored != c.ignored) => {
                removed.push(join_path(dir, &c.name));
            }
            Some(_) => {}
        }
    }
    for c in new {
        match old_by_name.get(c.name.as_str()) {
            Some(o) if o == &c => {}
            // New, or replaced above, or a file whose ignored flag changed (the client updates
            // an existing file in place).
            _ => added.push(node_of(dir, c, false)),
        }
    }
    (removed, added)
}

/// Applies a freshly computed set of listings `new` to the cached `old` and returns what the
/// client must be told: removed paths and per-directory insertions. Only directories present in
/// BOTH are diffed (a new directory arrives as an unloaded node in its parent's diff and is
/// listed when the client expands it; a cached directory `new` does not mention, such as a lazily
/// loaded ignored one, is left alone). Removed directories take their cached subtree with them.
pub fn apply_refresh(
    old: &mut BTreeMap<String, Vec<TreeChild>>,
    new: &BTreeMap<String, Vec<TreeChild>>,
) -> (Vec<String>, Vec<proto::RepoMetadataEntryUpdate>) {
    let mut removed = Vec::new();
    let mut updates = Vec::new();
    for (dir, new_children) in new {
        let Some(old_children) = old.get(dir).cloned() else {
            continue;
        };
        let (gone, added) = diff_dir(dir, &old_children, new_children);
        for path in gone {
            let below: Vec<String> = old
                .keys()
                .filter(|k| is_within(&path, k))
                .cloned()
                .collect();
            for k in below {
                old.remove(&k);
            }
            removed.push(path);
        }
        if !added.is_empty() {
            updates.push(proto::RepoMetadataEntryUpdate {
                parent_path_to_replace: dir.clone(),
                subtree_metadata: added,
            });
        }
        old.insert(dir.clone(), new_children.clone());
    }
    (removed, updates)
}

// ---------------------------------------------------------------- command builders

fn pwsh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn quote(kind: ShellKind, s: &str) -> String {
    if kind.is_powershell() {
        pwsh_quote(s)
    } else {
        sh_quote(s)
    }
}

/// Section `canon`: prints the canonical absolute path of the directory `path`; exits 2 when it
/// is not a directory.
pub fn canonical_dir_command(kind: ShellKind, path: &str) -> String {
    let q = quote(kind, path);
    if kind.is_powershell() {
        format!(
            "$p = Resolve-Path -LiteralPath {q} -ErrorAction SilentlyContinue\n\
             if (-not $p -or -not (Test-Path -LiteralPath $p.ProviderPath -PathType Container)) {{ exit 2 }}\n\
             $p.ProviderPath"
        )
    } else {
        format!("cd -- {q} 2>/dev/null || exit 2\npwd -P")
    }
}

/// Section `git`: prints the git work tree root containing `path`, nothing when there is none.
pub fn git_root_command(kind: ShellKind, path: &str) -> String {
    let q = quote(kind, path);
    if kind.is_powershell() {
        format!("Set-Location -LiteralPath {q}\ngit rev-parse --show-toplevel")
    } else {
        format!("cd -- {q} 2>/dev/null || exit 2\ngit rev-parse --show-toplevel")
    }
}

/// Section `list`: the children of directory `path` as NUL-terminated `<y><Y> <name>` records.
pub fn list_dir_command(kind: ShellKind, path: &str) -> String {
    let q = quote(kind, path);
    if kind.is_powershell() {
        format!(
            "Get-ChildItem -LiteralPath {q} -Force | ForEach-Object {{\n\
             $y = if ($_.LinkType) {{ 'l' }} elseif ($_.PSIsContainer) {{ 'd' }} else {{ 'f' }}\n\
             $t = if ($_.PSIsContainer) {{ 'd' }} else {{ 'f' }}\n\
             [Console]::Out.Write($y + $t + ' ' + $_.Name + [char]0)\n\
             }}"
        )
    } else {
        format!(
            "cd -- {q} 2>/dev/null || exit 2\n\
             if find . -maxdepth 0 -printf '' >/dev/null 2>&1; then\n\
             find . -mindepth 1 -maxdepth 1 -printf '%y%Y %f\\0'\n\
             else\n\
             ls -1Ap | while IFS= read -r n; do case \"$n\" in */) printf 'dd %s\\0' \"${{n%/}}\";; *) printf 'ff %s\\0' \"$n\";; esac; done\n\
             fi"
        )
    }
}

/// Section `list`, with metadata: the children of directory `path` as NUL-terminated
/// `<y><Y> <size> <mtime> <name>` records (`mtime` in seconds, possibly fractional), for
/// `ListDirectory`, which reports sizes, modification times and symlinks (#1878). Read by
/// [`parse_stat_records`]. The same `find -printf`, then `ls -1Ap` + `stat` fallback as
/// [`list_dir_command`].
pub fn list_dir_stat_command(kind: ShellKind, path: &str) -> String {
    let q = quote(kind, path);
    if kind.is_powershell() {
        format!(
            "Get-ChildItem -LiteralPath {q} -Force | ForEach-Object {{\n\
             $y = if ($_.LinkType) {{ 'l' }} elseif ($_.PSIsContainer) {{ 'd' }} else {{ 'f' }}\n\
             $t = if ($_.PSIsContainer) {{ 'd' }} else {{ 'f' }}\n\
             $s = if ($_.PSIsContainer) {{ 0 }} else {{ $_.Length }}\n\
             $ms = [DateTimeOffset]::new($_.LastWriteTimeUtc).ToUnixTimeMilliseconds()\n\
             $m = '{{0}}.{{1:D3}}' -f [math]::Floor($ms / 1000), ($ms % 1000)\n\
             [Console]::Out.Write($y + $t + ' ' + $s + ' ' + $m + ' ' + $_.Name + [char]0)\n\
             }}"
        )
    } else {
        format!(
            "cd -- {q} 2>/dev/null || exit 2\n\
             if find . -maxdepth 0 -printf '' >/dev/null 2>&1; then\n\
             find . -mindepth 1 -maxdepth 1 -printf '%y%Y %s %T@ %f\\0'\n\
             else\n\
             ls -1Ap | while IFS= read -r n; do\n\
             case \"$n\" in */) t=d; n=\"${{n%/}}\";; *) t=f;; esac\n\
             y=$t; if [ -L \"$n\" ]; then y=l; fi\n\
             s=$(stat -c '%s %Y' -- \"$n\" 2>/dev/null || stat -f '%z %m' -- \"$n\" 2>/dev/null || echo '0 0')\n\
             printf '%s%s %s %s %s\\0' \"$y\" \"$t\" \"${{s% *}}\" \"${{s#* }}\" \"$n\"\n\
             done\n\
             fi"
        )
    }
}

/// One entry of [`parse_stat_records`].
#[derive(Clone, Debug, PartialEq)]
pub struct StatEntry {
    pub name: String,
    /// `find`'s type letter for the entry itself: `d`, `f`, `l`, or another for special files.
    pub kind: char,
    pub size: u64,
    pub modified_epoch_millis: Option<u64>,
}

/// Reads the output of [`list_dir_stat_command`].
pub fn parse_stat_records(bytes: &[u8]) -> Vec<StatEntry> {
    bytes
        .split(|&b| b == 0)
        .filter_map(|record| {
            let text = String::from_utf8_lossy(record);
            let text = text.trim_start_matches('\n');
            let mut parts = text.splitn(4, ' ');
            let types = parts.next()?;
            let size = parts.next()?;
            let mtime = parts.next()?;
            let name = parts.next()?;
            if types.chars().count() != 2 || name.is_empty() || name == "." || name == ".." {
                return None;
            }
            Some(StatEntry {
                name: name.to_owned(),
                kind: types.chars().next()?,
                size: size.parse().unwrap_or(0),
                modified_epoch_millis: mtime
                    .parse::<f64>()
                    .ok()
                    .filter(|secs| secs.is_finite() && *secs >= 0.0)
                    .map(|secs| (secs * 1000.0).round() as u64),
            })
        })
        .collect()
}

/// Section `ignored`: which children of directory `path` git ignores (names, NUL separated).
/// Empty outside a git repository.
pub fn ignored_children_command(kind: ShellKind, path: &str) -> String {
    let q = quote(kind, path);
    if kind.is_powershell() {
        format!(
            "Set-Location -LiteralPath {q}\n\
             (Get-ChildItem -LiteralPath . -Force).Name | git check-ignore --stdin"
        )
    } else {
        format!(
            "cd -- {q} 2>/dev/null || exit 0\n\
             ls -1A | tr '\\n' '\\000' | git check-ignore -z --stdin 2>/dev/null\n\
             exit 0"
        )
    }
}

/// Section `files`: every non-ignored file of the repository at `root`, capped.
pub fn repo_files_command(kind: ShellKind, root: &str) -> String {
    repo_files_command_capped(kind, root, MAX_REPO_LIST_BYTES)
}

/// [`repo_files_command`] with the list cut at `cap` bytes (the batched refresh asks for less
/// than the standalone call and falls back to the standalone one when the list reaches it).
pub fn repo_files_command_capped(kind: ShellKind, root: &str, cap: usize) -> String {
    let q = quote(kind, root);
    if kind.is_powershell() {
        format!(
            "Set-Location -LiteralPath {q}\ngit -c core.quotepath=off ls-files -co --exclude-standard"
        )
    } else {
        format!(
            "cd -- {q} 2>/dev/null || exit 2\n\
             git ls-files -co --exclude-standard -z 2>/dev/null | head -c {cap}"
        )
    }
}

/// Section `ignoredpaths`: the repository's ignored paths, directories collapsed to `dir/`.
pub fn repo_ignored_command(kind: ShellKind, root: &str) -> String {
    repo_ignored_command_capped(kind, root, MAX_IGNORED_LIST_BYTES)
}

/// [`repo_ignored_command`] with the list cut at `cap` bytes.
pub fn repo_ignored_command_capped(kind: ShellKind, root: &str, cap: usize) -> String {
    let q = quote(kind, root);
    if kind.is_powershell() {
        format!(
            "Set-Location -LiteralPath {q}\n\
             git -c core.quotepath=off ls-files -oi --exclude-standard --directory"
        )
    } else {
        format!(
            "cd -- {q} 2>/dev/null || exit 2\n\
             git ls-files -oi --exclude-standard --directory -z 2>/dev/null | head -c {cap}"
        )
    }
}

// ---------------------------------------------------------------- parsers

/// Splits a path list: NUL separated when it contains a NUL, otherwise newline separated (with
/// CR trimmed). The final record is dropped when `truncated` (it may be cut short).
pub fn parse_path_list(bytes: &[u8], truncated: bool) -> Vec<String> {
    let sep = if bytes.contains(&0) { 0u8 } else { b'\n' };
    let mut records: Vec<&[u8]> = bytes.split(|&b| b == sep).collect();
    // A terminator leaves an empty final record; a cut-off list leaves a partial one.
    if let Some(last) = records.last() {
        if last.is_empty() || truncated {
            records.pop();
        }
    }
    records
        .into_iter()
        .map(|r| String::from_utf8_lossy(r).trim_end_matches('\r').to_owned())
        .filter(|r| !r.is_empty())
        .collect()
}

/// Reads the `list` section into children (unsorted, `ignored = false`). Symlinks to
/// directories, sockets, fifos and devices are dropped.
pub fn parse_list_records(bytes: &[u8]) -> Vec<TreeChild> {
    bytes
        .split(|&b| b == 0)
        .filter_map(|record| {
            let text = String::from_utf8_lossy(record);
            let text = text.trim_start_matches('\n');
            let mut chars = text.chars();
            let y = chars.next()?;
            let target = chars.next()?;
            if chars.next()? != ' ' {
                return None;
            }
            let name: String = chars.collect();
            if name.is_empty() || name == "." || name == ".." {
                return None;
            }
            match (y, target) {
                ('d', _) => Some(TreeChild {
                    name,
                    is_dir: true,
                    ignored: false,
                }),
                ('f', _) | ('l', 'f' | 'N' | 'L' | '?') => Some(TreeChild {
                    name,
                    is_dir: false,
                    ignored: false,
                }),
                // A symlink to a directory, or a special file.
                _ => None,
            }
        })
        .collect()
}

/// Applies the ignored-names section and the `.git` rule to a parsed listing, then sorts it.
/// `in_git` false leaves everything unignored (the old server's non-git tree had no ignore
/// rules).
pub fn finish_children(
    mut children: Vec<TreeChild>,
    ignored_names: &BTreeSet<String>,
    in_git: bool,
) -> Vec<TreeChild> {
    for c in &mut children {
        if in_git && (c.name == ".git" || ignored_names.contains(&c.name)) {
            c.ignored = true;
        }
    }
    sort_children(&mut children);
    children
}

/// What `NavigatedToDirectory` resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavTarget {
    /// The canonical directory the client named.
    pub dir: String,
    /// The git root containing it, if any.
    pub git_root: Option<String>,
}

/// Reads the `canon` and `git` sections. `Err` carries the message for an `Invalid path` error.
pub fn parse_nav_target(frame: &Frame) -> Result<NavTarget, String> {
    let canon = frame
        .section("canon")
        .ok_or_else(|| "no answer from the host".to_owned())?;
    if !canon.success() {
        return Err("not a directory, or it cannot be read".to_owned());
    }
    let dir = canon.text().trim().to_owned();
    if dir.is_empty() {
        return Err("the host returned no path".to_owned());
    }
    let git_root = frame
        .section("git")
        .filter(|s| s.success())
        .map(|s| s.text().trim().to_owned())
        .filter(|s| !s.is_empty());
    Ok(NavTarget { dir, git_root })
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;

# Phosphor bootstrap for nushell (0.116+). Sourced from a temp file by `source '<path>'`, typed
# with a leading space (history.ignore_space_prefixed keeps it out of history). It runs after
# nu_init_shell.nu / nu_init_subshell.nu emitted InitShell, so $env.WARP_SESSION_ID is set.
#
# Protocol: every hook is a JSON object hex-encoded inside a DCS (ESC P $ d <hex> 0x9c), or an
# OSC 9278 ; d ; <hex> BEL on Windows ConPTY. Payload shapes match fish.sh.
# Nushell has no way to evaluate a string, so this is a file; comment lines are dropped by the
# loader, so no comment may sit inside a multi-line string.

# Folder-independent: the app may ask for a working directory and extra PATH entries.
if ($env.WARP_INITIAL_WORKING_DIR? | default "" | is-not-empty) {
    try { cd $env.WARP_INITIAL_WORKING_DIR }
    hide-env WARP_INITIAL_WORKING_DIR
}
if ($env.WARP_PATH_APPEND? | default "" | is-not-empty) {
    $env.PATH = ($env.PATH | split row (char esep) | append ($env.WARP_PATH_APPEND | split row (char esep)))
    hide-env WARP_PATH_APPEND
}

# A JSON string literal (quotes included) for any string. nushell's `to json` leaves control
# characters raw, which is not valid JSON, so they are written as \u00XX here.
def __warp_jstr [s: string]: nothing -> string {
    let e = ($s | str replace -a '\' '\\' | str replace -a '"' '\"' | str replace -a (char nl) '\u000a')
    if ($e =~ '[\x00-\x1f\x7f]') {
        let digits = ('0123456789abcdef' | split chars)
        let body = ($e | split chars | each {|c|
            if ($c =~ '^[\x00-\x1f\x7f]$') {
                let b = ($"0x($c | into binary | encode hex)" | into int)
                $"\\u00($digits | get ($b // 16))($digits | get ($b mod 16))"
            } else {
                $c
            }
        } | str join)
        $'"($body)"'
    } else {
        $'"($e)"'
    }
}

# Sends one hook: hex-encoded JSON in a DCS (or an OSC under Windows ConPTY).
def __warp_send [json: string] {
    let hex = ($json | encode hex --lower)
    if ($env.WARP_USING_WINDOWS_CON_PTY? == "true") {
        print -n $"\e]9278;d;($hex)\u{7}"
    } else {
        print -n $"\eP$d($hex)"
        print -n -r 0x[9c]
    }
}

# Drops the replies to terminal queries (DA, cursor position, colour) that ended up in the line
# buffer; same patterns as __warp_strip_terminal_replies in fish.sh.
def __warp_strip_terminal_replies [text: string]: nothing -> string {
    $text
    | str replace -a -r '\x1b\[[?>=]?[0-9;]*[cRt]' ''
    | str replace -a -r '\x1b\[\??[0-9;]*\$y' ''
    | str replace -a -r '\x1bP>\|[^\x1b]*\x1b\\' ''
    | str replace -a -r '\x1b\][0-9;]*;rgba?:[0-9a-fA-F]{1,4}/[0-9a-fA-F]{1,4}/[0-9a-fA-F]{1,4}(?:\x07|\x1b\\)' ''
}

# The user's prompt text, for the Warp-prompt preview. Never throws.
def __warp_orig_prompt_text []: nothing -> string {
    let saved = ($env.__warp_orig_prompt? | default {})
    let render = {|v|
        if $v == null { "" } else if (($v | describe) | str starts-with "closure") { try { do $v | into string } catch { "" } } else { $v | into string }
    }
    let left = (do $render ($saved.cmd? | default null))
    let ind = (do $render ($saved.ind? | default null))
    $"($left)($ind)" | str replace -a (char nl) " " | str replace -a (char cr) " "
}

# Saves the user's prompt settings (while they are in effect) and applies the current mode:
# WARP_HONOR_PS1=1 keeps the user's prompt (nushell writes OSC 133 A/B around it), anything
# else blanks it, as fish.sh does.
def --env __warp_update_prompt_vars [] {
    if ($env.__warp_prompt_blank? | default false) == false {
        $env.__warp_orig_prompt = {
            cmd: ($env.PROMPT_COMMAND? | default null)
            right: ($env.PROMPT_COMMAND_RIGHT? | default null)
            ind: ($env.PROMPT_INDICATOR? | default null)
            vi_ins: ($env.PROMPT_INDICATOR_VI_INSERT? | default null)
            vi_norm: ($env.PROMPT_INDICATOR_VI_NORMAL? | default null)
            multi: ($env.PROMPT_MULTILINE_INDICATOR? | default null)
        }
    }
    let saved = $env.__warp_orig_prompt
    if ($env.WARP_HONOR_PS1? | default "1") == "0" {
        $env.PROMPT_COMMAND = {|| "" }
        $env.PROMPT_COMMAND_RIGHT = {|| "" }
        $env.PROMPT_INDICATOR = ""
        $env.PROMPT_INDICATOR_VI_INSERT = ""
        $env.PROMPT_INDICATOR_VI_NORMAL = ""
        $env.PROMPT_MULTILINE_INDICATOR = ""
        $env.__warp_prompt_blank = true
    } else if ($env.__warp_prompt_blank? | default false) {
        for pair in [[PROMPT_COMMAND cmd] [PROMPT_COMMAND_RIGHT right] [PROMPT_INDICATOR ind] [PROMPT_INDICATOR_VI_INSERT vi_ins] [PROMPT_INDICATOR_VI_NORMAL vi_norm] [PROMPT_MULTILINE_INDICATOR multi]] {
            let v = ($saved | get ($pair | get 1))
            if $v == null { hide-env -i ($pair | get 0) } else { load-env {($pair | get 0): $v} }
        }
        $env.__warp_prompt_blank = false
    }
}

# Bound to Alt-p / Alt-w: switch between the user's prompt and Warp's. Run through
# executehostcommand, so the next precmd/preexec must ignore them (__warp_internal).
def --env __warp_prompt_mode_ps1 [] {
    $env.__warp_internal = true
    $env.WARP_HONOR_PS1 = "1"
    __warp_update_prompt_vars
}
def --env __warp_prompt_mode_warp [] {
    $env.__warp_internal = true
    $env.WARP_HONOR_PS1 = "0"
    __warp_update_prompt_vars
}

# Bound to Alt-i: report the line buffer (typeahead) and clear it.
def --env __warp_report_input [] {
    $env.__warp_internal = true
    let buffer = (__warp_strip_terminal_replies (commandline))
    __warp_send $'{ "hook": "InputBuffer", "value": { "buffer": (__warp_jstr $buffer), "session_id": ($env.WARP_SESSION_ID) } }'
    commandline edit --replace ""
}

# Runs git read-only; returns "" when it fails or is missing.
def --wrapped __warp_git [...args: string]: nothing -> string {
    if (which git | is-empty) { return "" }
    let r = (with-env {GIT_OPTIONAL_LOCKS: "0"} { ^git ...$args } | complete)
    if $r.exit_code == 0 { $r.stdout | str trim } else { "" }
}

# Phosphor (#1233): finds the branch `git symbolic-ref --short HEAD` would print by reading
# .git/HEAD, without forking git (that fork ran on every prompt: ~3 ms of a 6 ms hook). Returns
# {branch, packed}: branch "" outside any repository (git prints nothing there either). Returns
# null, leaving the answer to git as before, for anything it cannot answer with certainty; the
# same cases as __warp_git_head_fast in fish.sh: a .git file (worktree, submodule), a bare repo or
# the .git directory itself, a symlinked .git, a repo owned by someone else (git refuses it),
# reftable (HEAD is refs/heads/.invalid), a detached HEAD (needs an abbreviated hash), a branch
# whose short name git would qualify because a ref of the same name exists elsewhere, and any
# error. The user name (one `id -un`, once, only inside a repository) is for the owner check. The walk starts at the physical path (`path expand` follows symlinks), as git's does.
def --env __warp_git_probe []: nothing -> any {
    mut p = ($env.PWD | path expand)
    mut gd = ""
    for _ in 1..64 {
        let g = ($p | path join .git)
        let t = (if ($g | path exists) { $g | path type | default "" } else { "" })
        if $t == "dir" { $gd = $g; break }
        if $t != "" { return null }
        if (($p | path join HEAD | path exists) and ($p | path join refs | path exists)) { return null }
        let parent = ($p | path dirname)
        if $parent == $p { return {branch: "", packed: false} }
        $p = $parent
    }
    if $gd == "" { return null }
    if ($env.__warp_me? | default "") == "" { $env.__warp_me = (do -i { ^id -un } | default "" | into string | str trim) }
    let me = $env.__warp_me
    if $me == "" { return null }
    let owners = (ls --long --directory $gd $p | get user)
    if ($owners | any {|u| $u != $me }) { return null }
    if not (($gd | path join objects | path exists) and ($gd | path join refs | path exists)) { return null }
    let line = (open --raw ($gd | path join HEAD) | lines | first)
    if not ($line | str starts-with "ref: refs/heads/") { return null }
    let branch = ($line | str substring 16..)
    if $branch == "" or $branch == ".invalid" { return null }
    if (($gd | path join $branch | path exists) or ($gd | path join refs $branch | path exists) or ($gd | path join refs tags $branch | path exists)) { return null }
    {branch: $branch, packed: ($gd | path join packed-refs | path exists)}
}

# The current branch ("" when there is none) and the HEAD name for the Precmd hook. A packed tag
# of the branch's name cannot be seen without reading all of packed-refs, so git is asked once per
# directory and branch in a repository that has one.
def --env __warp_git_info []: nothing -> record {
    if (which git | is-empty) { return {branch: "", head: ""} }
    let plain = ($nu.os-info.name != "windows" and $env.GIT_DIR? == null and $env.GIT_WORK_TREE? == null and $env.GIT_CEILING_DIRECTORIES? == null and $env.GIT_DISCOVERY_ACROSS_FILESYSTEM? == null and $env.GIT_NAMESPACE? == null and $env.GIT_COMMON_DIR? == null)
    if $plain {
        let r = (try { __warp_git_probe } catch { null })
        if $r != null {
            let key = $"($env.PWD)|($r.branch)"
            if not $r.packed or ($env.__warp_git_checked? | default "") == $key {
                return {branch: $r.branch, head: $r.branch}
            }
            if (__warp_git symbolic-ref --short HEAD) == $r.branch {
                $env.__warp_git_checked = $key
                return {branch: $r.branch, head: $r.branch}
            }
        }
    }
    let branch = (__warp_git symbolic-ref --short HEAD)
    {branch: $branch, head: (if ($branch | is-empty) { __warp_git rev-parse --short HEAD } else { $branch })}
}

# The Node.js version, only inside a git repository that holds a package.json, cached per PWD+PATH.
def --env __warp_node_version []: nothing -> string {
    if ($env.WARP_PROMPT_NODE_VERSION_ENABLED? | default "1") == "0" or (which node | is-empty) { return "" }
    let key = $"($env.PWD):($env.PATH | str join (char esep))"
    if ($env.__warp_node_key? | default "") == $key { return ($env.__warp_node_val? | default "") }
    mut dir = $env.PWD
    mut found = false
    mut in_repo = false
    loop {
        if ($dir | path join package.json | path exists) { $found = true; break }
        let parent = ($dir | path dirname)
        if $parent == $dir { break }
        $dir = $parent
    }
    if $found {
        loop {
            if ($dir | path join .git | path exists) { $in_repo = true; break }
            let parent = ($dir | path dirname)
            if $parent == $dir { break }
            $dir = $parent
        }
    }
    let version = if $found and $in_repo { (do -i { ^node --version } | default "" | into string | str trim) } else { "" }
    $env.__warp_node_key = $key
    $env.__warp_node_val = $version
    $version
}

# pre_prompt hook: CommandFinished then Precmd (see fish.sh warp_precmd).
def --env __warp_precmd [] {
    if ($env.__warp_internal? | default false) {
        $env.__warp_internal = false
        return
    }
    let code = ($env.LAST_EXIT_CODE? | default 0 | into int)
    let sid = $env.WARP_SESSION_ID
    let n = ($env.__warp_block_id? | default 0 | into int)
    let next_block_id = $"precmd-($sid)-($n)"
    __warp_send $'{"hook": "CommandFinished", "value": {"exit_code": ($code), "next_block_id": "($next_block_id)", "session_id": ($sid)}}'
    $env.__warp_block_id = ($n + 1)

    if ($env._WARP_GENERATOR_COMMAND? | default "" | is-not-empty) {
        hide-env _WARP_GENERATOR_COMMAND
        __warp_send $'{"hook": "Precmd", "value": {"exit_code": ($code), "next_block_id": "($next_block_id)", "pwd": "", "ps1": "", "git_head": "", "git_branch": "", "virtual_env": "", "conda_env": "", "node_version": "", "session_id": ($sid), "is_after_in_band_command": true}}'
        return
    }

    let pwd = (__warp_jstr $env.PWD)
    mut venv = '""'
    mut conda = '""'
    mut node = '""'
    mut git_head = '""'
    mut git_branch = '""'
    # Only fill these once bootstrapped: the blocks created during bootstrap have no visible
    # prompt, and git must not run before the user's PATH is complete.
    if ($env.WARP_BOOTSTRAPPED? | default "" | is-not-empty) {
        $venv = (__warp_jstr ($env.VIRTUAL_ENV? | default ""))
        $conda = (__warp_jstr ($env.CONDA_DEFAULT_ENV? | default ""))
        $node = (__warp_jstr (__warp_node_version))
        let git = (__warp_git_info)
        $git_branch = (__warp_jstr $git.branch)
        $git_head = (__warp_jstr $git.head)
    }

    __warp_update_prompt_vars
    let honor = ($env.WARP_HONOR_PS1? | default "1")
    let ps1_preview = if $honor == "1" { "" } else { __warp_orig_prompt_text | encode hex --lower }
    let ignore_markers = if $honor == "1" { "false" } else { "true" }
    let ps1 = if $honor == "1" { '""' } else { $'"($ps1_preview)"' }
    __warp_send $'{"hook": "Precmd", "value": {"exit_code": ($code), "next_block_id": "($next_block_id)", "pwd": ($pwd), "ignore_prompt_markers": ($ignore_markers), "ps1": ($ps1), "rprompt": "", "git_head": ($git_head), "git_branch": ($git_branch), "virtual_env": ($venv), "conda_env": ($conda), "node_version": ($node), "session_id": ($sid)}}'
}

# pre_execution hook.
def --env __warp_preexec [] {
    let line = (commandline)
    # The commands bound to keys run through executehostcommand; they are not user commands.
    if ($line | str trim | str starts-with "__warp_") { return }
    __warp_send $'{"hook": "Preexec", "value": {"command": (__warp_jstr $line), "session_id": ($env.WARP_SESSION_ID)}}'
}

# In-band generator: runs the POSIX command with bash (or sh), merges stderr into stdout, and
# answers with OSC 9277 ; A BEL <hex length> ; <hex of "<id>;<output>;<status>"> OSC 9277 ; B BEL.
# MATCHED PAIR with POSIX_GENERATOR_WRAPPER in in_band_command_executor.rs.
def --env warp_run_generator_command [command_id: string, command: string] {
    $env._WARP_GENERATOR_COMMAND = "1"
    let shell = if (which bash | is-not-empty) { "bash" } else { "sh" }
    let script = $"(char lp)\n($command)\n(char rp) 2>&1"
    let r = (do -i { ^$shell -c $script } | complete)
    let out = ($r.stdout? | default "" | into binary)
    let status = ($r.exit_code? | default 1 | into string)
    let hex = ([($"($command_id);" | into binary) $out ($";($status)" | into binary)] | bytes collect | encode hex --lower)
    print -n $"\e]9277;A\u{7}($hex | str length);($hex)\e]9277;B\u{7}"
    if ($env.WARP_USING_WINDOWS_CON_PTY? == "true") { print -n $"\e]9279\u{7}" }
}

# Replaces nushell's `clear`: the Clear hook makes Phosphor clear its own blocklist view.
def clear [--keep-scrollback (-k)] {
    __warp_send $'{"hook": "Clear", "value": {"session_id": ($env.WARP_SESSION_ID)}}'
}

# Reports the Bootstrapped hook (see warp_bootstrapped in fish.sh).
def __warp_bootstrapped [] {
    let cmds = (scope commands)
    let first_word = {|names| $names | each {|n| $n | split row ' ' | first } | uniq | sort | str join (char nl) }
    let builtins = (do $first_word ($cmds | where type in [built-in keyword plugin] | get name))
    let functions = (do $first_word ($cmds | where type == custom | get name))
    let aliases = (scope aliases | each {|a| $"($a.name) -> ($a.expansion | str replace -a (char nl) ' ')" } | str join (char nl))
    let env_names = ($env | columns | str join (char nl))
    let os = $nu.os-info.name
    let os_category = if $os == "macos" { "MacOS" } else if $os == "linux" { "Linux" } else if $os == "windows" { "Windows" } else { "" }
    let distro = if $os == "linux" {
        let rel = (["/etc/os-release" "/usr/lib/os-release"] | where {|p| $p | path exists } | get 0? | default "")
        if ($rel | is-empty) { "" } else { open --raw $rel | lines | where {|l| $l | str starts-with 'NAME="' } | get 0? | default "" | str replace 'NAME="' '' | str replace '"' '' }
    } else { "" }
    let histfile = if ($env.config.history.file_format? | default "plaintext") == "plaintext" { ($nu.history-path? | default "") } else { "" }
    # The default history format is sqlite; the app reads that file itself (#1203).
    let history_sqlite = if ($env.config.history.file_format? | default "plaintext") == "sqlite" { ($nu.history-path? | default "") } else { "" }
    let vi = if ($env.config.edit_mode? | default "emacs") == "vi" { "1" } else { "" }
    let options = if ($env.config.history.ignore_space_prefixed? | default true) { "ignore_space_prefixed" } else { "" }
    let path = ($env.PATH | each {|p| $p | into string } | str join (char esep))
    __warp_send $'{"hook": "Bootstrapped", "value": {"histfile": (__warp_jstr $histfile), "history_sqlite": (__warp_jstr $history_sqlite), "session_id": ($env.WARP_SESSION_ID), "shell": "nu", "home_dir": (__warp_jstr ($nu.home-dir? | default "")), "path": (__warp_jstr $path), "editor": (__warp_jstr ($env.EDITOR? | default "")), "abbreviations": "", "aliases": (__warp_jstr $aliases), "function_names": (__warp_jstr $functions), "env_var_names": (__warp_jstr $env_names), "builtins": (__warp_jstr $builtins), "keywords": "", "shell_version": (__warp_jstr (version | get version)), "shell_options": (__warp_jstr $options), "vi_mode_enabled": "($vi)", "os_category": "($os_category)", "linux_distribution": (__warp_jstr $distro), "wsl_name": (__warp_jstr ($env.WSL_DISTRO_NAME? | default "")), "shell_path": (__warp_jstr ($nu.current-exe? | default ""))}}'
}

# Hooks. Ours is first in pre_prompt so user hooks cannot disturb $env.LAST_EXIT_CODE before the
# exit code is read.
$env.config.hooks.pre_prompt = ($env.config.hooks.pre_prompt? | default [] | each {|h| $h } | prepend {|| __warp_precmd })
$env.config.hooks.pre_execution = ($env.config.hooks.pre_execution? | default [] | each {|h| $h } | append {|| __warp_preexec })

# Key bindings. Ctrl-P clears the line (the kill-buffer chord: DLE, see ShellType::kill_buffer_bytes),
# Alt-i reports the line buffer, Alt-p / Alt-w switch between the user's prompt and Warp's.
let _modes = [emacs vi_insert vi_normal]
$env.config.keybindings = ($env.config.keybindings | where {|k| not ($k.name? | default "" | str starts-with "warp_") } | append [
    {name: warp_kill_buffer, modifier: control, keycode: char_p, mode: $_modes, event: {edit: clear}}
    {name: warp_report_input, modifier: alt, keycode: char_i, mode: $_modes, event: {send: executehostcommand, cmd: " __warp_report_input"}}
    {name: warp_prompt_ps1, modifier: alt, keycode: char_p, mode: $_modes, event: {send: executehostcommand, cmd: " __warp_prompt_mode_ps1"}}
    {name: warp_prompt_warp, modifier: alt, keycode: char_w, mode: $_modes, event: {send: executehostcommand, cmd: " __warp_prompt_mode_warp"}}
])

# Autosuggestions are noise: input goes through Phosphor's editor.
$env.config.show_hints = false

# This script itself is one command; report it finished, then the session as bootstrapped.
__warp_precmd
__warp_bootstrapped
$env.WARP_BOOTSTRAPPED = "1"
# The line that sourced this file was the last one that needed to stay out of history; put the
# user's history.ignore_space_prefixed back (saved by the init script or the subshell stage).
if ($env.__wbh? != null) {
    $env.config.history.ignore_space_prefixed = $env.__wbh
    hide-env __wbh
}

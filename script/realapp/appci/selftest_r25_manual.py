# appci/selftest_r25_manual.py: offline self-test of the user manual against removed settings (#2178). Pure Python: no app, display or network.
import os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []

# Settings that no longer exist in the app. No docs/manual page may name one (as a setting, a table row or a source reference). Add a key here when a
# setting is deleted; the DECLINED.md row that records the removal is where the history lives, not the manual.
REMOVED_SETTINGS = [
    "warpify.ssh.use_ssh_tmux_wrapper",  # commit 692a68bfe: the tmux phosphorization option, its UI and its palette toggle
    "use_ssh_tmux_wrapper",
]


def check(f):
    CHECKS.append(f)
    return f


def _manual_pages(root=None):
    root = root or os.path.abspath(os.path.join(HERE, "..", "..", ".."))
    d = os.path.join(root, "docs", "manual")
    pages = sorted(n for n in os.listdir(d) if n.endswith(".md"))
    assert pages, "no manual pages under " + d
    return d, pages


@check
def manual_names_no_removed_setting():
    """#2178: no docs/manual page names a setting that has been removed from the app (start of the list: warpify.ssh.use_ssh_tmux_wrapper)"""
    d, pages = _manual_pages()
    hits = []
    for name in pages:
        with open(os.path.join(d, name), encoding="utf-8") as f:
            for n, line in enumerate(f.read().splitlines(), 1):
                for key in REMOVED_SETTINGS:
                    if key in line:
                        hits.append("%s:%d names the removed setting %s" % (name, n, key))
    assert not hits, "; ".join(hits)


@check
def manual_removed_settings_check_catches_a_mention():
    """the list above is not empty and its matcher fires on a line that names a removed setting (so a silently emptied list cannot pass)"""
    assert "warpify.ssh.use_ssh_tmux_wrapper" in REMOVED_SETTINGS
    line = "| `warpify.ssh.use_ssh_tmux_wrapper` | tmux | `false` |"
    assert any(k in line for k in REMOVED_SETTINGS)

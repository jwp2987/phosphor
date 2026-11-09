#!/usr/bin/env python3
# appci/fakecodex.py: a stand-in for the `codex plugin ...` commands the app runs to install the bundled plugin. It does what the real CLI does to $CODEX_HOME/config.toml and the
# plugin cache and nothing else, and it REFUSES to touch the network: a source that is not a local folder is an error (the bundled marketplace is a folder).
#   fakecodex.py plugin marketplace add <folder> | remove <name>
#   fakecodex.py plugin add <plugin>@<marketplace>
# Exit 0 on success, 1 on failure with the reason on stderr.
import json, os, re, shutil, sys

def home():
    return os.environ["CODEX_HOME"]

def cfg_path():
    return os.path.join(home(), "config.toml")

def read_cfg():
    try:
        return open(cfg_path()).read()
    except OSError:
        return ""

def drop_table(text, header):
    out, skip = [], False
    for line in text.splitlines(keepends=True):
        if line.startswith("["):
            skip = line.strip() == header
        if not skip:
            out.append(line)
    return "".join(out)

def fail(msg):
    print("fakecodex: " + msg, file=sys.stderr)
    return 1

def main(argv):
    os.makedirs(home(), exist_ok=True)
    if argv[:3] == ["plugin", "marketplace", "add"] and len(argv) == 4:
        src = argv[3]
        listing = os.path.join(src, ".agents", "plugins", "marketplace.json")
        if not os.path.isabs(src) or "://" in src or not os.path.isfile(listing):
            return fail("not a local marketplace folder (a remote source would need the network): %r" % src)
        name = json.load(open(listing))["name"]
        text = read_cfg()
        if re.search(r"^\[marketplaces\.%s\]" % re.escape(name), text, re.M):
            return fail("marketplace %r is already added from a different source; remove it before adding this source" % name)
        text += '\n[marketplaces.%s]\nsource_type = "local"\nsource = "%s"\n' % (name, src)
        open(cfg_path(), "w").write(text)
        return 0
    if argv[:3] == ["plugin", "marketplace", "remove"] and len(argv) == 4:
        open(cfg_path(), "w").write(drop_table(read_cfg(), "[marketplaces.%s]" % argv[3]))
        shutil.rmtree(os.path.join(home(), "plugins", "cache", argv[3]), ignore_errors=True)
        return 0
    if argv[:2] == ["plugin", "add"] and len(argv) == 3:
        plugin, _, market = argv[2].partition("@")
        m = re.search(r'^\[marketplaces\.%s\]\nsource_type = "local"\nsource = "([^"]+)"' % re.escape(market), read_cfg(), re.M)
        if not m:
            return fail("marketplace %r is not registered from a local folder" % market)
        root = m.group(1)
        listing = json.load(open(os.path.join(root, ".agents", "plugins", "marketplace.json")))
        item = next((p for p in listing.get("plugins", []) if p.get("name") == plugin), None)
        if not item:
            return fail("plugin %r is not in marketplace %r" % (plugin, market))
        src = os.path.normpath(os.path.join(root, item["source"]))
        manifest = json.load(open(os.path.join(src, ".codex-plugin", "plugin.json")))
        hooks = json.load(open(os.path.join(src, "hooks", "hooks.json")))
        for group in hooks["hooks"].values():
            for matcher in group:
                for hook in matcher["hooks"]:
                    script = hook["command"].replace("${PLUGIN_ROOT}", src)
                    if not (os.path.isfile(script) and os.access(script, os.X_OK)):
                        return fail("hook command is missing or not executable: %s" % script)
        dest = os.path.join(home(), "plugins", "cache", market, plugin, manifest["version"])
        shutil.rmtree(dest, ignore_errors=True)
        shutil.copytree(src, dest, symlinks=True)
        text = drop_table(read_cfg(), '[plugins."%s"]' % argv[2]) + '\n[plugins."%s"]\nenabled = true\n' % argv[2]
        open(cfg_path(), "w").write(text)
        return 0
    return fail("unsupported: %r" % argv)

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

#!/usr/bin/env python3
# appci/fakeclaude.py: a stand-in for the `claude plugin ...` commands the app runs to install the bundled plugin. It does what the real CLI does to its
# records and nothing else, and it REFUSES to touch the network: a source that is not a local folder is an error (the bundled marketplace is a folder).
#   fakeclaude.py plugin marketplace add <folder> | remove <name> | update <name>
#   fakeclaude.py plugin install <plugin>@<marketplace> | update <plugin>@<marketplace>
# State lives under $CLAUDE_CONFIG_DIR/plugins (known_marketplaces.json, installed_plugins.json). Exit 0 on success, 1 on failure with the reason on stderr.
import json, os, shutil, sys

def cfg():
    return os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "plugins")

def load(name):
    try:
        return json.load(open(os.path.join(cfg(), name)))
    except Exception:
        return {}

def save(name, data):
    os.makedirs(cfg(), exist_ok=True)
    json.dump(data, open(os.path.join(cfg(), name), "w"), indent=2)

def fail(msg):
    print("fakeclaude: " + msg, file=sys.stderr)
    return 1

def main(argv):
    if argv[:2] != ["plugin", "marketplace"] and argv[:2] != ["plugin", "install"] and argv[:2] != ["plugin", "update"]:
        return fail("unsupported: %r" % argv)
    if argv[1] == "marketplace":
        verb, arg = (argv[2:3] or [""])[0], (argv[3:4] or [""])[0]
        known = load("known_marketplaces.json")
        if verb == "add":
            if not os.path.isabs(arg) or "://" in arg or not os.path.isfile(os.path.join(arg, ".claude-plugin", "marketplace.json")):
                return fail("not a local marketplace folder (a remote source would need the network): %r" % arg)
            name = json.load(open(os.path.join(arg, ".claude-plugin", "marketplace.json")))["name"]
            known[name] = {"source": {"source": "directory", "path": arg}, "installLocation": arg}
            save("known_marketplaces.json", known)
            return 0
        if verb == "remove":
            known.pop(arg, None)
            save("known_marketplaces.json", known)
            inst = load("installed_plugins.json")
            inst.setdefault("plugins", {})
            for key in [k for k in inst["plugins"] if k.endswith("@" + arg)]:
                del inst["plugins"][key]
            save("installed_plugins.json", inst)
            return 0
        if verb == "update":
            return 0
        return fail("unsupported marketplace verb %r" % verb)
    key = (argv[2:3] or [""])[0]
    plugin, _, market = key.partition("@")
    entry = load("known_marketplaces.json").get(market)
    if not entry:
        return fail("marketplace %r is not registered" % market)
    root = entry["installLocation"]
    listing = json.load(open(os.path.join(root, ".claude-plugin", "marketplace.json")))
    item = next((p for p in listing.get("plugins", []) if p.get("name") == plugin), None)
    if not item:
        return fail("plugin %r is not in marketplace %r" % (plugin, market))
    src = os.path.normpath(os.path.join(root, item["source"]))
    manifest = json.load(open(os.path.join(src, ".claude-plugin", "plugin.json")))
    hooks = json.load(open(os.path.join(src, "hooks", "hooks.json")))
    for group in hooks["hooks"].values():
        for matcher in group:
            for hook in matcher["hooks"]:
                script = hook["command"].replace("${CLAUDE_PLUGIN_ROOT}", src)
                if not (os.path.isfile(script) and os.access(script, os.X_OK)):
                    return fail("hook command is missing or not executable: %s" % script)
    dest = os.path.join(cfg(), "cache", market, plugin, manifest["version"])
    shutil.rmtree(dest, ignore_errors=True)
    shutil.copytree(src, dest, symlinks=True)
    inst = load("installed_plugins.json")
    inst.setdefault("plugins", {})[key] = [{"version": manifest["version"], "installPath": dest}]
    save("installed_plugins.json", inst)
    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

#!/usr/bin/env python3
# appci/fakegemini.py: a stand-in for the `gemini extensions ...` commands the app runs to install the bundled extension. Like the real CLI, a LOCAL folder is copied into
# $GEMINI_CLI_HOME/.gemini/extensions/<name> and recorded in .gemini-extension-install.json as {"source": <folder>, "type": "local"}; it REFUSES any source that is not a local
# folder (a remote one would need the network).
#   fakegemini.py extensions install <folder> [--consent] | uninstall <name>
# Exit 0 on success, 1 on failure with the reason on stderr.
import json, os, shutil, sys

def ext_root():
    return os.path.join(os.environ["GEMINI_CLI_HOME"], ".gemini", "extensions")

def fail(msg):
    print("fakegemini: " + msg, file=sys.stderr)
    return 1

def main(argv):
    if argv[:2] == ["extensions", "install"] and len(argv) >= 3:
        src = argv[2]
        manifest = os.path.join(src, "gemini-extension.json")
        if not os.path.isabs(src) or "://" in src or not os.path.isfile(manifest):
            return fail("not a local extension folder (a remote source would need the network): %r" % src)
        name = json.load(open(manifest))["name"]
        dest = os.path.join(ext_root(), name)
        if os.path.exists(dest):
            return fail("Extension %r is already installed. Please uninstall it first." % name)
        hooks = json.load(open(os.path.join(src, "hooks", "hooks.json")))
        for group in hooks["hooks"].values():
            for matcher in group:
                for hook in matcher["hooks"]:
                    script = hook["command"].replace("${extensionPath}", dest)
                    if not script.startswith(dest):
                        return fail("hook command is outside the extension: %s" % script)
        shutil.copytree(src, dest, symlinks=True)
        json.dump({"source": src, "type": "local"}, open(os.path.join(dest, ".gemini-extension-install.json"), "w"))
        return 0
    if argv[:2] == ["extensions", "uninstall"] and len(argv) == 3:
        shutil.rmtree(os.path.join(ext_root(), argv[2]), ignore_errors=True)
        return 0
    return fail("unsupported: %r" % argv)

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

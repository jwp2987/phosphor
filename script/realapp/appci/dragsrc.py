#!/usr/bin/env python3
"""dragsrc.py: a drag SOURCE for the job's own display (r14-test-gaps T38, H8). xdotool and the virtual pointer can press, move and release, but a
drag needs a client that OWNS it (XDND selection on X11, wl_data_source on Wayland). This is that client: one small GTK 3 window whose whole area
starts a file drag (text/uri-list) of the paths on its command line. GTK does both display systems, so the same tool serves X11 and Wayland.

  dragsrc.py --probe                       exit 0 when python3-gi and GTK 3 can be loaded, else 3 with the reason on stderr (run it with GDK_BACKEND set:
                                           importing GTK through gi opens the display)
  dragsrc.py --ready FILE [--at X,Y] PATH...   show the window; FILE gets {"pid":..,"w":..,"h":..,"backend":..} when it is mapped, and one
                                               more line {"event": "begin" | "data" | "end" | "failed"} per drag event (the case reads them:
                                               "data" = a target asked for the files, i.e. the drop was accepted)

X11: the window is override-redirect (POPUP) and placed at --at (the job's Xvfb has no window manager, so nothing else would place or raise it).
Wayland: app_id is `appci_dragsrc`; the case makes it floating through sway before it starts.
Needs: python3-gi and gir1.2-gtk-3.0 (Debian / Ubuntu package names). Nothing is installed by the harness.
"""
import json
import os
import sys

W, H = 220, 120
APP_ID = "appci_dragsrc"


def file_uri(path):
    """file:// URI of an absolute path, percent-encoded as the uri-list format wants (a space is %20)"""
    from urllib.parse import quote
    return "file://" + quote(os.path.abspath(path))


def uri_list(paths):
    """the text/uri-list payload: one URI per line, CRLF line ends (RFC 2483)"""
    return "".join(file_uri(p) + "\r\n" for p in paths)


def parse_at(s):
    """'X,Y' -> (x, y); anything else -> (0, 0)"""
    try:
        x, y = s.split(",")
        return int(x), int(y)
    except (AttributeError, ValueError):
        return 0, 0


def probe():
    try:
        import gi
        gi.require_version("Gtk", "3.0")
        gi.require_version("Gdk", "3.0")
        from gi.repository import Gtk, Gdk  # noqa: F401
    except Exception as e:  # ImportError, ValueError (namespace not available)
        sys.stderr.write("%s: %s\n" % (type(e).__name__, e))
        return 3
    return 0


def run(ready, at, paths):
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gtk, Gdk, GLib

    def say(d):
        with open(ready, "a", encoding="utf-8") as f:
            f.write(json.dumps(d) + "\n")
    GLib.set_prgname(APP_ID)
    ok, _ = Gtk.init_check(sys.argv[:1])
    if not ok:
        say(dict(error="GTK could not open the display (DISPLAY=%r WAYLAND_DISPLAY=%r)" % (os.environ.get("DISPLAY"), os.environ.get("WAYLAND_DISPLAY"))))
        return 2
    backend = type(Gdk.Display.get_default()).__name__
    x11 = "X11" in backend
    win = Gtk.Window(type=Gtk.WindowType.POPUP if x11 else Gtk.WindowType.TOPLEVEL)
    win.set_title(APP_ID)
    win.set_default_size(W, H)
    win.set_decorated(False)
    box = Gtk.EventBox()
    box.add(Gtk.Label(label="DRAG SOURCE\n%d file(s)" % len(paths)))
    win.add(box)
    box.drag_source_set(Gdk.ModifierType.BUTTON1_MASK, [], Gdk.DragAction.COPY)
    box.drag_source_add_uri_targets()
    uris = [file_uri(p) for p in paths]
    box.connect("drag-begin", lambda *_a: say(dict(event="begin")))
    box.connect("drag-data-get", lambda _w, _c, data, _i, _t: (data.set_uris(uris), say(dict(event="data"))))
    box.connect("drag-end", lambda *_a: say(dict(event="end")))
    box.connect("drag-failed", lambda *_a: say(dict(event="failed")) or False)
    win.connect("destroy", Gtk.main_quit)

    def mapped(*_a):
        say(dict(pid=os.getpid(), w=W, h=H, backend=backend, at=list(at)))
        return False
    win.connect("map-event", mapped)
    if x11:
        win.move(*at)
    win.show_all()
    Gtk.main()
    return 0


def main(argv):
    if "--probe" in argv:
        return probe()
    if "--ready" not in argv:
        sys.stderr.write(__doc__)
        return 2
    ready = argv[argv.index("--ready") + 1]
    at = parse_at(argv[argv.index("--at") + 1]) if "--at" in argv else (0, 0)
    skip = {argv.index("--ready"), argv.index("--ready") + 1} | ({argv.index("--at"), argv.index("--at") + 1} if "--at" in argv else set())
    paths = [a for i, a in enumerate(argv) if i >= 1 and i not in skip]
    if not paths:
        sys.stderr.write("no file to drag\n")
        return 2
    return run(ready, at, paths)


if __name__ == "__main__":
    sys.exit(main(sys.argv))

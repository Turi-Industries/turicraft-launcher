#!/usr/bin/env python3
"""Capture une page avec WebKitGTK — le moteur du launcher sous Linux — dans
une fenêtre HORS ÉCRAN (rien ne s'affiche sur le bureau).

Chrome ne suffit pas : le 24/09, une mise en page juste dans Chrome
s'empilait dans le launcher (voir skill launcher-ux).

Usage : capture-webkit.py <url> <sortie.png> [largeur hauteur]
"""
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

url, out = sys.argv[1], sys.argv[2]
w = int(sys.argv[3]) if len(sys.argv) > 3 else 960
h = int(sys.argv[4]) if len(sys.argv) > 4 else 640

win = Gtk.OffscreenWindow()
win.set_default_size(w, h)
view = WebKit2.WebView()
view.set_size_request(w, h)
win.add(view)
win.show_all()


def done(v, result):
    try:
        surface = v.get_snapshot_finish(result)
        surface.write_to_png(out)
        print(out)
    except Exception as e:  # noqa: BLE001
        print(f"échec : {e}", file=sys.stderr)
    Gtk.main_quit()


def snap():
    view.get_snapshot(WebKit2.SnapshotRegion.VISIBLE, WebKit2.SnapshotOptions.NONE, None, done)
    return False


def on_load(v, event):
    if event == WebKit2.LoadEvent.FINISHED:
        GLib.timeout_add(2000, snap)  # laisser l'interface charger ses données


view.connect("load-changed", on_load)
view.load_uri(url)
GLib.timeout_add(20000, Gtk.main_quit)  # garde-fou
Gtk.main()

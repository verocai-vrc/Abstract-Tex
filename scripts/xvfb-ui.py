#!/usr/bin/env python3
"""Drive the app on an Xvfb display, for rung 4 of the verification ladder on a machine with no screen.

    scripts/xvfb-ui.py shot out.png          # screenshot of the whole display
    scripts/xvfb-ui.py click X Y             # left click at X, Y
    scripts/xvfb-ui.py key ctrl+shift+n      # a key or chord, as `xdotool key` spells it
    scripts/xvfb-ui.py type "some text"      # type text into whatever has focus

Uses Pillow for the screenshot and libxdo (installed with Tauri's Linux libraries) through ctypes
for input, so nothing else needs installing. XVFB_DISPLAY defaults to :99, which
`scripts/xvfb-app.sh` starts. Known limit: function keys (F5, F8) do not reach the page this way; click the button that
does the same thing instead. Typing into a GTK file chooser is unreliable: prefer
ABSTRACT_TEX_OPEN=<folder> over driving the picker.
"""
import ctypes
import os
import sys

# Not $DISPLAY: on a machine with a screen that is the real one, and clicking on it would be rude.
display = os.environ.get("XVFB_DISPLAY", ":99")

if len(sys.argv) < 2:
    sys.exit(__doc__)

if sys.argv[1] == "shot":
    from PIL import ImageGrab

    ImageGrab.grab(xdisplay=display).save(sys.argv[2])
    sys.exit()

xdo = ctypes.CDLL("libxdo.so.3")
xdo.xdo_new.restype = ctypes.c_void_p
handle = ctypes.c_void_p(xdo.xdo_new(display.encode()))
CURRENT_WINDOW = 0
DELAY_MICROSECONDS = 12000

if sys.argv[1] == "click":
    xdo.xdo_move_mouse(handle, int(sys.argv[2]), int(sys.argv[3]), 0)
    xdo.xdo_click_window(handle, CURRENT_WINDOW, 1)
elif sys.argv[1] == "key":
    xdo.xdo_send_keysequence_window(handle, CURRENT_WINDOW, sys.argv[2].encode(), DELAY_MICROSECONDS)
elif sys.argv[1] == "type":
    xdo.xdo_enter_text_window(handle, CURRENT_WINDOW, sys.argv[2].encode(), DELAY_MICROSECONDS)
else:
    sys.exit(__doc__)

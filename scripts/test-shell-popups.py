#!/usr/bin/python3
"""Use an isolated Xvfb/dbus-run-session; clicks only its virtual desktop."""
from pathlib import Path
import runpy
import subprocess
import sys

kind=sys.argv[1]
directory=Path(__file__).resolve().parent
module=runpy.run_path(str(directory/("winlux-start-menu.py" if kind=="start" else "winlux-popup.py")),run_name="popup_test")
app=module["Start"]() if kind=="start" else module["Popup"](kind)
failures=[]
completed=[]

def finish():
    try:
        assert not app.window.get_visible(),("Outside click did not close popup",getattr(app,"last_pointer_event",None),app.window.get_position(),app.window.get_size())
        completed.append(True)
    except Exception as error:failures.append(error)
    app.quit()
    return False

def click():
    try:
        assert app.window and app.window.get_visible(),"Popup did not open"
        assert getattr(app,"pointer_status",module["Gdk"].GrabStatus.SUCCESS)==module["Gdk"].GrabStatus.SUCCESS, getattr(app,"pointer_status",None)
        x,y=app.window.get_position();width,height=app.window.get_size()
        area=app.window.get_display().get_monitor(0).get_geometry()
        corners=[(area.x,area.y),(area.x+area.width-1,area.y+area.height-1),
                 (area.x+area.width-1,area.y),(area.x,area.y+area.height-1)]
        outside=next((px,py) for px,py in corners if not (x<=px<x+width and y<=py<y+height))
        subprocess.run(["xdotool","mousemove",str(outside[0]),str(outside[1]),"click","1"],check=True,timeout=5)
        module["GLib"].timeout_add(400,finish)
    except Exception as error:
        failures.append(error);app.quit()
    return False

def inside():
    try:
        assert app.window and app.window.get_visible(),"Popup did not open"
        x,y=app.window.get_position()
        subprocess.run(["xdotool","mousemove",str(x+20),str(y+20),"click","1"],check=True,timeout=5)
        module["GLib"].timeout_add(200,click)
    except Exception as error:
        failures.append(error);app.quit()
    return False

module["GLib"].timeout_add(800,inside)
result=app.run([])
if failures:raise failures[0]
assert result==0 and completed,"Popup test did not complete"
print(f"OK: {kind}, outside desktop click closes the popup")

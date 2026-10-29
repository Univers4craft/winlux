#!/usr/bin/python3
"""Compact, single-instance volume and power popups for the taskbar."""
from pathlib import Path
import re
import shutil
import subprocess
import sys
import gi
gi.require_version("Gtk","3.0")
gi.require_version("Gdk","3.0")
from gi.repository import Gtk,Gdk,Gio,GLib

def audio():
    try:
        if shutil.which("wpctl"):
            result=subprocess.run(["wpctl","get-volume","@DEFAULT_AUDIO_SINK@"],capture_output=True,text=True,timeout=2)
            found=re.search(r"Volume:\s*([0-9.]+)",result.stdout)
            if result.returncode==0 and found:return "wpctl",min(100,round(float(found[1])*100)),"MUTED" in result.stdout
        if shutil.which("amixer"):
            result=subprocess.run(["amixer","get","Master"],capture_output=True,text=True,timeout=2)
            found=re.findall(r"\[(\d+)%\]",result.stdout)
            if result.returncode==0 and found:return "amixer",int(found[-1]),"[off]" in result.stdout
    except (OSError,ValueError,subprocess.TimeoutExpired):pass
    return None,0,False

class Popup(Gtk.Application):
    def __init__(self,kind):
        super().__init__(application_id="org.winlux.Popup."+kind,flags=Gio.ApplicationFlags.FLAGS_NONE)
        self.kind=kind;self.window=None;self.timer=0;self.child=None;self.backend=None

    def hide(self,*_):
        if self.window.has_grab():self.window.grab_remove()
        self.window.get_display().get_default_seat().ungrab()
        self.window.hide()
        return True

    def activate_seat(self):
        if self.window.get_visible():
            self.pointer_status=self.window.get_display().get_default_seat().grab(
                self.window.get_window(),Gdk.SeatCapabilities.POINTER,False,None,None,None,None)
            if self.pointer_status==Gdk.GrabStatus.SUCCESS:self.window.grab_add()
        return False

    def outside(self,_,event):
        x,y=self.window.get_position();width,height=self.window.get_size()
        if not (x<=event.x_root<x+width and y<=event.y_root<y+height):return self.hide()
        return False

    def do_activate(self):
        if self.window and self.window.get_visible():self.hide();return
        if not self.window:self.build()
        self.window.show_all()
        self.refresh()
        self.window.present()
        monitor=self.window.get_display().get_monitor_at_point(*self.window.get_display().get_default_seat().get_pointer().get_position()[1:])
        area=monitor.get_workarea()
        width,height=self.window.get_size()
        self.window.move(area.x+area.width-width-12,area.y+area.height-height-12)
        GLib.idle_add(self.activate_seat)

    def build(self):
        self.window=Gtk.ApplicationWindow(application=self,title="Volume Winlux" if self.kind=="volume" else "Alimentation Winlux")
        self.window.set_name("winlux-popup")
        css=Gtk.CssProvider()
        css.load_from_data(b"#winlux-popup { background: #242238; color: #f7f5ff; border: 1px solid #6b5f92; border-radius: 8px; } #winlux-popup button { padding: 8px; border-radius: 5px; } #winlux-popup scale trough { min-height: 5px; } #winlux-popup scale highlight { background: #b19af9; }")
        Gtk.StyleContext.add_provider_for_screen(self.window.get_screen(),css,Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        self.window.set_decorated(False);self.window.set_resizable(False)
        self.window.set_default_size(340,-1);self.window.set_skip_taskbar_hint(True)
        self.window.set_type_hint(Gdk.WindowTypeHint.UTILITY);self.window.set_keep_above(True)
        self.window.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
        self.window.connect("button-press-event",self.outside)
        self.window.connect("key-press-event",lambda _,event:self.hide() if event.keyval==Gdk.KEY_Escape else False)
        self.window.connect("focus-out-event",lambda *_:self.hide())
        box=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=14,margin=20);self.window.add(box)
        self.label=Gtk.Label(xalign=0,wrap=True);box.pack_start(self.label,False,False,0)
        if self.kind=="volume":
            self.slider=Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL,0,100,1)
            self.slider.set_draw_value(True);self.slider.set_tooltip_text("Volume de sortie")
            self.slider.get_accessible().set_name("Volume de sortie, en pourcentage")
            box.pack_start(self.slider,False,False,0)
            self.slider.connect("value-changed",self.change_volume)
            self.mute=Gtk.Button(label="Muet / rétablir")
            self.mute.connect("clicked",lambda *_:self.command(["winlux-control","volume-mute"]))
            box.pack_start(self.mute,False,False,0)
        else:
            for title,action in [("Redémarrer", "reboot"), ("Éteindre", "poweroff")]:
                button=Gtk.Button(label=title)
                button.connect("clicked",lambda _,value=action:self.power_action(value))
                box.pack_start(button,False,False,0)
        button=Gtk.Button(label="Paramètres du son" if self.kind=="volume" else "Options d’alimentation")
        button.connect("clicked",lambda *_:self.settings());box.pack_start(button,False,False,0)
        GLib.timeout_add_seconds(1,self.refresh)

    def settings(self):
        self.hide();subprocess.Popen(["winlux-shell-app","settings","3" if self.kind=="volume" else "0"])

    def power_action(self,action):
        self.hide()
        dialog=Gtk.MessageDialog(transient_for=self.window,modal=True,
            buttons=Gtk.ButtonsType.OK_CANCEL,text="Éteindre l’ordinateur ?" if action=="poweroff" else "Redémarrer l’ordinateur ?",
            secondary_text="Enregistrez vos documents avant de continuer.")
        answer=dialog.run();dialog.destroy()
        if answer==Gtk.ResponseType.OK:self.command(["winlux-control",action])

    def command(self,args):
        if self.child and self.child.poll() is None:return
        self.child=subprocess.Popen(args,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)

    def change_volume(self,slider):
        if getattr(self,"updating",False):return
        if self.timer:GLib.source_remove(self.timer)
        self.timer=GLib.timeout_add(120,self.set_volume)

    def set_volume(self):
        self.timer=0
        if self.child and self.child.poll() is None:
            self.timer=GLib.timeout_add(60,self.set_volume)
            return False
        value=round(self.slider.get_value())
        if self.backend=="wpctl":self.command(["wpctl","set-volume","-l","1","@DEFAULT_AUDIO_SINK@",f"{value}%"])
        elif self.backend=="amixer":self.command(["amixer","set","Master",f"{value}%"])
        return False

    def refresh(self):
        if not self.window or not self.window.get_visible():return True
        if self.kind=="volume":
            self.backend,value,muted=audio()
            self.label.set_text("Sortie audio indisponible" if not self.backend else "Son coupé" if muted else f"Volume · {value} %")
            self.slider.set_sensitive(bool(self.backend));self.mute.set_sensitive(bool(self.backend))
            if not self.timer and not (self.child and self.child.poll() is None) and not self.slider.get_state_flags() & Gtk.StateFlags.ACTIVE:
                self.updating=True;self.slider.set_value(value);self.updating=False
        else:
            batteries=list(Path("/sys/class/power_supply").glob("BAT*/capacity"))
            if batteries:
                try:self.label.set_text(f"Batterie · {int(batteries[0].read_text())} %\n"+(batteries[0].parent/"status").read_text().strip())
                except (OSError,ValueError):self.label.set_text("État de batterie indisponible")
            else:self.label.set_text("Alimentation secteur\nAucune batterie exposée par cet ordinateur")
        return True

if __name__=="__main__":
    kind=sys.argv[1] if len(sys.argv)>1 else "volume"
    if kind not in ("volume","power"):raise SystemExit(2)
    raise SystemExit(Popup(kind).run([]))

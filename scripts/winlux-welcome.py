#!/usr/bin/python3
"""Optional, non-destructive first-session guide."""
import os
from pathlib import Path
import subprocess
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk

MARKER = Path.home() / ".config/winlux/welcome-complete"

class Welcome(Gtk.Window):
    def __init__(self):
        super().__init__(title="Bienvenue dans Winlux")
        self.set_default_size(620, 460)
        self.connect("destroy", Gtk.main_quit)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14, margin=24)
        self.add(box)
        title = Gtk.Label(label="Bienvenue dans Winlux", xalign=0)
        title.get_style_context().add_class("title")
        box.pack_start(title, False, False, 0)
        intro = Gtk.Label(label="Préparez votre ordinateur à votre rythme. Ces réglages restent accessibles à tout moment dans Paramètres.", xalign=0, wrap=True)
        box.pack_start(intro, False, False, 0)
        for label, command in (
            ("Choisir le clavier", ["winlux-shell-app", "settings", "4"]),
            ("Configurer le réseau", ["winlux-system-tools", "quick"]),
            ("Personnaliser l’apparence", ["winlux-system-tools", "appearance"]),
            ("Découvrir le secours", ["winlux-system-tools", "recovery"]),
        ):
            button = Gtk.Button(label=label)
            button.connect("clicked", lambda _, args=command: self.launch(args))
            box.pack_start(button, False, False, 0)
        hint = Gtk.Label(label="Astuce : Windows + E ouvre l’Explorateur, Windows + A les réglages rapides et Windows + N les notifications.", xalign=0, wrap=True)
        box.pack_start(hint, True, True, 0)
        finish = Gtk.Button(label="Terminer")
        finish.connect("clicked", self.finish)
        box.pack_end(finish, False, False, 0)

    def launch(self, command):
        try: subprocess.Popen(command)
        except OSError as error:
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, message_type=Gtk.MessageType.ERROR,
                                       buttons=Gtk.ButtonsType.CLOSE, text="Impossible d’ouvrir ce réglage")
            dialog.format_secondary_text(str(error)); dialog.run(); dialog.destroy()

    def finish(self, *_):
        try:
            MARKER.parent.mkdir(parents=True, exist_ok=True)
            temporary = MARKER.with_name(MARKER.name + "." + str(os.getpid()))
            temporary.write_text("1\n")
            temporary.replace(MARKER)
        except OSError as error:
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, message_type=Gtk.MessageType.ERROR,
                                       buttons=Gtk.ButtonsType.CLOSE, text="Impossible d’enregistrer cette préférence")
            dialog.format_secondary_text(str(error)); dialog.run(); dialog.destroy()
            return
        self.destroy()

if __name__ == "__main__":
    Welcome().show_all()
    Gtk.main()

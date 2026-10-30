#!/usr/bin/python3
"""Desktop utilities backed by Linux, XDG and the Winlux session services."""
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, Gio, GLib, GdkPixbuf

HOME = Path.home()
DATA = Path(os.environ.get("XDG_STATE_HOME",HOME/".local/state")) / "winlux"

def read(path):
    try: return Path(path).read_text(errors="replace")
    except OSError: return ""

def message(parent, text):
    dialog = Gtk.MessageDialog(transient_for=parent, modal=True, text=text,
        message_type=Gtk.MessageType.INFO, buttons=Gtk.ButtonsType.CLOSE)
    dialog.run()
    dialog.destroy()

class Tools(Gtk.Window):
    def __init__(self, page):
        super().__init__(title="Outils Winlux")
        self.set_default_size(850, 650)
        self.connect("destroy", Gtk.main_quit)
        self.page = page
        self.process = None
        self.previous = {}
        self.previous_time = time.monotonic()
        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12, margin=20)
        self.add(self.box)
        self.box.pack_start(Gtk.Label(label={"quick":"Réglages rapides", "notifications":"Notifications",
            "tasks":"Gestionnaire de tâches", "appearance":"Personnalisation", "apps":"Applications",
            "files":"Fichiers et périphériques", "recovery":"Secours et restauration"}.get(page, "Winlux"), xalign=0), False, False, 0)
        self.notice = Gtk.Label(label="", xalign=0, wrap=True, selectable=True)
        self.box.pack_end(self.notice, False, False, 0)
        self.content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        scroll = Gtk.ScrolledWindow()
        scroll.add(self.content)
        self.box.pack_start(scroll, True, True, 0)
        getattr(self, page)()
        GLib.timeout_add(1000, self.poll)
        self.show_all()

    def label(self, text):
        widget = Gtk.Label(label=text, xalign=0, wrap=True, selectable=True)
        self.content.pack_start(widget, False, False, 0)
        return widget

    def button(self, text, callback, available=True):
        widget = Gtk.Button(label=text)
        widget.set_sensitive(available)
        widget.connect("clicked", lambda *_: callback())
        self.content.pack_start(widget, False, False, 0)
        return widget

    def entry(self, placeholder):
        entry = Gtk.Entry(placeholder_text=placeholder)
        self.content.pack_start(entry, False, False, 0)
        return entry

    def command(self, args):
        if self.process is not None: return
        try:
            self.process = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            os.set_blocking(self.process.stdout.fileno(), False)
            self.output = bytearray()
            self.notice.set_text("Opération en cours…")
        except OSError as error: self.notice.set_text(str(error))

    def poll(self):
        if self.process is not None:
            chunk = self.process.stdout.read()
            if chunk: self.output.extend(chunk[:max(0, 64000-len(self.output))])
            if self.process.poll() is not None:
                self.notice.set_text(self.output.decode(errors="replace")[-8000:] or
                    ("Opération terminée." if self.process.returncode == 0 else "Échec de l’opération."))
                self.process.stdout.close()
                self.process = None
        if self.page == "tasks": self.refresh_tasks()
        if self.page == "notifications":
            stamp = read(DATA / "notifications.json")
            if stamp != self.notification_snapshot:
                for widget in self.content.get_children(): widget.destroy()
                self.notifications()
                self.content.show_all()
        return True

    def quick(self):
        self.label("Ethernet est prioritaire quand il est disponible. Le Wi-Fi peut rester configuré et prendre le relais lorsque le câble est débranché. Dans une VM, la connexion apparaît généralement comme Ethernet.")
        networks = Gtk.ComboBoxText()
        self.content.pack_start(networks, False, False, 0)
        def scan():
            self.notice.set_text("Recherche des réseaux Wi-Fi…")
            def worker():
                try:
                    result = subprocess.run(["nmcli", "-t", "-f", "BSSID,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", "yes"],
                                            capture_output=True, text=True, timeout=30, check=True)
                    rows = [split_nmcli(line) for line in result.stdout.splitlines()]
                    def display():
                        networks.remove_all()
                        count = 0
                        for row in rows:
                            if len(row) != 4 or not row[0] or not row[1]: continue
                            networks.append(row[0], f"{row[1]} · {row[2]} % · {row[3] or 'Ouvert'}")
                            count += 1
                        if count: networks.set_active(0)
                        self.notice.set_text(f"{count} réseau(x) trouvé(s)." if count else "Aucun réseau visible ou aucune carte Wi-Fi disponible.")
                        return False
                    GLib.idle_add(display)
                except (OSError, subprocess.SubprocessError) as error:
                    detail = getattr(error, "stderr", "") or str(error)
                    GLib.idle_add(lambda: (self.notice.set_text("Recherche Wi-Fi impossible : " + detail.strip()), False)[1])
            threading.Thread(target=worker, daemon=True).start()
        self.button("Rechercher les réseaux Wi-Fi", scan, bool(shutil.which("nmcli")))
        self.button("Se connecter au réseau sélectionné", lambda: self.command(["nmcli", "--wait", "90", "device", "wifi", "connect", networks.get_active_id()]) if networks.get_active_id() else self.notice.set_text("Choisissez d’abord un réseau."), bool(shutil.which("nmcli")))
        for title, args, binary in [
            ("Réseaux Wi-Fi disponibles", ["nmcli","-f","SSID,SIGNAL,SECURITY","device","wifi","list"],"nmcli"),
            ("Actualiser la recherche Wi-Fi", ["nmcli","device","wifi","rescan"],"nmcli"),
            ("État des cartes Ethernet et Wi-Fi", ["nmcli","device","status"],"nmcli"),
            ("Wi-Fi : activer", ["nmcli","radio","wifi","on"],"nmcli"),
            ("Wi-Fi : désactiver", ["nmcli","radio","wifi","off"],"nmcli"),
            ("Configurer les réseaux", ["nm-connection-editor"],"nm-connection-editor"),
            ("Bluetooth : activer", ["bluetoothctl","power","on"],"bluetoothctl"),
            ("Bluetooth : désactiver", ["bluetoothctl","power","off"],"bluetoothctl"),
            ("Associer un périphérique Bluetooth", ["blueman-manager"],"blueman-manager"),
            ("Luminosité −", ["brightnessctl","set","10%-"],"brightnessctl"),
            ("Luminosité +", ["brightnessctl","set","+10%"],"brightnessctl"),
            ("Volume −", ["winlux-control","volume-down"],"winlux-control"),
            ("Muet / rétablir", ["winlux-control","volume-mute"],"winlux-control"),
            ("Volume +", ["winlux-control","volume-up"],"winlux-control"),
            ("Alimentation", ["winlux-shell-app","settings","0"],"winlux-shell-app")]:
            self.button(title, lambda command=args: self.command(command), bool(shutil.which(binary)))
        ssid=self.entry("Nom du réseau Wi-Fi à rejoindre")
        self.button("Se connecter au réseau",lambda:self.command(["nmcli","--wait","90","device","wifi","connect",ssid.get_text()]) if ssid.get_text() else None,bool(shutil.which("nmcli")))
        self.label("Pour un réseau protégé, l’agent réseau demande le mot de passe. Si le point d’accès et l’agent proposent WPS, appuyez sur le bouton WPS de la box pendant la connexion. Les réseaux cachés et réglages avancés sont accessibles dans Configurer les réseaux.")

    def notification_call(self, method, parameters=None):
        try:
            Gio.bus_get_sync(Gio.BusType.SESSION, None).call_sync("org.freedesktop.Notifications",
                "/org/freedesktop/Notifications", "org.winlux.Notifications", method,
                parameters, None, Gio.DBusCallFlags.NONE, 3000, None)
        except GLib.Error as error: self.notice.set_text(str(error))

    def notifications(self):
        preference = Path(os.environ.get("XDG_CONFIG_HOME",HOME/".config")) / "winlux/do-not-disturb"
        switch = Gtk.CheckButton(label="Ne pas déranger : conserver l’historique sans afficher de bannière")
        switch.set_active(preference.exists())
        def toggle(widget):
            preference.parent.mkdir(parents=True, exist_ok=True)
            if widget.get_active(): preference.touch(mode=0o600)
            else: preference.unlink(missing_ok=True)
        switch.connect("toggled", toggle)
        self.content.pack_start(switch, False, False, 0)
        self.button("Effacer l’historique", lambda: self.notification_call("Clear"))
        self.notification_snapshot = read(DATA / "notifications.json")
        try: history = json.loads(self.notification_snapshot or "[]")
        except ValueError: history = []
        if not history: self.label("Aucune notification enregistrée.")
        for item in reversed(history[-100:]):
            self.label(f"{item['app']} · {time.strftime('%d/%m %H:%M', time.localtime(item['time']))}\n{item['title']}\n{item['body']}")
            actions = item.get("actions", [])
            for index in range(0, len(actions)-1, 2):
                self.button(actions[index+1], lambda identity=item["id"], action=actions[index]:
                    self.notification_call("Activate", GLib.Variant("(us)", (identity, action))))

    def tasks(self):
        self.summary = self.label("")
        self.store = Gtk.ListStore(int, str, float, float, str)
        self.tree = Gtk.TreeView(model=self.store)
        for index, title in enumerate(["PID", "Programme", "CPU % (un cœur)", "RAM Mio"]):
            column = Gtk.TreeViewColumn(title, Gtk.CellRendererText(), text=index)
            column.set_sort_column_id(index)
            column.set_resizable(True)
            self.tree.append_column(column)
        self.content.pack_start(self.tree, True, True, 0)
        self.button("Demander la fermeture", lambda: self.stop_task(signal.SIGTERM))
        self.button("Forcer l’arrêt du programme sélectionné", lambda: self.stop_task(signal.SIGKILL))
        self.refresh_tasks()

    def stop_task(self, sig):
        model, iterator = self.tree.get_selection().get_selected()
        if iterator is None: return
        pid, name, _, _, stamp = model[iterator]
        if pid in (1, os.getpid()):
            message(self, "Ce processus ne peut pas être fermé ici.")
            return
        dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
            text=f"Fermer {name} (PID {pid}) ?", secondary_text="Les modifications non enregistrées peuvent être perdues.")
        answer = dialog.run(); dialog.destroy()
        if answer != Gtk.ResponseType.OK: return
        try:
            stat = Path(f"/proc/{pid}/stat").read_text().rsplit(")",1)[1].split()
            if stat[19] != stamp: raise RuntimeError("Le processus a changé. Sélectionnez-le à nouveau.")
            os.kill(pid, sig)
        except (OSError, RuntimeError) as error: message(self, str(error))

    def refresh_tasks(self):
        now = time.monotonic(); elapsed = max(0.001, now-self.previous_time)
        model, selected = self.tree.get_selection().get_selected()
        selected_pid = model[selected][0] if selected else None
        self.store.clear()
        current = {}
        ticks = os.sysconf("SC_CLK_TCK"); pages = os.sysconf("SC_PAGE_SIZE")
        for path in Path("/proc").iterdir():
            if not path.name.isdigit(): continue
            try:
                text = (path/"stat").read_text(); fields = text.rsplit(")",1)[1].split()
                identity = (int(path.name), fields[19]); cpu = int(fields[11])+int(fields[12])
                percent = max(0, cpu-self.previous.get(identity,cpu))/ticks/elapsed*100
                current[identity] = cpu
                iterator = self.store.append([identity[0], text.split("(",1)[1].rsplit(")",1)[0], round(percent,1), round(int(fields[21])*pages/1048576,1), fields[19]])
                if identity[0] == selected_pid: self.tree.get_selection().select_iter(iterator)
            except (OSError, ValueError, IndexError): continue
        self.previous, self.previous_time = current, now
        memory = dict(line.split(":",1) for line in read("/proc/meminfo").splitlines() if ":" in line)
        self.summary.set_text(f"{len(current)} processus · RAM disponible : {memory.get('MemAvailable','inconnue').strip()}")

    def appearance(self):
        for theme in ("light", "dark"):
            self.button("Thème " + ("clair" if theme=="light" else "sombre"), lambda value=theme: self.command(["winlux-personalize","theme",value]))
        color = Gtk.ColorButton(title="Couleur d’accent")
        color.connect("color-set", lambda widget: self.command(["winlux-personalize","accent",
            "#%02x%02x%02x" % tuple(round(v*255) for v in (widget.get_rgba().red,widget.get_rgba().green,widget.get_rgba().blue))]))
        self.content.pack_start(color, False, False, 0)
        chooser = Gtk.FileChooserButton(title="Choisir un fond d’écran")
        chooser.connect("file-set", lambda widget: self.command(["winlux-personalize","wallpaper",widget.get_filename()]))
        self.content.pack_start(chooser, False, False, 0)
        icon = Gtk.FileChooserButton(title="Choisir l’icône du menu Démarrer")
        icon.connect("file-set", lambda widget: self.command(["winlux-personalize", "start-icon", widget.get_filename()]))
        self.label("Icône du menu Démarrer")
        self.content.pack_start(icon, False, False, 0)
        self.button("Rétablir l’icône Démarrer par défaut", lambda: self.command(["winlux-personalize", "start-icon", "default"]))
        for size in (32,40,48,56,64): self.button(f"Barre des tâches : {size} px", lambda value=size: self.command(["winlux-personalize","panel-size",str(value)]))
        self.button("Options avancées de la barre des tâches", lambda: self.command(["xfce4-panel", "--preferences"]), bool(shutil.which("xfce4-panel")))
        def reset_appearance():
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
                                       text="Rétablir l’apparence Winlux par défaut ?",
                                       secondary_text="Le thème, l’accent, le fond, l’icône Démarrer et la taille de la barre seront réinitialisés.")
            result = dialog.run(); dialog.destroy()
            if result == Gtk.ResponseType.OK: self.command(["winlux-personalize", "reset", "default"])
        self.button("Rétablir l’apparence par défaut", reset_appearance)

    def apps(self):
        if Path("/usr/share/winlux/apt-base").is_file():
            self.label("Applications Linux natives (APT)")
            native = self.entry("Nom du paquet Ubuntu, par exemple vlc")
            self.button("Actualiser le catalogue APT", lambda:self.command(["apt-get","update"]))
            self.button("Rechercher dans les paquets APT", lambda: self.command(["apt-cache", "search", native.get_text().strip()]) if native.get_text().strip() else self.notice.set_text("Saisissez une recherche."))
            self.button("Lister les paquets installés", lambda: self.command(["dpkg-query", "-W", "-f=${binary:Package} ${Version}\\n"]))
            def apt(action):
                name = native.get_text().strip()
                if not re.fullmatch(r"[a-z0-9][a-z0-9+.-]+",name):
                    message(self,"Entrez un nom de paquet valide.");return
                dialog=Gtk.MessageDialog(transient_for=self,modal=True,buttons=Gtk.ButtonsType.OK_CANCEL,
                    text=("Installer " if action=="install" else "Désinstaller ")+name+" ?")
                answer=dialog.run();dialog.destroy()
                if answer==Gtk.ResponseType.OK:
                    self.command(["env","DEBIAN_FRONTEND=noninteractive","apt-get","-y",
                                  "-o","Dpkg::Options::=--force-confold",action,name])
            self.button("Installer le paquet Linux",lambda:apt("install"))
            self.button("Désinstaller le paquet Linux",lambda:apt("remove"))
        self.label("Navigateurs intégrés et applications Flatpak installées pour votre profil.")
        self.button("Gérer les navigateurs", lambda: self.command(["winlux-shell-app","settings","11"]))
        identity = self.entry("Identifiant Flatpak, par exemple org.videolan.VLC")
        def flatpak(action):
            app = identity.get_text().strip()
            if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]*(?:\.[A-Za-z0-9_-]+){2,}",app):
                message(self,"Entrez un identifiant d’application valide."); return
            args = ["flatpak",action,"--user","--noninteractive"]
            if action=="install": args += ["flathub",app]
            else: args += [app]
            self.command(args)
        self.button("Activer le catalogue Flathub", lambda: self.command(["flatpak","remote-add","--user","--if-not-exists","flathub","https://dl.flathub.org/repo/flathub.flatpakrepo"]),bool(shutil.which("flatpak")))
        self.button("Rechercher une application Flatpak", lambda: self.command(["flatpak", "search", identity.get_text().strip()]) if identity.get_text().strip() else self.notice.set_text("Saisissez une recherche."), bool(shutil.which("flatpak")))
        self.button("Installer", lambda: flatpak("install"),bool(shutil.which("flatpak")))
        self.button("Désinstaller", lambda: flatpak("uninstall"),bool(shutil.which("flatpak")))
        self.button("Lister les applications Flatpak", lambda: self.command(["flatpak","list","--user"]),bool(shutil.which("flatpak")))
        def update_flatpak():
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
                                       text="Mettre à jour les applications Flatpak de ce profil ?")
            response = dialog.run(); dialog.destroy()
            if response == Gtk.ResponseType.OK:
                self.command(["flatpak", "update", "--user", "--noninteractive"])
        self.button("Mettre à jour les applications Flatpak", update_flatpak, bool(shutil.which("flatpak")))
        self.label("Applications par défaut")
        types=Gtk.ComboBoxText()
        for mime,label in [("text/plain","Documents texte"),("image/png","Images PNG"),("image/jpeg","Images JPEG"),
                           ("application/pdf","Documents PDF"),("text/html","Pages HTML"),("inode/directory","Dossiers")]:types.append(mime,label)
        holder=Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.content.pack_start(types,False,False,0);self.content.pack_start(holder,False,False,0)
        def choose_type(*_):
            for child in holder.get_children():child.destroy()
            self.default=Gtk.AppChooserButton(content_type=types.get_active_id())
            self.default.set_show_dialog_item(True);holder.pack_start(self.default,False,False,0);holder.show_all()
        types.connect("changed",choose_type);types.set_active(0)
        self.button("Définir l’application par défaut",lambda:self.default.get_app_info().set_as_default_for_type(types.get_active_id()) if self.default.get_app_info() else None)
        self.label("Programmes lancés à la connexion")
        choices = Gtk.ComboBoxText()
        self.applications = {app.get_id():app for app in Gio.AppInfo.get_all() if app.should_show() and app.get_id() and hasattr(app,"get_filename")}
        for identifier, app in sorted(self.applications.items(),key=lambda item:item[1].get_display_name()): choices.append(identifier,app.get_display_name())
        choices.set_active(0); self.content.pack_start(choices,False,False,0)
        def startup(enabled):
            identifier = choices.get_active_id()
            if not identifier: return
            app=self.applications[identifier]
            key=GLib.KeyFile(); key.load_from_file(app.get_filename(),GLib.KeyFileFlags.NONE)
            key.set_boolean("Desktop Entry","Hidden",not enabled)
            destination=HOME/".config/autostart"/Path(identifier).name
            destination.parent.mkdir(parents=True,exist_ok=True)
            destination.write_text(key.to_data()[0]); self.notice.set_text("Préférence de démarrage enregistrée.")
        self.button("Activer au démarrage",lambda:startup(True))
        self.button("Désactiver au démarrage",lambda:startup(False))

    def files(self):
        self.label("L’Explorateur natif propose les favoris dans sa barre latérale, les miniatures et les périphériques amovibles.")
        self.button("Ouvrir le gestionnaire de fichiers complet", lambda: self.command(["Thunar"]), bool(shutil.which("Thunar")))
        for label,path in [("Dossier personnel",HOME),("Documents",HOME/"Documents"),("Téléchargements",HOME/"Téléchargements")]:
            self.button(label,lambda target=path:self.command(["Thunar",str(target)]))
        self.button("Corbeille",lambda:self.command(["Thunar","trash:///"]))
        self.button("Ajouter un dossier aux favoris",self.add_bookmark)
        self.button("Aperçu d’un fichier",self.preview)
        self.label("Favoris : ouvrez un dossier puis utilisez Ctrl+D dans l’Explorateur natif. Éjection : bouton à côté du périphérique dans la barre latérale.")
        self.button("Lister les périphériques USB",lambda:self.command(["lsblk","-o","NAME,SIZE,TRAN,MOUNTPOINTS,MODEL"]))
        self.button("Gestion des disques et éjection",lambda:self.command(["gnome-disks"]),bool(shutil.which("gnome-disks")))

    def recovery(self):
        self.label("En cas de problème, une sauvegarde Winlux peut permettre de revenir à la version précédente. Les documents personnels ne sont pas une sauvegarde système : gardez-en également une copie sur un autre support.")
        if Path("/usr/share/winlux/update-backend").is_file() and read("/usr/share/winlux/update-backend").strip() == "apt":
            available = Path("/var/cache/winlux/previous.deb").is_file()
        else:
            name = read("/var/lib/winlux/rollback-current").strip()
            available = bool(re.fullmatch(r"update-backup\.[A-Za-z0-9._+-]+", name)) and (Path("/var/lib/winlux") / name / "restore.list").is_file()
        self.label("Sauvegarde précédente détectée." if available else "Aucune sauvegarde précédente détectée.")
        self.label("Pour restaurer, ouvrez Mises à jour et choisissez Restaurer. Une confirmation est demandée avant toute modification.")
        self.button("Ouvrir les mises à jour", lambda: self.command(["winlux-shell-app", "settings", "7"]))
        self.label("Si le bureau ne démarre plus : utilisez l’entrée Winlux secours au démarrage si elle est disponible. Sinon, démarrez sur une clé USB d’installation Winlux. Le menu de secours ne remplace pas une sauvegarde de vos documents.")
        self.button("Ouvrir le guide d’accueil", lambda: self.command(["winlux-welcome"]))

    def pick(self,folder=False):
        dialog=Gtk.FileChooserDialog(title="Choisir un dossier" if folder else "Choisir un fichier",transient_for=self,
            action=Gtk.FileChooserAction.SELECT_FOLDER if folder else Gtk.FileChooserAction.OPEN)
        dialog.add_buttons("Annuler",Gtk.ResponseType.CANCEL,"Choisir",Gtk.ResponseType.OK)
        answer=dialog.run();path=dialog.get_filename() if answer==Gtk.ResponseType.OK else None;dialog.destroy()
        return Path(path) if path else None

    def add_bookmark(self):
        folder=self.pick(True)
        if not folder:return
        bookmark=HOME/".config/gtk-3.0/bookmarks";bookmark.parent.mkdir(parents=True,exist_ok=True)
        lines=read(bookmark).splitlines();uri=folder.resolve().as_uri()
        if not any(line.split(" ",1)[0]==uri for line in lines):lines.append(uri)
        bookmark.write_text("\n".join(lines)+"\n");self.notice.set_text("Dossier ajouté aux favoris de l’Explorateur natif.")

    def preview(self):
        path=self.pick()
        if not path:return
        dialog=Gtk.Dialog(title=path.name,transient_for=self,modal=True)
        dialog.set_default_size(640,500);dialog.add_button("Fermer",Gtk.ResponseType.CLOSE)
        area=dialog.get_content_area()
        try:
            info=GdkPixbuf.Pixbuf.get_file_info(str(path))
            if info and info[0] and path.stat().st_size<20*1024*1024 and info[1]*info[2]<40000000:
                area.add(Gtk.Image.new_from_pixbuf(GdkPixbuf.Pixbuf.new_from_file_at_scale(str(path),600,440,True)))
            else:
                with path.open("rb") as stream:data=stream.read(65536)
                text="Aperçu indisponible pour ce format." if b"\0" in data else data.decode("utf-8",errors="replace")
                view=Gtk.TextView(editable=False,wrap_mode=Gtk.WrapMode.WORD_CHAR)
                view.get_buffer().set_text(text)
                scroll=Gtk.ScrolledWindow();scroll.add(view);area.pack_start(scroll,True,True,0)
                if len(data)==65536:area.add(Gtk.Label(label="Aperçu limité aux premiers 64 Kio."))
        except (OSError,GLib.Error) as error:area.add(Gtk.Label(label=str(error),wrap=True))
        dialog.show_all();dialog.run();dialog.destroy()

def split_nmcli(line):
    fields, field, escaped = [], [], False
    for char in line:
        if escaped: field.append(char); escaped = False
        elif char == "\\": escaped = True
        elif char == ":": fields.append("".join(field)); field = []
        else: field.append(char)
    if escaped: field.append("\\")
    fields.append("".join(field))
    return fields

if __name__ == "__main__":
    page = sys.argv[1] if len(sys.argv)>1 else "quick"
    if page not in ("quick","notifications","tasks","appearance","apps","files","recovery"): raise SystemExit(2)
    Tools(page)
    Gtk.main()

#!/usr/bin/python3
"""Native settings: nonblocking control actions and live Linux status."""
import os
from pathlib import Path
import subprocess
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, GLib

PAGES = ["Système", "Applications", "Réseau", "Son et alimentation", "Clavier",
         "Souris", "Personnalisation", "Mises à jour", "Écran", "À propos",
         "Programmes Windows", "Navigateurs", "Accessibilité", "Effets visuels", "Graphismes"]
GRAPHICS_PAGE = 14
FIELDS = {
    0: [("Version", "version"), ("Noyau", "kernel")],
    2: [("Connexion", "network")], 3: [("Volume", "audio"), ("Alimentation", "battery")],
    4: [("Disposition", "layout"), ("Toucher-clic", "tap")],
    5: [("Vitesse", "mouse_speed")], 6: [("Alignement", "align")],
    8: [("Définition", "display_current"), ("Échelle", "display_scale")],
    9: [("Version", "version"), ("Noyau", "kernel")],
    11: [("Navigateur par défaut", "browser_default")],
}
ACTIONS = {
    0: [("Relancer le bureau", "restart-session"), ("Redémarrer", "reboot"), ("Éteindre", "poweroff")],
    2: [("Renouveler la connexion", "network-renew")],
    3: [("Volume −", "volume-down"), ("Muet / rétablir", "volume-mute"), ("Volume +", "volume-up")],
    4: [("Français AZERTY", "keyboard-fr"), ("Anglais QWERTY", "keyboard-us"), ("Activer toucher-clic", "tap-on"), ("Désactiver toucher-clic", "tap-off")],
    5: [("Lente", "mouse-slow"), ("Normale", "mouse-normal"), ("Rapide", "mouse-fast")],
    6: [("À gauche", "align-left"), ("Centrer", "align-center"),
        ("Ajouter Corbeille", "desktop-icon-recycle"), ("Ajouter Explorateur", "desktop-icon-explorer"),
        ("Ajouter Ce PC", "desktop-icon-computer"), ("Ajouter Documents", "desktop-icon-documents")],
    7: [("Rechercher", "update-check"), ("Installer", "update-apply"), ("Restaurer", "update-rollback"),
        ("Mises à jour auto : activer", "update-auto-on"), ("Désactiver", "update-auto-off")],
    8: [("Suivre la taille de la VM", "display-follow"), ("Définition fixe", "display-fixed"),
        ("Échelle −", "display-scale-down"), ("Automatique", "display-scale-auto"), ("Échelle +", "display-scale-up")],
}

def gpu_summary():
    """Résumé lisible du rapport GPU : seules les mesures exposées par le pilote."""
    try:
        out = subprocess.run(["winlux", "--gpu-status"], capture_output=True, text=True, timeout=3).stdout
    except (OSError, subprocess.SubprocessError):
        return "Informations graphiques indisponibles (runtime Winlux introuvable)."
    report = dict(line.split("=", 1) for line in out.splitlines() if "=" in line)
    try: count = min(8, int(report.get("gpu_count", "0")))
    except ValueError: count = 0
    if not count:
        return "Aucune carte graphique détectée (machine virtuelle ou affichage de secours)."
    roles = {"integrated": "GPU intégré (économie d'énergie)", "dedicated": "GPU dédié (hautes performances)"}
    lines = []
    for i in range(count):
        get = lambda key: report.get(f"gpu{i}_{key}")
        lines.append(get("name") or "Carte graphique")
        kind = [roles[get("role")]] if get("role") in roles else []
        lines.append("   " + " · ".join(kind + [f"Pilote {get('driver') or 'inconnu'}"]))
        metrics = []
        if get("load"): metrics.append(f"Charge {get('load')} %")
        if get("vram_used_mb") and get("vram_total_mb"): metrics.append(f"VRAM {get('vram_used_mb')} / {get('vram_total_mb')} Mo")
        if get("temp_c"): metrics.append(f"{get('temp_c')} °C")
        if get("freq_mhz"): metrics.append(f"Fréquence {get('freq_mhz')} MHz")
        if get("power_w"): metrics.append(f"{get('power_w')} W")
        if get("encoder"): metrics.append(f"Encodeur {get('encoder')} %")
        if get("decoder"): metrics.append(f"Décodeur {get('decoder')} %")
        if metrics: lines.append("   " + " · ".join(metrics))
    if report.get("gpu_hybrid") == "1":
        lines.append("\nGPU par application : clic droit sur un .exe > Réglages de l'exécutable.")
    return "\n".join(lines)

def read(name):
    try: return Path(name).read_text(errors="replace")
    except OSError: return ""

class Settings(Gtk.Window):
    def __init__(self):
        super().__init__(title="Parametres Winlux")
        self.set_icon_name("preferences-system")
        self.set_default_size(1020, 740)
        self.set_size_request(720, 500)
        self.connect("destroy", Gtk.main_quit)
        try: requested = int(os.environ.get("WINLUX_SETTINGS_PAGE", "0"))
        except ValueError: requested = 0
        # Le bureau numérote « Graphismes » 23 ; ici elle suit « Effets visuels ».
        self.page = GRAPHICS_PAGE if requested == 23 else min(13, max(0, requested))
        self.process = None
        self.active_action = None
        css = Gtk.CssProvider()
        css.load_from_data(b"""
          window { background: #181b24; color: #ecedf5; }
          #sidebar { background: #202430; padding: 16px; }
          #sidebar list, #sidebar viewport { background: #202430; color: #ecedf5; }
          #sidebar row { padding: 10px; border-radius: 6px; }
          #sidebar row:hover { background: #30374a; }
          #sidebar row:selected { background: #423d66; }
          entry { background: #171b25; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 8px; }
          button { background: #30384b; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 10px 14px; box-shadow: none; }
          button:hover { background: #414b64; }
          button:disabled { color: #858da0; background: #252a36; }
          .heading { font-size: 24px; font-weight: bold; }
          .subtitle { color: #adb6c9; }
          .card { background: #242a38; border-radius: 8px; padding: 16px; }
        """)
        Gtk.StyleContext.add_provider_for_screen(self.get_screen(), css, 600)
        personal = Path.home() / ".config/gtk-3.0/gtk.css"
        if personal.is_file():
            override = Gtk.CssProvider()
            override.load_from_path(str(personal))
            Gtk.StyleContext.add_provider_for_screen(self.get_screen(), override, 800)
        outer = Gtk.Box()
        self.add(outer)
        side = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        side.set_name("sidebar")
        side.set_size_request(240, -1)
        outer.pack_start(side, False, False, 0)
        side.pack_start(Gtk.Label(label="WINLUX · PARAMÈTRES", xalign=0), False, False, 0)
        search = Gtk.SearchEntry(placeholder_text="Rechercher un réglage")
        side.pack_start(search, False, False, 0)
        nav_scroll = Gtk.ScrolledWindow()
        side.pack_start(nav_scroll, True, True, 0)
        self.nav = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
        nav_scroll.add(self.nav)
        for title in PAGES: self.nav.add(Gtk.Label(label=title, xalign=0))
        self.nav.set_filter_func(lambda row: search.get_text().casefold() in PAGES[row.get_index()].casefold())
        search.connect("search-changed", lambda *_: self.nav.invalidate_filter())
        self.nav.connect("row-selected", self.select)
        scroll = Gtk.ScrolledWindow()
        outer.pack_start(scroll, True, True, 0)
        self.body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=18, margin=28)
        scroll.add(self.body)
        self.nav.select_row(self.nav.get_row_at_index(self.page))
        GLib.timeout_add(1000, self.poll)
        self.show_all()
        self.progress.set_visible(self.page == 7)

    def label(self, text, style=None):
        label = Gtk.Label(label=text, xalign=0, wrap=True, selectable=True)
        if style: label.get_style_context().add_class(style)
        self.body.pack_start(label, False, False, 0)
        return label

    def select(self, _, row):
        if row is None: return
        self.page = row.get_index()
        for child in self.body.get_children(): child.destroy()
        self.label(PAGES[self.page], "heading")
        self.label("Vos préférences et l’état réel de votre ordinateur.", "subtitle")
        self.state = self.label("Chargement…", "card")
        actions = list(ACTIONS.get(self.page, []))
        if self.page == 11:
            actions = [(f"{verb} {name}", f"browser-{action}-{name}") for name in ("edge", "chrome", "firefox")
                       for verb, action in (("Installer", "install"), ("Par défaut :", "default"), ("Ouvrir", "launch"))]
        flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=3,
                           row_spacing=8, column_spacing=8)
        self.body.pack_start(flow, False, False, 0)
        self.install_button = None
        for title, action in actions:
            button = Gtk.Button(label=title)
            button.connect("clicked", lambda _, value=action: self.execute(value))
            flow.add(button)
            if action == "update-apply": self.install_button = button
        self.buttons = flow
        if self.page in (1, 2, 3, 6):
            tool = {1:"apps", 2:"quick", 3:"quick", 6:"appearance"}[self.page]
            shortcut = Gtk.Button(label={1:"Installer, désinstaller et gérer le démarrage",2:"Wi-Fi et Bluetooth",3:"Réglages rapides",6:"Thèmes, accent, fond et barre des tâches"}[self.page])
            shortcut.connect("clicked", lambda *_: subprocess.Popen(["winlux-system-tools", tool]))
            self.body.pack_start(shortcut, False, False, 0)
        if self.page == 8:
            self.modes = Gtk.ComboBoxText()
            status = dict(line.split("=", 1) for line in read("/run/winlux/control.txt").splitlines() if "=" in line)
            for mode in status.get("display_modes", "").split(","):
                if mode: self.modes.append_text(mode)
            self.modes.set_active(0)
            self.body.pack_start(self.modes, False, False, 0)
            button = Gtk.Button(label="Appliquer la définition")
            button.connect("clicked", lambda *_: self.execute("display-mode-" + self.modes.get_active_text()) if self.modes.get_active_text() else None)
            self.body.pack_start(button, False, False, 0)
        if self.page in (1, 10, 12, 13, GRAPHICS_PAGE):
            self.label("Ouvrir les réglages détaillés de cette catégorie :")
            button = Gtk.Button(label="Réglages avancés")
            legacy_page = 23 if self.page == GRAPHICS_PAGE else self.page
            button.connect("clicked", lambda *_: subprocess.Popen(["winlux-shell-app", "settings", str(legacy_page)], env=dict(os.environ, WINLUX_LEGACY_SETTINGS="1")))
            self.body.pack_start(button, False, False, 0)
        self.notice = self.label("")
        self.progress = Gtk.ProgressBar(show_text=True)
        self.body.pack_start(self.progress, False, False, 0)
        self.details = self.label("")
        if self.page == 7:
            self.label("La préparation des mises à jour utilise le disque. Gardez la VM allumée pendant l’installation.", "subtitle")
            recovery = Gtk.Button(label="Secours et restauration")
            recovery.connect("clicked", lambda *_: subprocess.Popen(["winlux-system-tools", "recovery"]))
            self.body.pack_start(recovery, False, False, 0)
            restart = Gtk.Button(label="Redémarrer après installation")
            restart.connect("clicked", lambda *_: self.execute("reboot"))
            self.body.pack_start(restart, False, False, 0)
        self.poll()
        self.body.show_all()
        self.progress.set_visible(self.page == 7)
        if self.install_button is not None:
            self.install_button.get_parent().set_visible("Winlux est à jour." not in read("/run/winlux/updates.txt"))

    def execute(self, action):
        if self.process is not None or self.busy: return
        if action in ("reboot", "poweroff", "restart-session", "update-rollback"):
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
                                       text="Confirmer cette opération ?", secondary_text="Enregistrez vos documents avant de continuer.")
            answer = dialog.run()
            dialog.destroy()
            if answer != Gtk.ResponseType.OK: return
        try:
            self.process = subprocess.Popen(["winlux-control", action], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError as error:
            self.show_error("Impossible de lancer l’opération", str(error))
            return
        self.active_action = action
        self.notice.set_text("Opération en cours…")
        self.buttons.set_sensitive(False)

    def poll(self):
        status = dict(line.split("=", 1) for line in read("/run/winlux/control.txt").splitlines() if "=" in line)
        self.busy = status.get("update_busy") == "1"
        if self.process is not None and self.process.poll() is not None:
            result = self.process.returncode
            action = self.active_action
            self.process = None
            self.active_action = None
            if result and action in ("update-check", "update-apply", "update-rollback"):
                self.show_update_error(action, result)
            elif result:
                detail = read("/run/winlux/action.log").strip()[-3000:] or "Aucun détail n’a été écrit dans le journal."
                self.show_error("L’opération a échoué", f"Code : {result}\n\n{detail}")
        self.buttons.set_sensitive(self.process is None and not self.busy)
        fields = FIELDS.get(self.page, [])
        text = "\n".join(f"{label} : {status.get(key) or 'Indisponible'}" for label, key in fields)
        if self.page == 7: text = read("/run/winlux/updates.txt")
        if self.install_button is not None:
            self.install_button.get_parent().set_visible("Winlux est à jour." not in text)
        if self.page == 9: text += "\n" + read("/usr/share/winlux/BUILD-INFO")
        if self.page == GRAPHICS_PAGE: text = gpu_summary()
        if self.state.get_text() != text: self.state.set_text(text)
        if self.process is None: self.notice.set_text(read("/run/winlux/action.txt").strip())
        progress = dict(line.split("=", 1) for line in read("/run/winlux/update-progress").splitlines() if "=" in line)
        try: percent = int(progress.get("percent", "-1"))
        except ValueError: percent = -1
        try: fraction = min(1, max(0, percent / 100))
        except ValueError: fraction = 0
        if percent < 0: self.progress.pulse()
        else: self.progress.set_fraction(fraction)
        item = progress.get("item", "")
        if progress.get("unit") == "bytes":
            try:
                done, total = int(progress.get("done", "0")), int(progress.get("total", "0"))
                amount = f"{done / 1048576:.1f} Mio" + (f" / {total / 1048576:.1f} Mio" if total else "")
                item = f"{item} · {amount}" + (f" · {percent} %" if percent >= 0 else "")
            except ValueError: pass
        elif progress.get("phase") in ("backing-up", "installing"):
            try:
                done, total = int(progress.get("done", "0")), int(progress.get("total", "0"))
                if total: item = f"{progress.get('phase') == 'installing' and 'Installation' or 'Sauvegarde'} · {done}/{total} fichiers · {done * 100 // total} %\n{item}"
            except ValueError: pass
        self.progress.set_text(item)
        self.progress.set_visible(self.page == 7)
        if self.page == 7:
            journal = read("/run/winlux/update.log")[-12000:]
            if self.details.get_text() != journal: self.details.set_text(journal)
        return True

    def show_update_error(self, action, result):
        stages = {"checking": "Recherche et préparation", "downloading": "Téléchargement", "verifying": "Vérification",
                  "extracting": "Décompression", "backing-up": "Sauvegarde",
                  "installing": "Installation", "finalizing": "Finalisation", "restoring": "Restauration"}
        phase = read("/run/winlux/update-error-phase").strip()
        stage = stages.get(phase, phase or {"update-check": "Recherche", "update-apply": "Mise à jour",
                                           "update-rollback": "Restauration"}[action])
        log = read("/run/winlux/update.log").strip()
        detail = log[-3000:] if log else "Aucun détail n’a été écrit dans le journal."
        self.show_error("La mise à jour a échoué", f"Étape : {stage} (code {result})\n\nDernières lignes du journal :\n{detail}")

    def show_error(self, title, detail):
        dialog = Gtk.MessageDialog(transient_for=self, modal=False, message_type=Gtk.MessageType.ERROR,
                                   buttons=Gtk.ButtonsType.CLOSE, text=title)
        dialog.format_secondary_text(detail)
        for child in dialog.get_message_area().get_children():
            if isinstance(child, Gtk.Label): child.set_selectable(True)
        dialog.connect("response", lambda window, _: window.destroy())
        dialog.show_all()

if __name__ == "__main__":
    Settings()
    Gtk.main()

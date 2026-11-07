#!/usr/bin/python3
"""Optional, non-destructive first-session guide."""
import os
from pathlib import Path
import subprocess
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk


# ── Langue de l'interface (Paramètres > Langue) : fr, en, es ou de ──────────

def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}

# Texte français -> (anglais, espagnol, allemand).
_TR = {
    'Bienvenue dans Winlux': ('Welcome to Winlux', 'Bienvenido a Winlux', 'Willkommen bei Winlux'),
    'Préparez votre ordinateur à votre rythme. Ces réglages restent accessibles à tout moment dans Paramètres.': ('Set up your computer at your own pace. These settings remain available at any time in Settings.', 'Prepare su equipo a su ritmo. Estos ajustes siguen disponibles en cualquier momento en Configuración.', 'Richten Sie Ihren Computer in Ihrem Tempo ein. Diese Einstellungen bleiben jederzeit in den Einstellungen verfügbar.'),
    'Visite guidée et logiciels courants': ('Guided tour and common apps', 'Recorrido guiado y aplicaciones habituales', 'Geführte Tour und gängige Programme'),
    'Choisir le clavier': ('Choose the keyboard', 'Elegir el teclado', 'Tastatur auswählen'),
    'Choisir et installer un navigateur': ('Choose and install a browser', 'Elegir e instalar un navegador', 'Browser auswählen und installieren'),
    'Choisir des boutiques de logiciels': ('Choose software stores', 'Elegir tiendas de programas', 'Software-Stores auswählen'),
    'Configurer le réseau': ('Set up the network', 'Configurar la red', 'Netzwerk einrichten'),
    'Personnaliser l’apparence': ('Personalize the appearance', 'Personalizar la apariencia', 'Darstellung anpassen'),
    'Découvrir le secours': ('Discover recovery', 'Descubrir la recuperación', 'Wiederherstellung entdecken'),
    'Astuce : Windows + E ouvre l’Explorateur, Windows + A les réglages rapides et Windows + N les notifications.': ('Tip: Windows + E opens File Explorer, Windows + A opens quick settings and Windows + N opens notifications.', 'Consejo: Windows + E abre el Explorador, Windows + A la configuración rápida y Windows + N las notificaciones.', 'Tipp: Windows + E öffnet den Explorer, Windows + A die Schnelleinstellungen und Windows + N die Benachrichtigungen.'),
    'Terminer': ('Finish', 'Finalizar', 'Fertigstellen'),
    'Projet indépendant, ni affilié à Microsoft ni approuvé par elle. Windows est une marque de Microsoft Corporation.': ('Independent project, neither affiliated with nor endorsed by Microsoft. Windows is a trademark of Microsoft Corporation.', 'Proyecto independiente, no afiliado a Microsoft ni aprobado por ella. Windows es una marca de Microsoft Corporation.', 'Unabhängiges Projekt, weder mit Microsoft verbunden noch von Microsoft unterstützt. Windows ist eine Marke der Microsoft Corporation.'),
    'Impossible d’ouvrir ce réglage': ('Unable to open this setting', 'No se puede abrir este ajuste', 'Diese Einstellung kann nicht geöffnet werden'),
    'Impossible d’enregistrer cette préférence': ('Unable to save this preference', 'No se puede guardar esta preferencia', 'Diese Einstellung kann nicht gespeichert werden'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


MARKER = Path.home() / ".config/winlux/welcome-complete"

class Welcome(Gtk.Window):
    def __init__(self):
        super().__init__(title=_t("Bienvenue dans Winlux"))
        self.set_icon_name("winlux-welcome")
        self.set_default_size(620, 460)
        self.connect("destroy", Gtk.main_quit)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14, margin=24)
        self.add(box)
        title = Gtk.Label(label=_t("Bienvenue dans Winlux"), xalign=0)
        title.get_style_context().add_class("title")
        box.pack_start(title, False, False, 0)
        intro = Gtk.Label(label=_t("Préparez votre ordinateur à votre rythme. Ces réglages restent accessibles à tout moment dans Paramètres."), xalign=0, wrap=True)
        box.pack_start(intro, False, False, 0)
        for label, command in (
            (_t("Visite guidée et logiciels courants"), ["winlux-shell-app", "welcome"]),
            (_t("Choisir le clavier"), ["winlux-shell-app", "settings", "4"]),
            # Aucun navigateur n'est fourni : l'utilisateur choisit le sien.
            (_t("Choisir et installer un navigateur"), ["winlux-shell-app", "settings", "11"]),
            # Aucune boutique tierce non plus : Paramètres > Applications.
            (_t("Choisir des boutiques de logiciels"), ["winlux-shell-app", "settings", "1"]),
            # Page Wi-Fi (ou Réseau sans carte Wi-Fi) de Paramètres.
            (_t("Configurer le réseau"), ["winlux-shell-app", "settings", "35"]),
            (_t("Personnaliser l’apparence"), ["winlux-shell-app", "settings", "21"]),
            (_t("Découvrir le secours"), ["winlux-shell-app", "settings", "7"]),
        ):
            button = Gtk.Button(label=label)
            button.connect("clicked", lambda _, args=command: self.launch(args))
            box.pack_start(button, False, False, 0)
        hint = Gtk.Label(label=_t("Astuce : Windows + E ouvre l’Explorateur, Windows + A les réglages rapides et Windows + N les notifications."), xalign=0, wrap=True)
        box.pack_start(hint, True, True, 0)
        finish = Gtk.Button(label=_t("Terminer"))
        finish.connect("clicked", self.finish)
        box.pack_end(finish, False, False, 0)
        # Mention de non-affiliation (docs/DEFENSE-DU-NOM.md).
        notice = Gtk.Label(label=_t("Projet indépendant, ni affilié à Microsoft ni approuvé par elle. Windows est une marque de Microsoft Corporation."), xalign=0, wrap=True)
        notice.get_style_context().add_class("dim-label")
        box.pack_end(notice, False, False, 0)

    def launch(self, command):
        try: subprocess.Popen(command)
        except OSError as error:
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, message_type=Gtk.MessageType.ERROR,
                                       buttons=Gtk.ButtonsType.CLOSE, text=_t("Impossible d’ouvrir ce réglage"))
            dialog.format_secondary_text(str(error)); dialog.run(); dialog.destroy()

    def finish(self, *_):
        try:
            MARKER.parent.mkdir(parents=True, exist_ok=True)
            temporary = MARKER.with_name(MARKER.name + "." + str(os.getpid()))
            temporary.write_text("1\n")
            temporary.replace(MARKER)
        except OSError as error:
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, message_type=Gtk.MessageType.ERROR,
                                       buttons=Gtk.ButtonsType.CLOSE, text=_t("Impossible d’enregistrer cette préférence"))
            dialog.format_secondary_text(str(error)); dialog.run(); dialog.destroy()
            return
        self.destroy()

if __name__ == "__main__":
    Welcome().show_all()
    Gtk.main()

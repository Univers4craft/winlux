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
    "{count} réseau(x) trouvé(s).": ("{count} network(s) found.", "{count} red(es) encontrada(s).", "{count} Netzwerk(e) gefunden."),
    "Fermer {name} (PID {pid}) ?": ("Close {name} (PID {pid})?", "¿Cerrar {name} (PID {pid})?", "{name} (PID {pid}) schließen?"),
    "{n} processus · RAM disponible : {ram}": ("{n} processes · Available RAM: {ram}", "{n} procesos · RAM disponible: {ram}", "{n} Prozesse · Verfügbarer RAM: {ram}"),
    "inconnue": ("unknown", "desconocida", "unbekannt"),
    "Barre des tâches : {size} px": ("Taskbar: {size} px", "Barra de tareas: {size} px", "Taskleiste: {size} px"),
    'Outils Winlux': ('Winlux tools', 'Herramientas de Winlux', 'Winlux-Tools'),
    'Réglages rapides': ('Quick settings', 'Configuración rápida', 'Schnelleinstellungen'),
    'Notifications': ('Notifications', 'Notificaciones', 'Benachrichtigungen'),
    'Gestionnaire de tâches': ('Task Manager', 'Administrador de tareas', 'Task-Manager'),
    'Personnalisation': ('Personalization', 'Personalización', 'Personalisierung'),
    'Applications': ('Apps', 'Aplicaciones', 'Apps'),
    'Fichiers et périphériques': ('Files and devices', 'Archivos y dispositivos', 'Dateien und Geräte'),
    'Secours et restauration': ('Recovery and restore', 'Recuperación y restauración', 'Wiederherstellung'),
    'Opération en cours…': ('Working…', 'Operación en curso…', 'Vorgang läuft…'),
    'Opération terminée.': ('Done.', 'Operación terminada.', 'Vorgang abgeschlossen.'),
    'Échec de l’opération.': ('The operation failed.', 'La operación ha fallado.', 'Der Vorgang ist fehlgeschlagen.'),
    'Ethernet est prioritaire quand il est disponible. Le Wi-Fi peut rester configuré et prendre le relais lorsque le câble est débranché. Dans une VM, la connexion apparaît généralement comme Ethernet.': ('Ethernet takes priority when available. Wi-Fi can stay configured and take over when the cable is unplugged. In a VM, the connection usually shows up as Ethernet.', 'Ethernet tiene prioridad cuando está disponible. El Wi-Fi puede seguir configurado y tomar el relevo al desconectar el cable. En una máquina virtual, la conexión suele aparecer como Ethernet.', 'Ethernet hat Vorrang, wenn verfügbar. WLAN kann eingerichtet bleiben und übernimmt, wenn das Kabel getrennt wird. In einer VM erscheint die Verbindung meist als Ethernet.'),
    'Recherche des réseaux Wi-Fi…': ('Looking for Wi-Fi networks…', 'Buscando redes Wi-Fi…', 'WLAN-Netzwerke werden gesucht…'),
    'Ouvert': ('Open', 'Abierta', 'Offen'),
    'Aucun réseau visible ou aucune carte Wi-Fi disponible.': ('No visible network or no Wi-Fi adapter available.', 'No hay redes visibles o ninguna tarjeta Wi-Fi disponible.', 'Kein sichtbares Netzwerk oder kein WLAN-Adapter verfügbar.'),
    'Recherche Wi-Fi impossible : ': ('Wi-Fi scan failed: ', 'No se pudo buscar redes Wi-Fi: ', 'WLAN-Suche fehlgeschlagen: '),
    'Rechercher les réseaux Wi-Fi': ('Search for Wi-Fi networks', 'Buscar redes Wi-Fi', 'WLAN-Netzwerke suchen'),
    'Se connecter au réseau sélectionné': ('Connect to the selected network', 'Conectarse a la red seleccionada', 'Mit dem ausgewählten Netzwerk verbinden'),
    'Choisissez d’abord un réseau.': ('Choose a network first.', 'Elija primero una red.', 'Wählen Sie zuerst ein Netzwerk.'),
    'Réseaux Wi-Fi disponibles': ('Available Wi-Fi networks', 'Redes Wi-Fi disponibles', 'Verfügbare WLAN-Netzwerke'),
    'Actualiser la recherche Wi-Fi': ('Refresh Wi-Fi search', 'Actualizar la búsqueda Wi-Fi', 'WLAN-Suche aktualisieren'),
    'État des cartes Ethernet et Wi-Fi': ('Ethernet and Wi-Fi adapter status', 'Estado de las tarjetas Ethernet y Wi-Fi', 'Status der Ethernet- und WLAN-Adapter'),
    'Wi-Fi : activer': ('Wi-Fi: turn on', 'Wi-Fi: activar', 'WLAN: einschalten'),
    'Wi-Fi : désactiver': ('Wi-Fi: turn off', 'Wi-Fi: desactivar', 'WLAN: ausschalten'),
    'Configurer les réseaux': ('Configure networks', 'Configurar las redes', 'Netzwerke konfigurieren'),
    'Bluetooth : activer': ('Bluetooth: turn on', 'Bluetooth: activar', 'Bluetooth: einschalten'),
    'Bluetooth : désactiver': ('Bluetooth: turn off', 'Bluetooth: desactivar', 'Bluetooth: ausschalten'),
    'Associer un périphérique Bluetooth': ('Pair a Bluetooth device', 'Emparejar un dispositivo Bluetooth', 'Bluetooth-Gerät koppeln'),
    'Luminosité −': ('Brightness −', 'Brillo −', 'Helligkeit −'),
    'Luminosité +': ('Brightness +', 'Brillo +', 'Helligkeit +'),
    'Muet / rétablir': ('Mute / unmute', 'Silenciar / reactivar', 'Stumm / Ton ein'),
    'Alimentation': ('Power', 'Energía', 'Energie'),
    'Nom du réseau Wi-Fi à rejoindre': ('Name of the Wi-Fi network to join', 'Nombre de la red Wi-Fi a la que unirse', 'Name des WLAN-Netzwerks'),
    'Se connecter au réseau': ('Connect to the network', 'Conectarse a la red', 'Mit dem Netzwerk verbinden'),
    'Pour un réseau protégé, l’agent réseau demande le mot de passe. Si le point d’accès et l’agent proposent WPS, appuyez sur le bouton WPS de la box pendant la connexion. Les réseaux cachés et réglages avancés sont accessibles dans Configurer les réseaux.': ("For a secured network, the network agent asks for the password. If the access point and the agent offer WPS, press the router's WPS button while connecting. Hidden networks and advanced settings are in Configure networks.", 'En una red protegida, el agente de red pide la contraseña. Si el punto de acceso y el agente ofrecen WPS, pulse el botón WPS del router durante la conexión. Las redes ocultas y los ajustes avanzados están en Configurar las redes.', 'Bei einem geschützten Netzwerk fragt der Netzwerkagent nach dem Kennwort. Wenn Zugangspunkt und Agent WPS anbieten, drücken Sie während der Verbindung die WPS-Taste am Router. Versteckte Netzwerke und erweiterte Einstellungen finden Sie unter Netzwerke konfigurieren.'),
    'Ne pas déranger : conserver l’historique sans afficher de bannière': ('Do not disturb: keep the history without showing banners', 'No molestar: conservar el historial sin mostrar avisos', 'Nicht stören: Verlauf behalten, ohne Banner anzuzeigen'),
    'Effacer l’historique': ('Clear history', 'Borrar el historial', 'Verlauf löschen'),
    'Aucune notification enregistrée.': ('No notifications saved.', 'No hay notificaciones guardadas.', 'Keine gespeicherten Benachrichtigungen.'),
    'Programme': ('Program', 'Programa', 'Programm'),
    'Demander la fermeture': ('Ask to close', 'Solicitar el cierre', 'Schließen anfordern'),
    'Forcer l’arrêt du programme sélectionné': ('Force the selected program to stop', 'Forzar el cierre del programa seleccionado', 'Ausgewähltes Programm zwangsweise beenden'),
    'Ce processus ne peut pas être fermé ici.': ("This process can't be closed here.", 'Este proceso no se puede cerrar aquí.', 'Dieser Prozess kann hier nicht beendet werden.'),
    'Les modifications non enregistrées peuvent être perdues.': ('Unsaved changes may be lost.', 'Los cambios no guardados pueden perderse.', 'Nicht gespeicherte Änderungen können verloren gehen.'),
    'Le processus a changé. Sélectionnez-le à nouveau.': ('The process changed. Select it again.', 'El proceso ha cambiado. Selecciónelo de nuevo.', 'Der Prozess hat sich geändert. Wählen Sie ihn erneut aus.'),
    'Thème ': ('Theme ', 'Tema ', 'Design '),
    'Couleur d’accent': ('Accent color', 'Color de énfasis', 'Akzentfarbe'),
    'Choisir un fond d’écran': ('Choose a wallpaper', 'Elegir un fondo de pantalla', 'Hintergrundbild wählen'),
    'Choisir l’icône du menu Démarrer': ('Choose the Start menu icon', 'Elegir el icono del menú Inicio', 'Symbol des Startmenüs wählen'),
    'Icône du menu Démarrer': ('Start menu icon', 'Icono del menú Inicio', 'Startmenü-Symbol'),
    'Rétablir l’icône Démarrer par défaut': ('Restore the default Start icon', 'Restablecer el icono de Inicio predeterminado', 'Standard-Startsymbol wiederherstellen'),
    'Options avancées de la barre des tâches': ('Advanced taskbar options', 'Opciones avanzadas de la barra de tareas', 'Erweiterte Taskleistenoptionen'),
    'Rétablir l’apparence Winlux par défaut ?': ('Restore the default Winlux look?', '¿Restablecer la apariencia predeterminada de Winlux?', 'Standarddarstellung von Winlux wiederherstellen?'),
    'Le thème, l’accent, le fond, l’icône Démarrer et la taille de la barre seront réinitialisés.': ('The theme, accent, wallpaper, Start icon and taskbar size will be reset.', 'Se restablecerán el tema, el color de énfasis, el fondo, el icono de Inicio y el tamaño de la barra.', 'Design, Akzent, Hintergrund, Startsymbol und Taskleistengröße werden zurückgesetzt.'),
    'Rétablir l’apparence par défaut': ('Restore the default look', 'Restablecer la apariencia predeterminada', 'Standarddarstellung wiederherstellen'),
    'Applications Linux natives (APT)': ('Native Linux apps (APT)', 'Aplicaciones Linux nativas (APT)', 'Native Linux-Apps (APT)'),
    'Nom du paquet Ubuntu, par exemple vlc': ('Ubuntu package name, for example vlc', 'Nombre del paquete de Ubuntu, por ejemplo vlc', 'Ubuntu-Paketname, zum Beispiel vlc'),
    'Actualiser le catalogue APT': ('Refresh the APT catalog', 'Actualizar el catálogo APT', 'APT-Katalog aktualisieren'),
    'Rechercher dans les paquets APT': ('Search APT packages', 'Buscar en los paquetes APT', 'APT-Pakete durchsuchen'),
    'Saisissez une recherche.': ('Enter a search.', 'Escriba una búsqueda.', 'Geben Sie einen Suchbegriff ein.'),
    'Lister les paquets installés': ('List installed packages', 'Mostrar los paquetes instalados', 'Installierte Pakete auflisten'),
    'Entrez un nom de paquet valide.': ('Enter a valid package name.', 'Escriba un nombre de paquete válido.', 'Geben Sie einen gültigen Paketnamen ein.'),
    'Installer ': ('Install ', 'Instalar ', 'Installieren: '),
    'Désinstaller ': ('Uninstall ', 'Desinstalar ', 'Deinstallieren: '),
    'Installer le paquet Linux': ('Install the Linux package', 'Instalar el paquete Linux', 'Linux-Paket installieren'),
    'Désinstaller le paquet Linux': ('Uninstall the Linux package', 'Desinstalar el paquete Linux', 'Linux-Paket deinstallieren'),
    'Navigateurs intégrés et applications Flatpak installées pour votre profil.': ('Built-in browsers and Flatpak apps installed for your profile.', 'Navegadores integrados y aplicaciones Flatpak instaladas para su perfil.', 'Integrierte Browser und für Ihr Profil installierte Flatpak-Apps.'),
    'Gérer les navigateurs': ('Manage browsers', 'Administrar los navegadores', 'Browser verwalten'),
    'Identifiant Flatpak, par exemple org.videolan.VLC': ('Flatpak ID, for example org.videolan.VLC', 'Identificador de Flatpak, por ejemplo org.videolan.VLC', 'Flatpak-ID, zum Beispiel org.videolan.VLC'),
    'Entrez un identifiant d’application valide.': ('Enter a valid app ID.', 'Escriba un identificador de aplicación válido.', 'Geben Sie eine gültige App-ID ein.'),
    'Activer le catalogue Flathub': ('Enable the Flathub catalog', 'Activar el catálogo Flathub', 'Flathub-Katalog aktivieren'),
    'Rechercher une application Flatpak': ('Search for a Flatpak app', 'Buscar una aplicación Flatpak', 'Flatpak-App suchen'),
    'Installer': ('Install', 'Instalar', 'Installieren'),
    'Désinstaller': ('Uninstall', 'Desinstalar', 'Deinstallieren'),
    'Lister les applications Flatpak': ('List Flatpak apps', 'Mostrar las aplicaciones Flatpak', 'Flatpak-Apps auflisten'),
    'Mettre à jour les applications Flatpak de ce profil ?': ("Update this profile's Flatpak apps?", '¿Actualizar las aplicaciones Flatpak de este perfil?', 'Flatpak-Apps dieses Profils aktualisieren?'),
    'Mettre à jour les applications Flatpak': ('Update Flatpak apps', 'Actualizar las aplicaciones Flatpak', 'Flatpak-Apps aktualisieren'),
    'Applications par défaut': ('Default apps', 'Aplicaciones predeterminadas', 'Standard-Apps'),
    'Documents texte': ('Text documents', 'Documentos de texto', 'Textdokumente'),
    'Images PNG': ('PNG images', 'Imágenes PNG', 'PNG-Bilder'),
    'Images JPEG': ('JPEG images', 'Imágenes JPEG', 'JPEG-Bilder'),
    'Documents PDF': ('PDF documents', 'Documentos PDF', 'PDF-Dokumente'),
    'Pages HTML': ('HTML pages', 'Páginas HTML', 'HTML-Seiten'),
    'Dossiers': ('Folders', 'Carpetas', 'Ordner'),
    'Définir l’application par défaut': ('Set the default app', 'Establecer la aplicación predeterminada', 'Standard-App festlegen'),
    'Programmes lancés à la connexion': ('Programs started at sign-in', 'Programas que se inician al iniciar sesión', 'Bei der Anmeldung gestartete Programme'),
    'Préférence de démarrage enregistrée.': ('Startup preference saved.', 'Preferencia de inicio guardada.', 'Autostart-Einstellung gespeichert.'),
    'Activer au démarrage': ('Enable at startup', 'Activar al iniciar', 'Beim Start aktivieren'),
    'Désactiver au démarrage': ('Disable at startup', 'Desactivar al iniciar', 'Beim Start deaktivieren'),
    'L’Explorateur natif propose les favoris dans sa barre latérale, les miniatures et les périphériques amovibles.': ('The native Explorer offers favorites in its sidebar, thumbnails and removable devices.', 'El Explorador nativo ofrece favoritos en su barra lateral, miniaturas y dispositivos extraíbles.', 'Der native Explorer bietet Favoriten in der Seitenleiste, Miniaturansichten und Wechseldatenträger.'),
    'Ouvrir le gestionnaire de fichiers complet': ('Open the full file manager', 'Abrir el administrador de archivos completo', 'Vollständigen Dateimanager öffnen'),
    'Dossier personnel': ('Home folder', 'Carpeta personal', 'Persönlicher Ordner'),
    'Documents': ('Documents', 'Documentos', 'Dokumente'),
    'Téléchargements': ('Downloads', 'Descargas', 'Downloads'),
    'Corbeille': ('Recycle Bin', 'Papelera', 'Papierkorb'),
    'Ajouter un dossier aux favoris': ('Add a folder to favorites', 'Agregar una carpeta a favoritos', 'Ordner zu Favoriten hinzufügen'),
    'Aperçu d’un fichier': ('Preview a file', 'Vista previa de un archivo', 'Dateivorschau'),
    'Favoris : ouvrez un dossier puis utilisez Ctrl+D dans l’Explorateur natif. Éjection : bouton à côté du périphérique dans la barre latérale.': ('Favorites: open a folder then press Ctrl+D in the native Explorer. Eject: button next to the device in the sidebar.', 'Favoritos: abra una carpeta y pulse Ctrl+D en el Explorador nativo. Expulsar: botón junto al dispositivo en la barra lateral.', 'Favoriten: Öffnen Sie einen Ordner und drücken Sie Strg+D im nativen Explorer. Auswerfen: Schaltfläche neben dem Gerät in der Seitenleiste.'),
    'Lister les périphériques USB': ('List USB devices', 'Mostrar los dispositivos USB', 'USB-Geräte auflisten'),
    'Gestion des disques et éjection': ('Disk management and eject', 'Administración de discos y expulsión', 'Datenträgerverwaltung und Auswerfen'),
    'En cas de problème, une sauvegarde Winlux peut permettre de revenir à la version précédente. Les documents personnels ne sont pas une sauvegarde système : gardez-en également une copie sur un autre support.': ('If something goes wrong, a Winlux backup can take you back to the previous version. Personal documents are not part of a system backup: keep a copy on another drive too.', 'Si hay un problema, una copia de seguridad de Winlux permite volver a la versión anterior. Los documentos personales no forman parte de la copia del sistema: guarde también una copia en otro soporte.', 'Bei Problemen kann eine Winlux-Sicherung zur vorherigen Version zurückführen. Persönliche Dokumente sind keine Systemsicherung: Bewahren Sie auch eine Kopie auf einem anderen Datenträger auf.'),
    'Sauvegarde précédente détectée.': ('Previous backup found.', 'Se detectó una copia de seguridad anterior.', 'Vorherige Sicherung gefunden.'),
    'Aucune sauvegarde précédente détectée.': ('No previous backup found.', 'No se detectó ninguna copia de seguridad anterior.', 'Keine vorherige Sicherung gefunden.'),
    'Pour restaurer, ouvrez Mises à jour et choisissez Restaurer. Une confirmation est demandée avant toute modification.': ("To restore, open Updates and choose Restore. You'll be asked to confirm before anything changes.", 'Para restaurar, abra Actualizaciones y elija Restaurar. Se pide confirmación antes de cualquier cambio.', 'Öffnen Sie zum Wiederherstellen Updates und wählen Sie Wiederherstellen. Vor jeder Änderung wird eine Bestätigung angefordert.'),
    'Ouvrir les mises à jour': ('Open updates', 'Abrir las actualizaciones', 'Updates öffnen'),
    'Si le bureau ne démarre plus : utilisez l’entrée Winlux secours au démarrage si elle est disponible. Sinon, démarrez sur une clé USB d’installation Winlux. Le menu de secours ne remplace pas une sauvegarde de vos documents.': ('If the desktop no longer starts: use the Winlux recovery entry at boot if available. Otherwise, boot from a Winlux installation USB drive. The recovery menu does not replace a backup of your documents.', 'Si el escritorio ya no arranca: use la entrada de recuperación de Winlux al iniciar si está disponible. Si no, arranque desde una memoria USB de instalación de Winlux. El menú de recuperación no sustituye a una copia de seguridad de sus documentos.', 'Wenn der Desktop nicht mehr startet: Verwenden Sie beim Start den Eintrag „Winlux-Wiederherstellung“, falls vorhanden. Andernfalls starten Sie von einem Winlux-Installations-USB-Stick. Das Wiederherstellungsmenü ersetzt keine Sicherung Ihrer Dokumente.'),
    'Ouvrir le guide d’accueil': ('Open the welcome guide', 'Abrir la guía de bienvenida', 'Willkommensleitfaden öffnen'),
    'Choisir un dossier': ('Choose a folder', 'Elegir una carpeta', 'Ordner auswählen'),
    'Choisir un fichier': ('Choose a file', 'Elegir un archivo', 'Datei auswählen'),
    'Annuler': ('Cancel', 'Cancelar', 'Abbrechen'),
    'Choisir': ('Choose', 'Elegir', 'Auswählen'),
    'Dossier ajouté aux favoris de l’Explorateur natif.': ("Folder added to the native Explorer's favorites.", 'Carpeta agregada a los favoritos del Explorador nativo.', 'Ordner zu den Favoriten des nativen Explorers hinzugefügt.'),
    'Fermer': ('Close', 'Cerrar', 'Schließen'),
    'Aperçu indisponible pour ce format.': ('No preview for this format.', 'Vista previa no disponible para este formato.', 'Keine Vorschau für dieses Format.'),
    'Aperçu limité aux premiers 64 Kio.': ('Preview limited to the first 64 KiB.', 'Vista previa limitada a los primeros 64 KiB.', 'Vorschau auf die ersten 64 KiB begrenzt.'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


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
        super().__init__(title=_t("Outils Winlux"))
        self.set_default_size(850, 650)
        self.connect("destroy", Gtk.main_quit)
        self.page = page
        self.process = None
        self.previous = {}
        self.previous_time = time.monotonic()
        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12, margin=20)
        self.add(self.box)
        self.box.pack_start(Gtk.Label(label={"quick":_t("Réglages rapides"), "notifications":_t("Notifications"),
            "tasks":_t("Gestionnaire de tâches"), "appearance":_t("Personnalisation"), "apps":_t("Applications"),
            "files":_t("Fichiers et périphériques"), "recovery":_t("Secours et restauration")}.get(page, "Winlux"), xalign=0), False, False, 0)
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
            self.notice.set_text(_t("Opération en cours…"))
        except OSError as error: self.notice.set_text(str(error))

    def poll(self):
        if self.process is not None:
            chunk = self.process.stdout.read()
            if chunk: self.output.extend(chunk[:max(0, 64000-len(self.output))])
            if self.process.poll() is not None:
                self.notice.set_text(self.output.decode(errors="replace")[-8000:] or
                    (_t("Opération terminée.") if self.process.returncode == 0 else _t("Échec de l’opération.")))
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
        self.label(_t("Ethernet est prioritaire quand il est disponible. Le Wi-Fi peut rester configuré et prendre le relais lorsque le câble est débranché. Dans une VM, la connexion apparaît généralement comme Ethernet."))
        networks = Gtk.ComboBoxText()
        self.content.pack_start(networks, False, False, 0)
        def scan():
            self.notice.set_text(_t("Recherche des réseaux Wi-Fi…"))
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
                            networks.append(row[0], f"{row[1]} · {row[2]} % · {row[3] or _t('Ouvert')}")
                            count += 1
                        if count: networks.set_active(0)
                        self.notice.set_text(_t("{count} réseau(x) trouvé(s).").format(count=count) if count else _t("Aucun réseau visible ou aucune carte Wi-Fi disponible."))
                        return False
                    GLib.idle_add(display)
                except (OSError, subprocess.SubprocessError) as error:
                    detail = getattr(error, "stderr", "") or str(error)
                    GLib.idle_add(lambda: (self.notice.set_text(_t("Recherche Wi-Fi impossible : ") + detail.strip()), False)[1])
            threading.Thread(target=worker, daemon=True).start()
        self.button(_t("Rechercher les réseaux Wi-Fi"), scan, bool(shutil.which("nmcli")))
        self.button(_t("Se connecter au réseau sélectionné"), lambda: self.command(["nmcli", "--wait", "90", "device", "wifi", "connect", networks.get_active_id()]) if networks.get_active_id() else self.notice.set_text(_t("Choisissez d’abord un réseau.")), bool(shutil.which("nmcli")))
        for title, args, binary in [
            (_t("Réseaux Wi-Fi disponibles"), ["nmcli","-f","SSID,SIGNAL,SECURITY","device","wifi","list"],"nmcli"),
            (_t("Actualiser la recherche Wi-Fi"), ["nmcli","device","wifi","rescan"],"nmcli"),
            (_t("État des cartes Ethernet et Wi-Fi"), ["nmcli","device","status"],"nmcli"),
            (_t("Wi-Fi : activer"), ["nmcli","radio","wifi","on"],"nmcli"),
            (_t("Wi-Fi : désactiver"), ["nmcli","radio","wifi","off"],"nmcli"),
            (_t("Configurer les réseaux"), ["nm-connection-editor"],"nm-connection-editor"),
            (_t("Bluetooth : activer"), ["bluetoothctl","power","on"],"bluetoothctl"),
            (_t("Bluetooth : désactiver"), ["bluetoothctl","power","off"],"bluetoothctl"),
            (_t("Associer un périphérique Bluetooth"), ["blueman-manager"],"blueman-manager"),
            (_t("Luminosité −"), ["brightnessctl","set","10%-"],"brightnessctl"),
            (_t("Luminosité +"), ["brightnessctl","set","+10%"],"brightnessctl"),
            ("Volume −", ["winlux-control","volume-down"],"winlux-control"),
            (_t("Muet / rétablir"), ["winlux-control","volume-mute"],"winlux-control"),
            ("Volume +", ["winlux-control","volume-up"],"winlux-control"),
            (_t("Alimentation"), ["winlux-shell-app","settings","0"],"winlux-shell-app")]:
            self.button(title, lambda command=args: self.command(command), bool(shutil.which(binary)))
        ssid=self.entry(_t("Nom du réseau Wi-Fi à rejoindre"))
        self.button(_t("Se connecter au réseau"),lambda:self.command(["nmcli","--wait","90","device","wifi","connect",ssid.get_text()]) if ssid.get_text() else None,bool(shutil.which("nmcli")))
        self.label(_t("Pour un réseau protégé, l’agent réseau demande le mot de passe. Si le point d’accès et l’agent proposent WPS, appuyez sur le bouton WPS de la box pendant la connexion. Les réseaux cachés et réglages avancés sont accessibles dans Configurer les réseaux."))

    def notification_call(self, method, parameters=None):
        try:
            Gio.bus_get_sync(Gio.BusType.SESSION, None).call_sync("org.freedesktop.Notifications",
                "/org/freedesktop/Notifications", "org.winlux.Notifications", method,
                parameters, None, Gio.DBusCallFlags.NONE, 3000, None)
        except GLib.Error as error: self.notice.set_text(str(error))

    def notifications(self):
        preference = Path(os.environ.get("XDG_CONFIG_HOME",HOME/".config")) / "winlux/do-not-disturb"
        switch = Gtk.CheckButton(label=_t("Ne pas déranger : conserver l’historique sans afficher de bannière"))
        switch.set_active(preference.exists())
        def toggle(widget):
            preference.parent.mkdir(parents=True, exist_ok=True)
            if widget.get_active(): preference.touch(mode=0o600)
            else: preference.unlink(missing_ok=True)
        switch.connect("toggled", toggle)
        self.content.pack_start(switch, False, False, 0)
        self.button(_t("Effacer l’historique"), lambda: self.notification_call("Clear"))
        self.notification_snapshot = read(DATA / "notifications.json")
        try: history = json.loads(self.notification_snapshot or "[]")
        except ValueError: history = []
        if not history: self.label(_t("Aucune notification enregistrée."))
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
        for index, title in enumerate(["PID", _t("Programme"), "CPU % (un cœur)", "RAM Mio"]):
            column = Gtk.TreeViewColumn(title, Gtk.CellRendererText(), text=index)
            column.set_sort_column_id(index)
            column.set_resizable(True)
            self.tree.append_column(column)
        self.content.pack_start(self.tree, True, True, 0)
        self.button(_t("Demander la fermeture"), lambda: self.stop_task(signal.SIGTERM))
        self.button(_t("Forcer l’arrêt du programme sélectionné"), lambda: self.stop_task(signal.SIGKILL))
        self.refresh_tasks()

    def stop_task(self, sig):
        model, iterator = self.tree.get_selection().get_selected()
        if iterator is None: return
        pid, name, _, _, stamp = model[iterator]
        if pid in (1, os.getpid()):
            message(self, _t("Ce processus ne peut pas être fermé ici."))
            return
        dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
            text=_t("Fermer {name} (PID {pid}) ?").format(name=name, pid=pid), secondary_text=_t("Les modifications non enregistrées peuvent être perdues."))
        answer = dialog.run(); dialog.destroy()
        if answer != Gtk.ResponseType.OK: return
        try:
            stat = Path(f"/proc/{pid}/stat").read_text().rsplit(")",1)[1].split()
            if stat[19] != stamp: raise RuntimeError(_t("Le processus a changé. Sélectionnez-le à nouveau."))
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
        self.summary.set_text(_t("{n} processus · RAM disponible : {ram}").format(n=len(current), ram=memory.get('MemAvailable',_t('inconnue')).strip()))

    def appearance(self):
        for theme in ("light", "dark"):
            self.button(_t("Thème ") + ("clair" if theme=="light" else "sombre"), lambda value=theme: self.command(["winlux-personalize","theme",value]))
        color = Gtk.ColorButton(title=_t("Couleur d’accent"))
        color.connect("color-set", lambda widget: self.command(["winlux-personalize","accent",
            "#%02x%02x%02x" % tuple(round(v*255) for v in (widget.get_rgba().red,widget.get_rgba().green,widget.get_rgba().blue))]))
        self.content.pack_start(color, False, False, 0)
        chooser = Gtk.FileChooserButton(title=_t("Choisir un fond d’écran"))
        chooser.connect("file-set", lambda widget: self.command(["winlux-personalize","wallpaper",widget.get_filename()]))
        self.content.pack_start(chooser, False, False, 0)
        icon = Gtk.FileChooserButton(title=_t("Choisir l’icône du menu Démarrer"))
        icon.connect("file-set", lambda widget: self.command(["winlux-personalize", "start-icon", widget.get_filename()]))
        self.label(_t("Icône du menu Démarrer"))
        self.content.pack_start(icon, False, False, 0)
        self.button(_t("Rétablir l’icône Démarrer par défaut"), lambda: self.command(["winlux-personalize", "start-icon", "default"]))
        for size in (32,40,48,56,64): self.button(_t("Barre des tâches : {size} px").format(size=size), lambda value=size: self.command(["winlux-personalize","panel-size",str(value)]))
        self.button(_t("Options avancées de la barre des tâches"), lambda: self.command(["xfce4-panel", "--preferences"]), bool(shutil.which("xfce4-panel")))
        def reset_appearance():
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
                                       text=_t("Rétablir l’apparence Winlux par défaut ?"),
                                       secondary_text=_t("Le thème, l’accent, le fond, l’icône Démarrer et la taille de la barre seront réinitialisés."))
            result = dialog.run(); dialog.destroy()
            if result == Gtk.ResponseType.OK: self.command(["winlux-personalize", "reset", "default"])
        self.button(_t("Rétablir l’apparence par défaut"), reset_appearance)

    def apps(self):
        if Path("/usr/share/winlux/apt-base").is_file():
            self.label(_t("Applications Linux natives (APT)"))
            native = self.entry(_t("Nom du paquet Ubuntu, par exemple vlc"))
            self.button(_t("Actualiser le catalogue APT"), lambda:self.command(["apt-get","update"]))
            self.button(_t("Rechercher dans les paquets APT"), lambda: self.command(["apt-cache", "search", native.get_text().strip()]) if native.get_text().strip() else self.notice.set_text(_t("Saisissez une recherche.")))
            self.button(_t("Lister les paquets installés"), lambda: self.command(["dpkg-query", "-W", "-f=${binary:Package} ${Version}\\n"]))
            def apt(action):
                name = native.get_text().strip()
                if not re.fullmatch(r"[a-z0-9][a-z0-9+.-]+",name):
                    message(self,_t("Entrez un nom de paquet valide."));return
                dialog=Gtk.MessageDialog(transient_for=self,modal=True,buttons=Gtk.ButtonsType.OK_CANCEL,
                    text=(_t("Installer ") if action=="install" else _t("Désinstaller "))+name+" ?")
                answer=dialog.run();dialog.destroy()
                if answer==Gtk.ResponseType.OK:
                    self.command(["env","DEBIAN_FRONTEND=noninteractive","apt-get","-y",
                                  "-o","Dpkg::Options::=--force-confold",action,name])
            self.button(_t("Installer le paquet Linux"),lambda:apt("install"))
            self.button(_t("Désinstaller le paquet Linux"),lambda:apt("remove"))
        self.label(_t("Navigateurs intégrés et applications Flatpak installées pour votre profil."))
        self.button(_t("Gérer les navigateurs"), lambda: self.command(["winlux-shell-app","settings","11"]))
        identity = self.entry(_t("Identifiant Flatpak, par exemple org.videolan.VLC"))
        def flatpak(action):
            app = identity.get_text().strip()
            if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]*(?:\.[A-Za-z0-9_-]+){2,}",app):
                message(self,_t("Entrez un identifiant d’application valide.")); return
            args = ["flatpak",action,"--user","--noninteractive"]
            if action=="install": args += ["flathub",app]
            else: args += [app]
            self.command(args)
        self.button(_t("Activer le catalogue Flathub"), lambda: self.command(["flatpak","remote-add","--user","--if-not-exists","flathub","https://dl.flathub.org/repo/flathub.flatpakrepo"]),bool(shutil.which("flatpak")))
        self.button(_t("Rechercher une application Flatpak"), lambda: self.command(["flatpak", "search", identity.get_text().strip()]) if identity.get_text().strip() else self.notice.set_text(_t("Saisissez une recherche.")), bool(shutil.which("flatpak")))
        self.button(_t("Installer"), lambda: flatpak("install"),bool(shutil.which("flatpak")))
        self.button(_t("Désinstaller"), lambda: flatpak("uninstall"),bool(shutil.which("flatpak")))
        self.button(_t("Lister les applications Flatpak"), lambda: self.command(["flatpak","list","--user"]),bool(shutil.which("flatpak")))
        def update_flatpak():
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
                                       text=_t("Mettre à jour les applications Flatpak de ce profil ?"))
            response = dialog.run(); dialog.destroy()
            if response == Gtk.ResponseType.OK:
                self.command(["flatpak", "update", "--user", "--noninteractive"])
        self.button(_t("Mettre à jour les applications Flatpak"), update_flatpak, bool(shutil.which("flatpak")))
        self.label(_t("Applications par défaut"))
        types=Gtk.ComboBoxText()
        for mime,label in [("text/plain",_t("Documents texte")),("image/png",_t("Images PNG")),("image/jpeg",_t("Images JPEG")),
                           ("application/pdf",_t("Documents PDF")),("text/html",_t("Pages HTML")),("inode/directory",_t("Dossiers"))]:types.append(mime,label)
        holder=Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.content.pack_start(types,False,False,0);self.content.pack_start(holder,False,False,0)
        def choose_type(*_):
            for child in holder.get_children():child.destroy()
            self.default=Gtk.AppChooserButton(content_type=types.get_active_id())
            self.default.set_show_dialog_item(True);holder.pack_start(self.default,False,False,0);holder.show_all()
        types.connect("changed",choose_type);types.set_active(0)
        self.button(_t("Définir l’application par défaut"),lambda:self.default.get_app_info().set_as_default_for_type(types.get_active_id()) if self.default.get_app_info() else None)
        self.label(_t("Programmes lancés à la connexion"))
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
            destination.write_text(key.to_data()[0]); self.notice.set_text(_t("Préférence de démarrage enregistrée."))
        self.button(_t("Activer au démarrage"),lambda:startup(True))
        self.button(_t("Désactiver au démarrage"),lambda:startup(False))

    def files(self):
        self.label(_t("L’Explorateur natif propose les favoris dans sa barre latérale, les miniatures et les périphériques amovibles."))
        self.button(_t("Ouvrir le gestionnaire de fichiers complet"), lambda: self.command(["Thunar"]), bool(shutil.which("Thunar")))
        # Dossiers XDG réels (user-dirs.dirs) : leur nom sur disque ne suit pas la langue de l'interface.
        def xdg(kind,fallback):
            special=GLib.get_user_special_dir(kind)
            return Path(special) if special else HOME/fallback
        for label,path in [(_t("Dossier personnel"),HOME),(_t("Documents"),xdg(GLib.UserDirectory.DIRECTORY_DOCUMENTS,"Documents")),(_t("Téléchargements"),xdg(GLib.UserDirectory.DIRECTORY_DOWNLOAD,"Téléchargements"))]:
            self.button(label,lambda target=path:self.command(["Thunar",str(target)]))
        self.button(_t("Corbeille"),lambda:self.command(["Thunar","trash:///"]))
        self.button(_t("Ajouter un dossier aux favoris"),self.add_bookmark)
        self.button(_t("Aperçu d’un fichier"),self.preview)
        self.label(_t("Favoris : ouvrez un dossier puis utilisez Ctrl+D dans l’Explorateur natif. Éjection : bouton à côté du périphérique dans la barre latérale."))
        self.button(_t("Lister les périphériques USB"),lambda:self.command(["lsblk","-o","NAME,SIZE,TRAN,MOUNTPOINTS,MODEL"]))
        self.button(_t("Gestion des disques et éjection"),lambda:self.command(["gnome-disks"]),bool(shutil.which("gnome-disks")))

    def recovery(self):
        self.label(_t("En cas de problème, une sauvegarde Winlux peut permettre de revenir à la version précédente. Les documents personnels ne sont pas une sauvegarde système : gardez-en également une copie sur un autre support."))
        if Path("/usr/share/winlux/update-backend").is_file() and read("/usr/share/winlux/update-backend").strip() == "apt":
            available = Path("/var/cache/winlux/previous.deb").is_file()
        else:
            name = read("/var/lib/winlux/rollback-current").strip()
            available = bool(re.fullmatch(r"update-backup\.[A-Za-z0-9._+-]+", name)) and (Path("/var/lib/winlux") / name / "restore.list").is_file()
        self.label(_t("Sauvegarde précédente détectée.") if available else _t("Aucune sauvegarde précédente détectée."))
        self.label(_t("Pour restaurer, ouvrez Mises à jour et choisissez Restaurer. Une confirmation est demandée avant toute modification."))
        self.button(_t("Ouvrir les mises à jour"), lambda: self.command(["winlux-shell-app", "settings", "7"]))
        self.label(_t("Si le bureau ne démarre plus : utilisez l’entrée Winlux secours au démarrage si elle est disponible. Sinon, démarrez sur une clé USB d’installation Winlux. Le menu de secours ne remplace pas une sauvegarde de vos documents."))
        self.button(_t("Ouvrir le guide d’accueil"), lambda: self.command(["winlux-welcome"]))

    def pick(self,folder=False):
        dialog=Gtk.FileChooserDialog(title=_t("Choisir un dossier") if folder else _t("Choisir un fichier"),transient_for=self,
            action=Gtk.FileChooserAction.SELECT_FOLDER if folder else Gtk.FileChooserAction.OPEN)
        dialog.add_buttons(_t("Annuler"),Gtk.ResponseType.CANCEL,_t("Choisir"),Gtk.ResponseType.OK)
        answer=dialog.run();path=dialog.get_filename() if answer==Gtk.ResponseType.OK else None;dialog.destroy()
        return Path(path) if path else None

    def add_bookmark(self):
        folder=self.pick(True)
        if not folder:return
        bookmark=HOME/".config/gtk-3.0/bookmarks";bookmark.parent.mkdir(parents=True,exist_ok=True)
        lines=read(bookmark).splitlines();uri=folder.resolve().as_uri()
        if not any(line.split(" ",1)[0]==uri for line in lines):lines.append(uri)
        bookmark.write_text("\n".join(lines)+"\n");self.notice.set_text(_t("Dossier ajouté aux favoris de l’Explorateur natif."))

    def preview(self):
        path=self.pick()
        if not path:return
        dialog=Gtk.Dialog(title=path.name,transient_for=self,modal=True)
        dialog.set_default_size(640,500);dialog.add_button(_t("Fermer"),Gtk.ResponseType.CLOSE)
        area=dialog.get_content_area()
        try:
            info=GdkPixbuf.Pixbuf.get_file_info(str(path))
            if info and info[0] and path.stat().st_size<20*1024*1024 and info[1]*info[2]<40000000:
                area.add(Gtk.Image.new_from_pixbuf(GdkPixbuf.Pixbuf.new_from_file_at_scale(str(path),600,440,True)))
            else:
                with path.open("rb") as stream:data=stream.read(65536)
                text=_t("Aperçu indisponible pour ce format.") if b"\0" in data else data.decode("utf-8",errors="replace")
                view=Gtk.TextView(editable=False,wrap_mode=Gtk.WrapMode.WORD_CHAR)
                view.get_buffer().set_text(text)
                scroll=Gtk.ScrolledWindow();scroll.add(view);area.pack_start(scroll,True,True,0)
                if len(data)==65536:area.add(Gtk.Label(label=_t("Aperçu limité aux premiers 64 Kio.")))
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
    # Le Gestionnaire des tâches complet (onglets, graphiques) est l'application
    # Winlux intégrée ; cette page reste en secours si elle n'est pas installée.
    if page == "tasks" and Path("/opt/winlux/tests/gui_demo.exe").is_file() and shutil.which("winlux-shell-app"):
        os.execvp("winlux-shell-app", ["winlux-shell-app", "taskmgr"])
    Tools(page)
    Gtk.main()

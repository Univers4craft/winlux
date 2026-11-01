#!/usr/bin/python3
"""Pages des Paramètres natifs branchées sur les vrais services du système.

Module chargé par winlux-settings (fenêtre GTK du bureau natif). Chaque page
agit sur ce qui tourne réellement dans la session : BlueZ, PipeWire (pactl),
APT (winlux-sysupdate), lecteurs réseau (winlux-drives), registre Windows,
xsettings (xfconf) et Openbox. Une page dont le matériel ou le service est
absent n'apparaît pas ; une section impossible sur cette machine est masquée.
"""
import configparser
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import xml.etree.ElementTree as ET
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf, GLib, Gtk


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
    # Commun
    "Activé": ("On", "Activado", "Ein"),
    "Désactivé": ("Off", "Desactivado", "Aus"),
    "Appliquer": ("Apply", "Aplicar", "Übernehmen"),
    "L’opération a échoué": ("The operation failed", "La operación ha fallado", "Der Vorgang ist fehlgeschlagen"),
    "Supprimer": ("Remove", "Eliminar", "Entfernen"),
    # Bluetooth et appareils
    "Bluetooth": ("Bluetooth", "Bluetooth", "Bluetooth"),
    "Recherche d’appareils en cours…": ("Searching for devices…", "Buscando dispositivos…", "Geräte werden gesucht…"),
    "Visible pour les appareils à proximité": ("Visible to nearby devices", "Visible para dispositivos cercanos", "Für Geräte in der Nähe sichtbar"),
    "Ajouter un appareil": ("Add a device", "Agregar un dispositivo", "Gerät hinzufügen"),
    "Mettez l’appareil en mode appairage, puis lancez la recherche.": ("Put the device in pairing mode, then start the search.", "Ponga el dispositivo en modo de emparejamiento y luego inicie la búsqueda.", "Versetzen Sie das Gerät in den Kopplungsmodus und starten Sie dann die Suche."),
    "Appairer": ("Pair", "Emparejar", "Koppeln"),
    "Connecter": ("Connect", "Conectar", "Verbinden"),
    "Déconnecter": ("Disconnect", "Desconectar", "Trennen"),
    "Oublier": ("Forget", "Olvidar", "Entfernen"),
    "Connecté": ("Connected", "Conectado", "Verbunden"),
    "Appairé": ("Paired", "Emparejado", "Gekoppelt"),
    "Disponible": ("Available", "Disponible", "Verfügbar"),
    "Audio": ("Audio", "Audio", "Audio"),
    "Clavier": ("Keyboard", "Teclado", "Tastatur"),
    "Souris": ("Mouse", "Mouse", "Maus"),
    "Manette": ("Gamepad", "Mando", "Gamepad"),
    "Téléphone": ("Phone", "Teléfono", "Telefon"),
    "Ordinateur": ("Computer", "Equipo", "Computer"),
    "Appareil": ("Device", "Dispositivo", "Gerät"),
    "Options Bluetooth avancées": ("More Bluetooth options", "Más opciones de Bluetooth", "Weitere Bluetooth-Optionen"),
    "Appareils connectés à ce PC": ("Devices connected to this PC", "Dispositivos conectados a este equipo", "Mit diesem PC verbundene Geräte"),
    "Manettes de jeu": ("Game controllers", "Mandos de juego", "Gamecontroller"),
    "Webcams": ("Webcams", "Cámaras web", "Webcams"),
    "Micro par défaut": ("Default microphone", "Micrófono predeterminado", "Standardmikrofon"),
    "Périphériques d’entrée": ("Input devices", "Dispositivos de entrada", "Eingabegeräte"),
    "Les programmes Windows (.exe) utilisent ces appareils directement.": ("Windows programs (.exe) use these devices directly.", "Los programas de Windows (.exe) usan estos dispositivos directamente.", "Windows-Programme (.exe) verwenden diese Geräte direkt."),
    # Personnalisation du bureau
    "Arrière-plan": ("Background", "Fondo", "Hintergrund"),
    "Winlux Nuit": ("Winlux Night", "Winlux Noche", "Winlux Nacht"),
    "Winlux Aube": ("Winlux Dawn", "Winlux Amanecer", "Winlux Morgenröte"),
    "Classique": ("Classic", "Clásico", "Klassisch"),
    "Choisir une photo…": ("Browse photos…", "Elegir una foto…", "Foto auswählen…"),
    "Mode de couleur": ("Color mode", "Modo de color", "Farbmodus"),
    "Sombre": ("Dark", "Oscuro", "Dunkel"),
    "Clair": ("Light", "Claro", "Hell"),
    "Couleur d’accentuation": ("Accent color", "Color de énfasis", "Akzentfarbe"),
    "Personnalisée…": ("Custom…", "Personalizado…", "Benutzerdefiniert…"),
    "Barre des tâches": ("Taskbar", "Barra de tareas", "Taskleiste"),
    "Taille": ("Size", "Tamaño", "Größe"),
    "Petite": ("Small", "Pequeña", "Klein"),
    "Moyenne": ("Medium", "Mediana", "Mittel"),
    "Grande": ("Large", "Grande", "Groß"),
    "Très grande": ("Extra large", "Muy grande", "Sehr groß"),
    "Alignement": ("Alignment", "Alineación", "Ausrichtung"),
    "Centré": ("Center", "Centrado", "Zentriert"),
    "À gauche": ("Left", "A la izquierda", "Links"),
    "Icône du menu Démarrer": ("Start menu icon", "Icono del menú Inicio", "Startmenü-Symbol"),
    "Choisir une image…": ("Choose an image…", "Elegir una imagen…", "Bild auswählen…"),
    "Icône Winlux": ("Winlux icon", "Icono de Winlux", "Winlux-Symbol"),
    "Rétablir l’apparence par défaut": ("Restore the default look", "Restablecer la apariencia predeterminada", "Standarddarstellung wiederherstellen"),
    "Images": ("Images", "Imágenes", "Bilder"),
    # Lecteurs réseau
    "Lecteurs connectés": ("Connected drives", "Unidades conectadas", "Verbundene Laufwerke"),
    "Aucun lecteur réseau pour l’instant.": ("No network drive yet.", "Todavía no hay unidades de red.", "Noch kein Netzlaufwerk."),
    "En ligne": ("Online", "En línea", "Online"),
    "Hors ligne": ("Offline", "Sin conexión", "Offline"),
    "Connexion…": ("Connecting…", "Conectando…", "Verbindung wird hergestellt…"),
    "Mot de passe requis": ("Password required", "Contraseña requerida", "Kennwort erforderlich"),
    "Libre : {free}": ("Free: {free}", "Libre: {free}", "Frei: {free}"),
    "Connecter un lecteur réseau": ("Map a network drive", "Conectar a una unidad de red", "Netzlaufwerk verbinden"),
    "Type de connexion": ("Connection type", "Tipo de conexión", "Verbindungstyp"),
    "Serveur": ("Server", "Servidor", "Server"),
    "Port (facultatif)": ("Port (optional)", "Puerto (opcional)", "Port (optional)"),
    "Nom d’utilisateur": ("User name", "Nombre de usuario", "Benutzername"),
    "Mot de passe": ("Password", "Contraseña", "Kennwort"),
    "Dossier distant": ("Remote folder", "Carpeta remota", "Remoteordner"),
    "Nom affiché": ("Display name", "Nombre para mostrar", "Anzeigename"),
    "Lettre": ("Letter", "Letra", "Buchstabe"),
    "Se reconnecter à l’ouverture de session": ("Reconnect at sign-in", "Volver a conectar al iniciar sesión", "Bei der Anmeldung erneut verbinden"),
    "Mémoriser le mot de passe": ("Remember the password", "Recordar la contraseña", "Kennwort speichern"),
    "Tester": ("Test", "Probar", "Testen"),
    "Indiquez l’adresse du serveur.": ("Enter the server address.", "Indique la dirección del servidor.", "Geben Sie die Serveradresse ein."),
    "Le mot de passe est chiffré et ne quitte jamais cet ordinateur.": ("The password is encrypted and never leaves this computer.", "La contraseña se cifra y nunca sale de este equipo.", "Das Kennwort wird verschlüsselt und verlässt diesen Computer nie."),
    "Supprimer le lecteur {letter}: ?": ("Remove drive {letter}:?", "¿Eliminar la unidad {letter}:?", "Laufwerk {letter}: entfernen?"),
    # Mises à jour système
    "Logiciels et pilotes Linux": ("Linux software and drivers", "Software y controladores de Linux", "Linux-Software und Treiber"),
    "Installation des mises à jour": ("Installing updates", "Instalación de actualizaciones", "Installation von Updates"),
    "Automatique": ("Automatic", "Automática", "Automatisch"),
    "Manuelle (je choisis quand)": ("Manual (I choose when)", "Manual (yo elijo cuándo)", "Manuell (ich entscheide wann)"),
    "Planifiée": ("Scheduled", "Programada", "Geplant"),
    "Chaque jour à {hour} h": ("Every day at {hour}:00", "Todos los días a las {hour}:00", "Täglich um {hour}:00 Uhr"),
    "Chaque {day} à {hour} h": ("Every {day} at {hour}:00", "Cada {day} a las {hour}:00", "Jeden {day} um {hour}:00 Uhr"),
    "Heure −": ("Hour −", "Hora −", "Stunde −"),
    "Heure +": ("Hour +", "Hora +", "Stunde +"),
    "Changer de jour": ("Change day", "Cambiar de día", "Tag ändern"),
    "lundi": ("Monday", "lunes", "Montag"), "mardi": ("Tuesday", "martes", "Dienstag"),
    "mercredi": ("Wednesday", "miércoles", "Mittwoch"), "jeudi": ("Thursday", "jueves", "Donnerstag"),
    "vendredi": ("Friday", "viernes", "Freitag"), "samedi": ("Saturday", "sábado", "Samstag"),
    "dimanche": ("Sunday", "domingo", "Sonntag"),
    "Dernière recherche : {date}": ("Last checked: {date}", "Última búsqueda: {date}", "Zuletzt gesucht: {date}"),
    "jamais": ("never", "nunca", "nie"),
    "Aucune recherche pour l’instant.": ("Not checked yet.", "Todavía no se ha buscado.", "Noch nicht gesucht."),
    "Parcourir…": ("Browse…", "Examinar…", "Durchsuchen…"),
    "Votre système est à jour.": ("Your system is up to date.", "El sistema está actualizado.", "Ihr System ist auf dem neuesten Stand."),
    "{count} mise(s) à jour disponible(s)": ("{count} update(s) available", "{count} actualización(es) disponible(s)", "{count} Update(s) verfügbar"),
    "Rechercher des mises à jour": ("Check for updates", "Buscar actualizaciones", "Nach Updates suchen"),
    "Tout installer": ("Install all", "Instalar todo", "Alle installieren"),
    "Recherche ou installation en cours…": ("Checking or installing…", "Buscando o instalando…", "Suche oder Installation läuft…"),
    "Journal": ("Log", "Registro", "Protokoll"),
    "Les mises à jour de Winlux lui-même sont dans « Mises à jour ».": ("Updates to Winlux itself are under “Updates”.", "Las actualizaciones del propio Winlux están en «Actualizaciones».", "Updates von Winlux selbst finden Sie unter „Updates“."),
    # Son avancé
    "Sortie": ("Output", "Salida", "Ausgabe"),
    "Entrée": ("Input", "Entrada", "Eingabe"),
    "Haut-parleurs ou casque": ("Speakers or headphones", "Altavoces o auriculares", "Lautsprecher oder Kopfhörer"),
    "Micro": ("Microphone", "Micrófono", "Mikrofon"),
    "Volume": ("Volume", "Volumen", "Lautstärke"),
    "Muet": ("Mute", "Silencio", "Stumm"),
    "Mélangeur de volume": ("Volume mixer", "Mezclador de volumen", "Lautstärkemixer"),
    "Aucune application ne joue de son pour l’instant.": ("No app is playing sound right now.", "Ninguna aplicación reproduce sonido ahora.", "Zurzeit gibt keine App Ton wieder."),
    # Gestionnaire de fichiers
    "Explorateur de fichiers": ("File Explorer", "Explorador de archivos", "Datei-Explorer"),
    "Afficher les fichiers et dossiers cachés": ("Show hidden files and folders", "Mostrar archivos y carpetas ocultos", "Ausgeblendete Dateien und Ordner anzeigen"),
    "Afficher les extensions des noms de fichiers": ("Show file name extensions", "Mostrar extensiones de nombre de archivo", "Dateinamenerweiterungen anzeigen"),
    "Affichage par défaut": ("Default view", "Vista predeterminada", "Standardansicht"),
    "Très grandes icônes": ("Extra large icons", "Iconos muy grandes", "Extra große Symbole"),
    "Grandes icônes": ("Large icons", "Iconos grandes", "Große Symbole"),
    "Icônes moyennes": ("Medium icons", "Iconos medianos", "Mittelgroße Symbole"),
    "Petites icônes": ("Small icons", "Iconos pequeños", "Kleine Symbole"),
    "Liste": ("List", "Lista", "Liste"),
    "Détails": ("Details", "Detalles", "Details"),
    "Mosaïques": ("Tiles", "Mosaicos", "Kacheln"),
    "S’applique aux prochaines fenêtres de l’Explorateur et aux programmes Windows.": ("Applies to new File Explorer windows and to Windows programs.", "Se aplica a las nuevas ventanas del Explorador y a los programas de Windows.", "Gilt für neue Explorer-Fenster und für Windows-Programme."),
    "Ouvrir l’Explorateur": ("Open File Explorer", "Abrir el Explorador", "Explorer öffnen"),
    # Localisation
    "Langue de l’interface": ("Display language", "Idioma de la interfaz", "Anzeigesprache"),
    "S’applique aux fenêtres ouvertes ensuite.": ("Applies to windows opened afterwards.", "Se aplica a las ventanas que se abran después.", "Gilt für danach geöffnete Fenster."),
    "Disposition du clavier": ("Keyboard layout", "Distribución del teclado", "Tastaturlayout"),
    "Fuseau horaire": ("Time zone", "Zona horaria", "Zeitzone"),
    "Date et heure actuelles : {now}": ("Current date and time: {now}", "Fecha y hora actuales: {now}", "Aktuelles Datum und Uhrzeit: {now}"),
    "Français (France)": ("French (France)", "Francés (Francia)", "Französisch (Frankreich)"),
    "Français (AZERTY)": ("French (AZERTY)", "Francés (AZERTY)", "Französisch (AZERTY)"),
    "Anglais (États-Unis, QWERTY)": ("English (US, QWERTY)", "Inglés (EE. UU., QWERTY)", "Englisch (USA, QWERTY)"),
    "Anglais (Royaume-Uni)": ("English (UK)", "Inglés (Reino Unido)", "Englisch (Vereinigtes Königreich)"),
    "Allemand (QWERTZ)": ("German (QWERTZ)", "Alemán (QWERTZ)", "Deutsch (QWERTZ)"),
    "Espagnol": ("Spanish", "Español", "Spanisch"),
    "Italien": ("Italian", "Italiano", "Italienisch"),
    "Portugais": ("Portuguese", "Portugués", "Portugiesisch"),
    "Belge (AZERTY)": ("Belgian (AZERTY)", "Belga (AZERTY)", "Belgisch (AZERTY)"),
    # Accessibilité
    "Vision": ("Vision", "Visión", "Sehen"),
    "Thème de contraste": ("Contrast theme", "Tema de contraste", "Kontrastdesign"),
    "Aucun": ("None", "Ninguno", "Keins"),
    "Aquatique": ("Aquatic", "Acuático", "Aquatisch"),
    "Désert": ("Desert", "Desierto", "Wüste"),
    "Crépuscule": ("Dusk", "Crepúsculo", "Dämmerung"),
    "Ciel nocturne": ("Night sky", "Cielo nocturno", "Nachthimmel"),
    "Taille du pointeur": ("Mouse pointer size", "Tamaño del puntero", "Zeigergröße"),
    "Effets d’animation": ("Animation effects", "Efectos de animación", "Animationseffekte"),
    "Interaction": ("Interaction", "Interacción", "Interaktion"),
    "Touches rémanentes": ("Sticky keys", "Teclas especiales", "Einrastfunktion"),
    "Appuyez sur Ctrl, Maj ou Alt l’une après l’autre au lieu de les maintenir.": ("Press Ctrl, Shift or Alt one at a time instead of holding them.", "Pulse Ctrl, Mayús o Alt de una en una en lugar de mantenerlas.", "Strg, Umschalt oder Alt nacheinander drücken statt gedrückt zu halten."),
    "Touches filtres": ("Filter keys", "Teclas filtro", "Anschlagverzögerung"),
    "Ignore les frappes trop brèves ou répétées.": ("Ignores brief or repeated keystrokes.", "Omite las pulsaciones breves o repetidas.", "Ignoriert kurze oder wiederholte Tastenanschläge."),
    "Narrateur (Orca)": ("Narrator (Orca)", "Narrador (Orca)", "Sprachausgabe (Orca)"),
    "Lit à voix haute ce qui est à l’écran.": ("Reads aloud what is on the screen.", "Lee en voz alta lo que aparece en pantalla.", "Liest vor, was auf dem Bildschirm steht."),
    "Le contraste et le pointeur s’appliquent aussi aux programmes Windows.": ("Contrast and pointer also apply to Windows programs.", "El contraste y el puntero también se aplican a los programas de Windows.", "Kontrast und Zeiger gelten auch für Windows-Programme."),
    # Effets visuels
    "Animer l’ouverture et la réduction des fenêtres et des menus.": ("Animate windows and menus opening and minimizing.", "Animar la apertura y minimización de ventanas y menús.", "Öffnen und Minimieren von Fenstern und Menüs animieren."),
    "Afficher le contenu des fenêtres pendant le redimensionnement": ("Show window contents while resizing", "Mostrar el contenido de las ventanas al cambiar el tamaño", "Fensterinhalt beim Ändern der Größe anzeigen"),
    "Lisser les polices de l’écran": ("Smooth edges of screen fonts", "Suavizar las fuentes de la pantalla", "Bildschirmschriftarten glätten"),
    "Rendu des polices": ("Font rendering", "Representación de fuentes", "Schriftdarstellung"),
    "Lissage léger": ("Slight hinting", "Suavizado ligero", "Leichte Glättung"),
    "Lissage complet": ("Full hinting", "Suavizado completo", "Volle Glättung"),
    "Sans ajustement": ("No hinting", "Sin ajuste", "Ohne Hinting"),
    # Programmes Windows
    "Programmes installés": ("Installed programs", "Programas instalados", "Installierte Programme"),
    "Aucun programme Windows installé pour l’instant. Double-cliquez sur un installateur .exe pour en ajouter un.": ("No Windows program installed yet. Double-click an .exe installer to add one.", "Todavía no hay ningún programa de Windows instalado. Haga doble clic en un instalador .exe para añadir uno.", "Noch kein Windows-Programm installiert. Doppelklicken Sie auf ein .exe-Installationsprogramm, um eines hinzuzufügen."),
    "Réglages d’un autre programme…": ("Settings for another program…", "Ajustes de otro programa…", "Einstellungen eines anderen Programms…"),
    "Programmes Windows (.exe)": ("Windows programs (.exe)", "Programas de Windows (.exe)", "Windows-Programme (.exe)"),
    "Échelle": ("Scale", "Escala", "Skalierung"),
    "Automatique (comme l’écran)": ("Automatic (same as screen)", "Automática (igual que la pantalla)", "Automatisch (wie Bildschirm)"),
    "Carte graphique": ("Graphics card", "Tarjeta gráfica", "Grafikkarte"),
    "GPU économie d’énergie": ("Power-saving GPU", "GPU de ahorro de energía", "Energiespar-GPU"),
    "GPU hautes performances": ("High-performance GPU", "GPU de alto rendimiento", "Hochleistungs-GPU"),
    "Journal de diagnostic": ("Diagnostic log", "Registro de diagnóstico", "Diagnoseprotokoll"),
    "Lancer": ("Run", "Ejecutar", "Starten"),
    "Ouvrir l’emplacement": ("Open file location", "Abrir ubicación", "Dateipfad öffnen"),
    "Installer ou désinstaller des programmes": ("Install or uninstall programs", "Instalar o desinstalar programas", "Programme installieren oder deinstallieren"),
    "Enregistrement impossible : le dossier du programme doit être accessible en écriture.": ("Unable to save: the program folder must be writable.", "No se puede guardar: la carpeta del programa debe admitir escritura.", "Speichern nicht möglich: Der Programmordner muss beschreibbar sein."),
    "Les réglages s’appliquent au prochain lancement du programme.": ("Settings apply the next time the program starts.", "Los ajustes se aplican la próxima vez que se inicie el programa.", "Die Einstellungen gelten beim nächsten Programmstart."),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


HOME = Path.home()
RUN = Path(os.environ.get("WINLUX_RUN_DIR", "/run/winlux"))
CONTROL = RUN / "control.txt"
A11Y_FILE = HOME / ".local/share/winlux/accessibility.ini"
APPEARANCE = HOME / ".config/winlux/appearance.json"
WALLPAPERS = Path(os.environ.get("WINLUX_WALLPAPERS", "/usr/share/winlux"))
EXPLORER_ADVANCED = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced"
EXPLORER_WINLUX = r"HKCU\Software\Winlux\Explorer"

PAGE_PROGRAMS, PAGE_ACCESSIBILITY, PAGE_EFFECTS, PAGE_LOCALE, PAGE_FILES = 10, 12, 13, 14, 15
PAGE_SOUND, PAGE_SYSUPDATE, PAGE_NETDRIVES, PAGE_DESKTOP, PAGE_BLUETOOTH = 16, 17, 20, 21, 22

# Mêmes valeurs que la page PE (contraste, pointeur) : accessibility.ini est lu
# par le runtime pour les programmes Windows.
CONTRASTS = ("Aucun", "Aquatique", "Désert", "Crépuscule", "Ciel nocturne")
# Désert est clair, les trois autres thèmes sont sombres.
CONTRAST_GTK = (None, "HighContrastInverse", "HighContrast", "HighContrastInverse", "HighContrastInverse")
POINTER_SIZES = (24, 32, 48, 64, 96)
LAYOUTS = (("fr", "Français (AZERTY)"), ("be", "Belge (AZERTY)"), ("us", "Anglais (États-Unis, QWERTY)"),
           ("gb", "Anglais (Royaume-Uni)"), ("de", "Allemand (QWERTZ)"), ("es", "Espagnol"),
           ("it", "Italien"), ("pt", "Portugais"))
LANGUAGES = (("fr", "Français"), ("en", "English"), ("es", "Español"), ("de", "Deutsch"))
VIEWS = ("Très grandes icônes", "Grandes icônes", "Icônes moyennes", "Petites icônes", "Liste", "Détails", "Mosaïques")
DAYS = {"mon": "lundi", "tue": "mardi", "wed": "mercredi", "thu": "jeudi", "fri": "vendredi", "sat": "samedi", "sun": "dimanche"}
PROTOCOLS = {"ftp": "FTP", "ftps": "FTPS", "sftp": "SFTP (SSH)", "webdav": "WebDAV", "webdavs": "WebDAV TLS",
             "smb": "SMB", "nfs": "NFS", "s3": "S3"}
DRIVE_STATES = {"ok": "En ligne", "offline": "Hors ligne", "connecting": "Connexion…", "password": "Mot de passe requis",
                "disconnected": "Hors ligne"}
BT_KINDS = {"audio": "Audio", "clavier": "Clavier", "souris": "Souris", "manette": "Manette",
            "telephone": "Téléphone", "ordinateur": "Ordinateur"}
ACCENTS = ("#8178b7", "#0078d4", "#0099bc", "#00b294", "#10893e", "#ca5010", "#e81123", "#c239b3")
EXE_SCALES = (("auto", "Automatique (comme l’écran)"), ("1", "100 %"), ("1.25", "125 %"), ("1.5", "150 %"), ("2", "200 %"))
EXE_GPUS = (("auto", "Automatique"), ("integrated", "GPU économie d’énergie"), ("dedicated", "GPU hautes performances"))


def read(path):
    try: return Path(path).read_text(errors="replace")
    except OSError: return ""


def keyvals(text):
    return dict(line.split("=", 1) for line in text.splitlines() if "=" in line)


def have(command):
    return shutil.which(command) is not None


def run(args, timeout=5):
    """Commande courte et silencieuse ; None si elle est absente ou échoue."""
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.SubprocessError):
        return None
    return result if result.returncode == 0 else None


# ── xsettings (xfsettingsd) et Openbox ──────────────────────────────────────

def xfconf_get(channel, prop):
    result = run(["xfconf-query", "-c", channel, "-p", prop], timeout=2) if have("xfconf-query") else None
    return result.stdout.strip() if result else None


def xfconf_set(channel, prop, kind, value):
    if not have("xfconf-query"): return False
    args = ["xfconf-query", "-c", channel, "-p", prop]
    create = [] if run(args, timeout=2) else ["-n", "-t", kind]
    return run(args + create + ["-s", str(value)], timeout=2) is not None


OPENBOX_NS = "http://openbox.org/3.4/rc"
OPENBOX_RC = HOME / ".config/openbox/rc.xml"


def openbox_get(section, option):
    try: root = ET.parse(OPENBOX_RC).getroot()
    except (OSError, ET.ParseError): return None
    node = root.find(f"{{{OPENBOX_NS}}}{section}/{{{OPENBOX_NS}}}{option}")
    return None if node is None else (node.text or "").strip()


def openbox_set(section, option, value):
    """Réglage <section><option> de rc.xml puis rechargement d’Openbox."""
    ET.register_namespace("", OPENBOX_NS)
    try: tree = ET.parse(OPENBOX_RC)
    except (OSError, ET.ParseError): return False
    parent = tree.getroot().find(f"{{{OPENBOX_NS}}}{section}")
    if parent is None: parent = ET.SubElement(tree.getroot(), f"{{{OPENBOX_NS}}}{section}")
    node = parent.find(f"{{{OPENBOX_NS}}}{option}")
    if node is None: node = ET.SubElement(parent, f"{{{OPENBOX_NS}}}{option}")
    node.text = value
    temporary = OPENBOX_RC.with_suffix(".xml.tmp")
    try:
        tree.write(temporary, encoding="utf-8", xml_declaration=True)
        temporary.replace(OPENBOX_RC)
    except OSError: return False
    if have("openbox"): run(["openbox", "--reconfigure"], timeout=3)
    return True


# ── Accessibilité partagée avec les programmes Windows ──────────────────────

def a11y_text():
    """Octets du fichier conservés tels quels (le bureau PE l’écrit en Windows-1252)."""
    try: return A11Y_FILE.read_text(encoding="latin-1")
    except OSError: return ""


def a11y_read():
    values = {}
    for line in a11y_text().splitlines():
        key, sep, value = line.partition("=")
        if sep and value.strip().isdigit(): values[key.strip()] = int(value.strip())
    return values


def a11y_write(**changes):
    """Met à jour accessibility.ini sans toucher aux autres réglages (loupe, narrateur…)."""
    lines = a11y_text().splitlines() or ["# Winlux - Param\xe8tres > Accessibilit\xe9"]
    done = set()
    for index, line in enumerate(lines):
        key = line.partition("=")[0].strip()
        if key in changes:
            lines[index] = f"{key}={int(changes[key])}"
            done.add(key)
    lines += [f"{key}={int(value)}" for key, value in changes.items() if key not in done]
    try:
        A11Y_FILE.parent.mkdir(parents=True, exist_ok=True)
        temporary = A11Y_FILE.with_suffix(".tmp")
        temporary.write_text("\n".join(lines) + "\n", encoding="latin-1", errors="replace")
        temporary.replace(A11Y_FILE)
    except OSError: return False
    return True


def appearance():
    try: return json.loads(APPEARANCE.read_text())
    except (OSError, ValueError): return {"theme": "dark", "accent": "#8178b7"}


# ── Registre Windows (reg.exe intégré au runtime) ───────────────────────────

def reg_tool():
    if have("winlux"): return ["winlux", "--reg"]
    return None


def reg_dword(key, name):
    tool = reg_tool()
    result = run(tool + ["query", key, "/v", name], timeout=3) if tool else None
    if not result: return None
    for line in result.stdout.splitlines():
        parts = line.split()
        if len(parts) >= 3 and parts[0].lower() == name.lower() and parts[1] == "REG_DWORD":
            try: return int(parts[2], 0)
            except ValueError: return None
    return None


def reg_set_dword(key, name, value):
    tool = reg_tool()
    return bool(tool) and run(tool + ["add", key, "/v", name, "/t", "REG_DWORD", "/d", str(int(value)), "/f"], timeout=3) is not None


# ── Son (PipeWire via pactl) ────────────────────────────────────────────────

def pactl_list(kind):
    result = run(["pactl", "-f", "json", "list", kind], timeout=2) if have("pactl") else None
    if not result: return []
    try: items = json.loads(result.stdout or "[]")
    except ValueError: return []
    return [item for item in items if isinstance(item, dict)]


def pactl_default(kind):
    result = run(["pactl", f"get-default-{kind}"], timeout=2) if have("pactl") else None
    return result.stdout.strip() if result else ""


def volume_percent(item):
    values = []
    for channel in (item.get("volume") or {}).values():
        try: values.append(int(str(channel.get("value_percent", "0")).rstrip("%")))
        except (AttributeError, ValueError): pass
    return max(values) if values else 0


def audio_devices():
    sinks = pactl_list("sinks")
    sources = [s for s in pactl_list("sources")
               if not str(s.get("name", "")).endswith(".monitor") and s.get("monitor_of_sink") in (None, "", "n/a")]
    return sinks, sources


# ── Lecteurs réseau (winlux-drives) ─────────────────────────────────────────

def drives_tool():
    if have("winlux-drives"): return ["winlux-drives"]
    if have("winlux"): return ["winlux", "--drives"]
    return None


def drives_dir():
    """Même dossier d’état que le runtime (mounts::state_dir)."""
    if os.environ.get("WINLUX_STATE_DIR"): return Path(os.environ["WINLUX_STATE_DIR"])
    if os.access(RUN, os.W_OK): return RUN
    if os.environ.get("XDG_RUNTIME_DIR"): return Path(os.environ["XDG_RUNTIME_DIR"]) / "winlux"
    return RUN


def drives_status(refresh=False):
    tool = drives_tool()
    if tool and (refresh or not (drives_dir() / "netdrives.txt").is_file()):
        run(tool + ["status"], timeout=4)
    return keyvals(read(drives_dir() / "netdrives.txt"))


# ── Programmes Windows ──────────────────────────────────────────────────────

def catalogue():
    """winlux-applications (registre Uninstall, raccourcis .lnk, icônes PE)."""
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve().parent
    for candidate in [os.environ.get("WINLUX_APPLICATIONS", ""), "/usr/bin/winlux-applications",
                      str(here / "winlux-applications.py")]:
        if candidate and Path(candidate).is_file():
            try:
                loader = SourceFileLoader("winlux_applications", candidate)
                spec = importlib.util.spec_from_loader("winlux_applications", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module
            except Exception:  # catalogue facultatif : la page reste utilisable
                return None
    return None


def installed_programs(module):
    if module is None: return []
    found = {}
    try: entries = list(module.installed_registry(HOME)) + [(t, n) for t, n, _ in module.shortcuts(HOME)]
    except Exception: entries = []
    for target, name in entries:
        try: key = str(Path(target).resolve())
        except OSError: continue
        found.setdefault(key, (Path(target), name))
    return sorted(found.values(), key=lambda item: item[1].casefold())


def profile_path(exe):
    return Path(str(exe) + ".winlux.ini")


def profile_read(exe):
    return keyvals(read(profile_path(exe)))


def profile_write(exe, **changes):
    """Réglages propres au .exe, en gardant ceux d’autres pages (processeur…)."""
    path = profile_path(exe)
    lines = [line for line in read(path).splitlines() if line.partition("=")[0] not in changes]
    for key, value in changes.items():
        if value not in (None, ""): lines.append(f"{key}={value}")
    try:
        if lines: path.write_text("\n".join(lines) + "\n")
        elif path.exists(): path.unlink()
    except OSError: return False
    return True


def gpu_hybrid():
    result = run(["winlux", "--gpu-status"], timeout=3) if have("winlux") else None
    return bool(result) and keyvals(result.stdout).get("gpu_hybrid") == "1"


def pixbuf_file(path, width, height):
    try: return GdkPixbuf.Pixbuf.new_from_file_at_scale(str(path), width, height, True)
    except GLib.Error: return None


def pe_pixbuf(module, exe, size=32):
    if module is None: return None
    try:
        data = module.pe_icon(exe)
        if not data: return None
        loader = GdkPixbuf.PixbufLoader.new_with_type("ico")
        loader.write(data)
        loader.close()
        pixbuf = loader.get_pixbuf()
        return pixbuf.scale_simple(size, size, GdkPixbuf.InterpType.BILINEAR) if pixbuf else None
    except Exception:
        return None


def wallpapers():
    found = []
    for name, title in (("wallpapers/winlux-prisme-nuit.png", "Winlux Nuit"),
                        ("wallpapers/winlux-prisme-aube.png", "Winlux Aube"), ("wallpaper.bmp", "Classique")):
        path = WALLPAPERS / name
        if path.is_file(): found.append((path, title))
    return found


# ── Disponibilité : une page sans contenu possible n’apparaît pas ───────────

def page_available(page, status=None):
    status = keyvals(read(CONTROL)) if status is None else status
    if page == PAGE_SOUND:
        sinks, sources = audio_devices()
        return bool(sinks or sources)
    if page == PAGE_SYSUPDATE:
        if status.get("sysupdate_apt") == "0": return False
        return have("winlux-sysupdate") and have("apt-get")
    if page == PAGE_NETDRIVES:
        return drives_tool() is not None and bool(drives_status().get("protocols"))
    if page == PAGE_FILES:
        return reg_tool() is not None
    if page == PAGE_BLUETOOTH:
        return keyvals(read(RUN / "bluetooth.txt")).get("present") == "1" or any(
            status.get(key) for key in ("gamepads", "cameras", "microphone", "input_devices"))
    if page == PAGE_DESKTOP:
        return have("winlux-personalize")
    return True


class ExtraPages:
    """Mélangé à la fenêtre Settings : self.body, self.execute, self.show_error."""

    EXTRA_PAGES = {PAGE_PROGRAMS: "page_programs", PAGE_ACCESSIBILITY: "page_accessibility",
                   PAGE_EFFECTS: "page_effects", PAGE_LOCALE: "page_locale", PAGE_FILES: "page_files",
                   PAGE_SOUND: "page_sound", PAGE_SYSUPDATE: "page_sysupdate", PAGE_NETDRIVES: "page_netdrives",
                   PAGE_DESKTOP: "page_desktop", PAGE_BLUETOOTH: "page_bluetooth"}

    def extra_page(self, page):
        """Construit la page ; False si elle n’est pas gérée ici."""
        name = self.EXTRA_PAGES.get(page)
        if not name: return False
        self.status = keyvals(read(CONTROL))
        getattr(self, name)()
        return True

    def extra_refresh(self):
        """Reconstruit la page après une action (état relu)."""
        if self.page in self.EXTRA_PAGES and not getattr(self, "_rebuilding", False):
            self._rebuilding = True
            try: self.select(self.nav, self.nav.get_selected_row())
            finally: self._rebuilding = False
        return False

    # ── Briques communes ──

    def section(self, title, text=None):
        label = Gtk.Label(label=title, xalign=0)
        label.get_style_context().add_class("section")
        self.body.pack_start(label, False, False, 0)
        if text: self.hint(text)
        return label

    def hint(self, text, parent=None):
        label = Gtk.Label(label=text, xalign=0, wrap=True)
        label.get_style_context().add_class("subtitle")
        (parent or self.body).pack_start(label, False, False, 0)
        return label

    def card(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        box.get_style_context().add_class("card")
        self.body.pack_start(box, False, False, 0)
        return box

    def row(self, parent, title, widget, subtitle=None):
        line = Gtk.Box(spacing=12)
        texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, valign=Gtk.Align.CENTER)
        texts.pack_start(Gtk.Label(label=title, xalign=0, wrap=True), False, False, 0)
        if subtitle: self.hint(subtitle, texts)
        line.pack_start(texts, True, True, 0)
        if widget is not None:
            widget.set_valign(Gtk.Align.CENTER)
            line.pack_start(widget, False, False, 0)
        parent.pack_start(line, False, False, 0)
        return line

    def switch(self, parent, title, active, changed, subtitle=None):
        widget = Gtk.Switch(active=bool(active))
        widget.connect("notify::active", lambda w, _: changed(w.get_active()))
        self.row(parent, title, widget, subtitle)
        return widget

    def combo(self, parent, title, items, active, changed, subtitle=None):
        widget = Gtk.ComboBoxText()
        for key, label in items: widget.append(str(key), label)
        if active is not None: widget.set_active_id(str(active))
        widget.connect("changed", lambda w: w.get_active_id() is not None and changed(w.get_active_id()))
        self.row(parent, title, widget, subtitle)
        return widget

    def actions(self, parent, buttons):
        line = Gtk.Box(spacing=8)
        for title, callback, *rest in buttons:
            button = Gtk.Button(label=title)
            button.set_sensitive(rest[0] if rest else True)
            button.connect("clicked", lambda *_, cb=callback: cb())
            line.pack_start(button, False, False, 0)
        parent.pack_start(line, False, False, 0)
        return line

    def spawn(self, args):
        try: subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        except OSError as error: self.show_error(_t("L’opération a échoué"), str(error))

    def tool(self, args, refresh=True):
        """Commande d’un réglage (winlux-personalize…) sans figer la fenêtre."""
        try:
            process = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        except OSError as error:
            self.show_error(_t("L’opération a échoué"), str(error))
            return

        def wait():
            if process.poll() is None: return True
            if process.returncode:
                self.show_error(_t("L’opération a échoué"), (process.stderr.read() or "").strip()[-2000:])
            if refresh: self.extra_refresh()
            return False
        GLib.timeout_add(200, wait)

    def browse_button(self, title, file_filter, chosen):
        """Bouton « Parcourir… » (le FileChooserButton de GTK affiche « (None) »)."""
        button = Gtk.Button(label=_t("Parcourir…"))

        def pick(*_):
            dialog = Gtk.FileChooserNative.new(title, self, Gtk.FileChooserAction.OPEN, None, None)
            dialog.add_filter(file_filter)
            if dialog.run() == Gtk.ResponseType.ACCEPT and dialog.get_filename(): chosen(dialog.get_filename())
            dialog.destroy()
        button.connect("clicked", pick)
        return button

    def confirm(self, text):
        dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL, text=text)
        answer = dialog.run()
        dialog.destroy()
        return answer == Gtk.ResponseType.OK

    # ── Bluetooth et appareils ──

    def page_bluetooth(self):
        bt = read(RUN / "bluetooth.txt")
        info = keyvals(bt)
        if info.get("present") == "1":
            box = self.card()
            powered = info.get("powered") == "1"
            state = _t("Recherche d’appareils en cours…") if info.get("discovering") == "1" else (
                _t("Visible pour les appareils à proximité") if powered else _t("Désactivé"))
            self.switch(box, _t("Bluetooth"), powered,
                        lambda on: self.execute("bluetooth-on" if on else "bluetooth-off"), state)
            self.actions(box, [(_t("Ajouter un appareil"), lambda: self.execute("bluetooth-scan"), powered)])
            devices = [line[len("device="):].split("|") for line in bt.splitlines() if line.startswith("device=")]
            if not devices and powered:
                self.hint(_t("Mettez l’appareil en mode appairage, puis lancez la recherche."), box)
            for fields in devices:
                if len(fields) < 5: continue
                mac, name, kind, paired, connected = fields[:5]
                status = _t("Connecté") if connected == "1" else (_t("Appairé") if paired == "1" else _t("Disponible"))
                line = Gtk.Box(spacing=8)
                line.pack_start(Gtk.Label(label=f"{name} · {_t(BT_KINDS.get(kind, 'Appareil'))} · {status}", xalign=0), True, True, 0)
                verb = "disconnect" if connected == "1" else ("connect" if paired == "1" else "pair")
                title = {"disconnect": "Déconnecter", "connect": "Connecter", "pair": "Appairer"}[verb]
                for label, action in [(title, verb)] + ([("Oublier", "remove")] if paired == "1" else []):
                    button = Gtk.Button(label=_t(label))
                    button.connect("clicked", lambda *_, a=action, m=mac: self.execute(f"bluetooth-{a}", m))
                    line.pack_start(button, False, False, 0)
                box.pack_start(line, False, False, 0)
            if have("blueman-manager"):
                self.actions(box, [(_t("Options Bluetooth avancées"), lambda: self.spawn(["blueman-manager"]))])
        self.section(_t("Appareils connectés à ce PC"), _t("Les programmes Windows (.exe) utilisent ces appareils directement."))
        box = self.card()
        status = self.status
        for title, key in (("Manettes de jeu", "gamepads"), ("Webcams", "cameras"),
                           ("Micro par défaut", "microphone"), ("Périphériques d’entrée", "input_devices")):
            value = status.get(key)
            if value: self.row(box, _t(title), Gtk.Label(label=value, xalign=1, selectable=True))

    # ── Personnalisation du bureau ──

    def page_desktop(self):
        prefs = appearance()
        self.section(_t("Arrière-plan"))
        flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=4, row_spacing=10, column_spacing=10)
        current = prefs.get("wallpaper", "")
        for path, title in wallpapers():
            button = Gtk.Button()
            inner = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
            pixbuf = pixbuf_file(path, 176, 99)
            if pixbuf: inner.pack_start(Gtk.Image.new_from_pixbuf(pixbuf), False, False, 0)
            inner.pack_start(Gtk.Label(label=("✓ " if str(path) == current else "") + _t(title)), False, False, 0)
            button.add(inner)
            button.connect("clicked", lambda *_, p=path: self.tool(["winlux-personalize", "wallpaper", str(p)]))
            flow.add(button)
        self.body.pack_start(flow, False, False, 0)
        images = Gtk.FileFilter()
        images.set_name(_t("Images"))
        images.add_pixbuf_formats()
        box = self.card()
        self.row(box, _t("Choisir une photo…"), self.browse_button(
            _t("Choisir une photo…"), images, lambda path: self.tool(["winlux-personalize", "wallpaper", path])))
        self.combo(box, _t("Mode de couleur"), [("dark", _t("Sombre")), ("light", _t("Clair"))], prefs.get("theme", "dark"),
                   lambda value: self.tool(["winlux-personalize", "theme", value]))
        self.section(_t("Couleur d’accentuation"))
        palette = Gtk.Box(spacing=8)
        for color in ACCENTS:
            swatch = Gtk.Button(label="✓" if prefs.get("accent", "").lower() == color else " ")
            swatch.set_size_request(44, 36)
            provider = Gtk.CssProvider()
            provider.load_from_data(f"button {{ background: {color}; color: #ffffff; border-color: {color}; }}".encode())
            swatch.get_style_context().add_provider(provider, Gtk.STYLE_PROVIDER_PRIORITY_USER + 2)
            swatch.connect("clicked", lambda *_, c=color: self.tool(["winlux-personalize", "accent", c]))
            palette.pack_start(swatch, False, False, 0)
        custom = Gtk.ColorButton(title=_t("Personnalisée…"))
        custom.connect("color-set", lambda w: self.tool(["winlux-personalize", "accent", "#%02x%02x%02x" % tuple(
            round(v * 255) for v in (w.get_rgba().red, w.get_rgba().green, w.get_rgba().blue))]))
        palette.pack_start(custom, False, False, 0)
        self.body.pack_start(palette, False, False, 0)
        self.section(_t("Barre des tâches"))
        box = self.card()
        sizes = [("32", _t("Petite")), ("40", _t("Moyenne")), ("48", _t("Grande")), ("56", _t("Très grande"))]
        size = xfconf_get("xfce4-panel", "/panels/panel-1/size") or prefs.get("panel-size", "40")
        self.combo(box, _t("Taille"), sizes, size, lambda value: self.tool(["winlux-personalize", "panel-size", value]))
        self.combo(box, _t("Alignement"), [("center", _t("Centré")), ("left", _t("À gauche"))],
                   self.status.get("align") or "center", lambda value: self.execute("align-" + value))
        self.row(box, _t("Icône du menu Démarrer"), self.browse_button(
            _t("Choisir une image…"), images, lambda path: self.tool(["winlux-personalize", "start-icon", path])))
        self.actions(box, [(_t("Icône Winlux"), lambda: self.tool(["winlux-personalize", "start-icon", "default"])),
                           (_t("Rétablir l’apparence par défaut"), lambda: self.tool(["winlux-personalize", "reset", "default"]))])

    # ── Lecteurs réseau ──

    def page_netdrives(self):
        status = drives_status(refresh=True)
        self.section(_t("Lecteurs connectés"))
        box = self.card()
        letters = status.get("drives", "")
        if not letters: self.hint(_t("Aucun lecteur réseau pour l’instant."), box)
        for letter in letters:
            fields = status.get(f"drive.{letter}", "").split("|")
            if len(fields) < 10: continue
            label, protocol, host, state, free = fields[0], fields[1], fields[2], fields[9], fields[10] if len(fields) > 10 else ""
            text = f"{letter}:  {label or host} · {PROTOCOLS.get(protocol, protocol)} · {_t(DRIVE_STATES.get(state, 'Hors ligne'))}"
            if free: text += " · " + _t("Libre : {free}").format(free=free)
            line = Gtk.Box(spacing=8)
            line.pack_start(Gtk.Label(label=text, xalign=0, ellipsize=3), True, True, 0)
            for title, verb in ((("Déconnecter", "disconnect") if state == "ok" else ("Connecter", "connect")), ("Supprimer", "remove")):
                button = Gtk.Button(label=_t(title))
                button.connect("clicked", lambda *_, v=verb, l=letter: self.drive_command(v, l))
                line.pack_start(button, False, False, 0)
            box.pack_start(line, False, False, 0)
        if status.get("message") and status.get("result"):
            self.hint(status["message"], box)
        self.section(_t("Connecter un lecteur réseau"), _t("Le mot de passe est chiffré et ne quitte jamais cet ordinateur."))
        box = self.card()
        offered = [p for p in status.get("protocols", "").split(",") if p in PROTOCOLS]
        self.drive_form = {}
        self.drive_form["protocol"] = self.combo(box, _t("Type de connexion"), [(p, PROTOCOLS[p]) for p in offered],
                                                 offered[0] if offered else None, lambda _: None)
        grid = Gtk.Grid(column_spacing=12, row_spacing=8)
        for index, (key, title, secret) in enumerate((("host", "Serveur", False), ("port", "Port (facultatif)", False),
                                                     ("user", "Nom d’utilisateur", False), ("password", "Mot de passe", True),
                                                     ("path", "Dossier distant", False), ("name", "Nom affiché", False))):
            grid.attach(Gtk.Label(label=_t(title), xalign=0), (index % 2) * 2, index // 2, 1, 1)
            entry = Gtk.Entry(hexpand=True, visibility=not secret)
            if secret: entry.set_input_purpose(Gtk.InputPurpose.PASSWORD)
            grid.attach(entry, (index % 2) * 2 + 1, index // 2, 1, 1)
            self.drive_form[key] = entry
        box.pack_start(grid, False, False, 0)
        used = set(letters)
        free_letters = [chr(c) for c in range(ord("Y"), ord("D") - 1, -1) if chr(c) not in used]
        self.drive_form["letter"] = self.combo(box, _t("Lettre"), [(l, l + ":") for l in free_letters],
                                               status.get("next_letter") or (free_letters[0] if free_letters else None), lambda _: None)
        auto = Gtk.CheckButton(label=_t("Se reconnecter à l’ouverture de session"), active=True)
        remember = Gtk.CheckButton(label=_t("Mémoriser le mot de passe"), active=True)
        box.pack_start(auto, False, False, 0)
        box.pack_start(remember, False, False, 0)
        self.drive_form["auto"], self.drive_form["remember"] = auto, remember
        self.actions(box, [(_t("Tester"), lambda: self.drive_submit("test")), (_t("Connecter"), lambda: self.drive_submit("add"))])

    def drive_submit(self, action):
        form = self.drive_form
        if not form["host"].get_text().strip():
            self.show_error(_t("L’opération a échoué"), _t("Indiquez l’adresse du serveur."))
            return
        values = {"letter": form["letter"].get_active_id() or "", "protocol": form["protocol"].get_active_id() or ""}
        for key in ("host", "port", "user", "password", "path", "name"):
            values[key] = form[key].get_text().replace("\n", " ").strip()
        values["auto"] = "1" if form["auto"].get_active() else "0"
        values["remember"] = "1" if form["remember"].get_active() else "0"
        request = drives_dir() / "netdrive-request"
        try:
            fd = os.open(request, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            with os.fdopen(fd, "w", encoding="cp1252", errors="replace") as stream:
                stream.write("".join(f"{key}={value}\n" for key, value in values.items()))
        except OSError as error:
            self.show_error(_t("L’opération a échoué"), str(error))
            return
        form["password"].set_text("")
        self.tool(drives_tool() + [action])

    def drive_command(self, verb, letter):
        if verb == "remove" and not self.confirm(_t("Supprimer le lecteur {letter}: ?").format(letter=letter)): return
        self.tool(drives_tool() + [verb, letter])

    # ── Mises à jour système (APT) ──

    def page_sysupdate(self):
        status = self.status
        self.hint(_t("Les mises à jour de Winlux lui-même sont dans « Mises à jour »."))
        self.section(_t("Logiciels et pilotes Linux"))
        box = self.card()
        busy = status.get("sysupdate_busy") == "1"
        try: count = int(status.get("sysupdate_count", "0"))
        except ValueError: count = 0
        last = status.get("sysupdate_last", "jamais")
        title = _t("Aucune recherche pour l’instant.") if last == "jamais" and not count else _t("Votre système est à jour.")
        self.row(box, title if not count else _t("{count} mise(s) à jour disponible(s)").format(count=count),
                 None, _t("Dernière recherche : {date}").format(date=_t(last) if last == "jamais" else last))
        packages = read(RUN / "sysupdates.txt").strip().splitlines()
        if count and packages:
            listing = Gtk.Label(label="\n".join(packages[:40]) + ("\n…" if len(packages) > 40 else ""), xalign=0, selectable=True)
            box.pack_start(listing, False, False, 0)
        if busy: self.hint(_t("Recherche ou installation en cours…"), box)
        self.actions(box, [(_t("Rechercher des mises à jour"), lambda: self.execute("sysupdate-check"), not busy)]
                     + ([(_t("Tout installer"), lambda: self.execute("sysupdate-apply"), not busy)] if count else []))
        self.section(_t("Installation des mises à jour"))
        box = self.card()
        mode = status.get("sysupdate_mode", "manual")
        self.combo(box, _t("Installation des mises à jour"), [("auto", _t("Automatique")), ("manual", _t("Manuelle (je choisis quand)")),
                                                             ("scheduled", _t("Planifiée"))], mode,
                   lambda value: value != mode and self.execute("sysupdate-mode-" + value))
        if mode == "scheduled":
            hour, day = status.get("sysupdate_hour", "3"), status.get("sysupdate_day", "daily")
            when = (_t("Chaque jour à {hour} h").format(hour=hour) if day not in DAYS
                    else _t("Chaque {day} à {hour} h").format(day=_t(DAYS[day]), hour=hour))
            self.row(box, when, None)
            self.actions(box, [(_t("Heure −"), lambda: self.execute("sysupdate-hour-down")),
                               (_t("Heure +"), lambda: self.execute("sysupdate-hour-up")),
                               (_t("Changer de jour"), lambda: self.execute("sysupdate-day-next"))])
        log = read(RUN / "sysupdate.log").strip()
        if log:
            self.section(_t("Journal"))
            self.body.pack_start(Gtk.Label(label=log[-4000:], xalign=0, wrap=True, selectable=True), False, False, 0)

    # ── Son avancé ──

    def page_sound(self):
        sinks, sources = audio_devices()
        for kind, title, devices, default in (("sink", "Sortie", sinks, pactl_default("sink")),
                                              ("source", "Entrée", sources, pactl_default("source"))):
            if not devices: continue
            self.section(_t(title))
            box = self.card()
            items = [(d.get("name", ""), d.get("description") or d.get("name", "")) for d in devices]
            self.combo(box, _t("Haut-parleurs ou casque") if kind == "sink" else _t("Micro"), items, default,
                       lambda name, k=kind: run(["pactl", f"set-default-{k}", name]) and self.audio_refresh())
            device = next((d for d in devices if d.get("name") == default), devices[0])
            self.volume_row(box, _t("Volume"), volume_percent(device),
                            lambda v, k=kind: run(["pactl", f"set-{k}-volume", f"@DEFAULT_{k.upper()}@", f"{v}%"]))
            self.switch(box, _t("Muet"), device.get("mute"),
                        lambda on, k=kind: run(["pactl", f"set-{k}-mute", f"@DEFAULT_{k.upper()}@", "1" if on else "0"]))
        self.section(_t("Mélangeur de volume"))
        box = self.card()
        streams = pactl_list("sink-inputs")
        if not streams: self.hint(_t("Aucune application ne joue de son pour l’instant."), box)
        for stream in streams:
            props = stream.get("properties") or {}
            name = props.get("application.name") or props.get("application.process.binary") or props.get("media.name") or "?"
            self.volume_row(box, name, volume_percent(stream),
                            lambda v, i=stream.get("index"): run(["pactl", "set-sink-input-volume", str(i), f"{v}%"]))

    def volume_row(self, parent, title, value, changed):
        scale = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 100, 1)
        scale.set_value(min(100, value))
        scale.set_size_request(260, -1)
        scale.set_value_pos(Gtk.PositionType.RIGHT)
        pending = {}

        def apply():
            pending.pop("id", None)
            changed(round(scale.get_value()))
            return False

        def moved(_):
            if "id" not in pending: pending["id"] = GLib.timeout_add(120, apply)
        scale.connect("value-changed", moved)
        self.row(parent, title, scale)

    def audio_refresh(self):
        GLib.idle_add(self.extra_refresh)
        return True

    # ── Gestionnaire de fichiers ──

    def page_files(self):
        self.section(_t("Explorateur de fichiers"), _t("S’applique aux prochaines fenêtres de l’Explorateur et aux programmes Windows."))
        box = self.card()
        hidden = reg_dword(EXPLORER_ADVANCED, "Hidden")
        hide_ext = reg_dword(EXPLORER_ADVANCED, "HideFileExt")
        self.switch(box, _t("Afficher les fichiers et dossiers cachés"), hidden == 1,
                    lambda on: self.reg_change(EXPLORER_ADVANCED, "Hidden", 1 if on else 2))
        self.switch(box, _t("Afficher les extensions des noms de fichiers"), hide_ext != 1,
                    lambda on: self.reg_change(EXPLORER_ADVANCED, "HideFileExt", 0 if on else 1))
        view = reg_dword(EXPLORER_WINLUX, "DefaultView")
        self.combo(box, _t("Affichage par défaut"), [(i, _t(v)) for i, v in enumerate(VIEWS)], 5 if view is None else view,
                   lambda value: self.reg_change(EXPLORER_WINLUX, "DefaultView", int(value)))
        self.actions(box, [(_t("Ouvrir l’Explorateur"), lambda: self.spawn(["winlux-shell-app", "explorer"]))])

    def reg_change(self, key, name, value):
        if not reg_set_dword(key, name, value):
            self.show_error(_t("L’opération a échoué"), f"{key}\\{name}")

    # ── Localisation ──

    def page_locale(self):
        status = self.status
        box = self.card()
        self.combo(box, _t("Langue de l’interface"), LANGUAGES, status.get("lang") or "fr",
                   lambda value: self.execute("lang-" + value), _t("S’applique aux fenêtres ouvertes ensuite."))
        self.combo(box, _t("Disposition du clavier"), [(k, _t(v)) for k, v in LAYOUTS], status.get("layout") or "fr",
                   lambda value: self.execute("keyboard-" + value))
        try:
            import zoneinfo
            zones = sorted(z for z in zoneinfo.available_timezones() if "/" in z and not z.startswith(("Etc/", "SystemV/", "posix/", "right/")))
        except Exception:
            zones = []
        current = status.get("timezone", "")
        if zones:
            zone = Gtk.ComboBoxText.new_with_entry()
            for name in zones: zone.append(name, name)
            zone.set_active_id(current) if current in zones else zone.get_child().set_text(current)
            completion = Gtk.EntryCompletion()
            model = Gtk.ListStore(str)
            for name in zones: model.append([name])
            completion.set_model(model)
            completion.set_text_column(0)
            completion.set_match_func(lambda c, key, it, m: key.casefold() in m[it][0].casefold(), model)
            zone.get_child().set_completion(completion)
            line = Gtk.Box(spacing=8)
            line.pack_start(zone, False, False, 0)
            apply = Gtk.Button(label=_t("Appliquer"))
            apply.connect("clicked", lambda *_: zone.get_child().get_text() in zones and zone.get_child().get_text() != current
                          and self.execute("tz-" + zone.get_child().get_text()))
            line.pack_start(apply, False, False, 0)
            self.row(box, _t("Fuseau horaire"), line)
        now = GLib.DateTime.new_now_local().format("%x %X")
        self.hint(_t("Date et heure actuelles : {now}").format(now=now), box)

    # ── Accessibilité ──

    def page_accessibility(self):
        values = a11y_read()
        self.hint(_t("Le contraste et le pointeur s’appliquent aussi aux programmes Windows."))
        self.section(_t("Vision"))
        box = self.card()
        self.combo(box, _t("Thème de contraste"), [(i, _t(n)) for i, n in enumerate(CONTRASTS)], values.get("contrast", 0),
                   lambda value: self.set_contrast(int(value)))
        self.combo(box, _t("Taille du pointeur"), [(i + 1, f"{size} px") for i, size in enumerate(POINTER_SIZES)],
                   values.get("pointer_size", 1), lambda value: self.set_pointer(int(value)))
        self.switch(box, _t("Effets d’animation"), values.get("animations", 1) == 1, self.set_animations)
        self.section(_t("Interaction"))
        box = self.card()
        self.switch(box, _t("Touches rémanentes"), values.get("sticky", 0) == 1,
                    lambda on: (a11y_write(sticky=on), xfconf_set("accessibility", "/StickyKeys", "bool", str(on).lower())),
                    _t("Appuyez sur Ctrl, Maj ou Alt l’une après l’autre au lieu de les maintenir."))
        self.switch(box, _t("Touches filtres"), values.get("filter", 0) == 1, self.set_filter_keys,
                    _t("Ignore les frappes trop brèves ou répétées."))
        if have("orca"):
            running = run(["pgrep", "-x", "orca"], timeout=2) is not None
            self.switch(box, _t("Narrateur (Orca)"), running,
                        lambda on: self.spawn(["orca", "--replace"]) if on else run(["pkill", "-x", "orca"]),
                        _t("Lit à voix haute ce qui est à l’écran."))

    def set_contrast(self, index):
        a11y_write(contrast=index)
        theme = CONTRAST_GTK[index] if 0 <= index < len(CONTRAST_GTK) else None
        if theme is None: theme = "Adwaita-dark" if appearance().get("theme", "dark") == "dark" else "Adwaita"
        xfconf_set("xsettings", "/Net/ThemeName", "string", theme)

    def set_pointer(self, size):
        a11y_write(pointer_size=size)
        xfconf_set("xsettings", "/Gtk/CursorThemeSize", "int", POINTER_SIZES[max(1, min(5, size)) - 1])

    def set_animations(self, on):
        a11y_write(animations=on)
        xfconf_set("xsettings", "/Gtk/EnableAnimations", "bool", str(on).lower())
        if openbox_get("theme", "animateIconify") is not None or OPENBOX_RC.is_file():
            openbox_set("theme", "animateIconify", "yes" if on else "no")

    def set_filter_keys(self, on):
        a11y_write(filter=on)
        for prop in ("/SlowKeys", "/BounceKeys"): xfconf_set("accessibility", prop, "bool", str(on).lower())

    # ── Effets visuels ──

    def page_effects(self):
        box = self.card()
        self.switch(box, _t("Effets d’animation"), a11y_read().get("animations", 1) == 1, self.set_animations,
                    _t("Animer l’ouverture et la réduction des fenêtres et des menus."))
        if OPENBOX_RC.is_file():
            self.switch(box, _t("Afficher le contenu des fenêtres pendant le redimensionnement"),
                        openbox_get("resize", "drawContents") != "no",
                        lambda on: openbox_set("resize", "drawContents", "yes" if on else "no"))
        antialias = xfconf_get("xsettings", "/Xft/Antialias")
        if antialias is not None or have("xfconf-query"):
            self.switch(box, _t("Lisser les polices de l’écran"), antialias != "0",
                        lambda on: xfconf_set("xsettings", "/Xft/Antialias", "int", 1 if on else 0))
            hint = xfconf_get("xsettings", "/Xft/HintStyle") or "hintslight"
            self.combo(box, _t("Rendu des polices"), [("hintslight", _t("Lissage léger")), ("hintfull", _t("Lissage complet")),
                                                      ("hintnone", _t("Sans ajustement"))], hint,
                       lambda value: xfconf_set("xsettings", "/Xft/HintStyle", "string", value))

    # ── Programmes Windows ──

    def page_programs(self):
        module = catalogue()
        programs = installed_programs(module)
        self.hybrid = gpu_hybrid()
        self.hint(_t("Les réglages s’appliquent au prochain lancement du programme."))
        self.actions(self.body, [(_t("Installer ou désinstaller des programmes"), lambda: self.spawn(["winlux-shell-app", "programs"]))])
        exes = Gtk.FileFilter()
        exes.set_name(_t("Programmes Windows (.exe)"))
        exes.add_pattern("*.exe")
        exes.add_pattern("*.EXE")
        self.row(self.body, _t("Réglages d’un autre programme…"), self.browse_button(
            _t("Réglages d’un autre programme…"), exes,
            lambda path: self.program_card(module, Path(path), Path(path).stem, first=True)))
        self.section(_t("Programmes installés"))
        self.programs_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        self.body.pack_start(self.programs_box, False, False, 0)
        if not programs:
            self.hint(_t("Aucun programme Windows installé pour l’instant. Double-cliquez sur un installateur .exe pour en ajouter un."), self.programs_box)
        for exe, name in programs:
            self.program_card(module, exe, name)

    def program_card(self, module, exe, name, first=False):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        box.get_style_context().add_class("card")
        head = Gtk.Box(spacing=12)
        pixbuf = pe_pixbuf(module, exe)
        if pixbuf: head.pack_start(Gtk.Image.new_from_pixbuf(pixbuf), False, False, 0)
        names = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        names.pack_start(Gtk.Label(label=name, xalign=0), False, False, 0)
        where = Gtk.Label(label=str(exe), xalign=0, ellipsize=3)
        where.get_style_context().add_class("subtitle")
        names.pack_start(where, False, False, 0)
        head.pack_start(names, True, True, 0)
        for title, args in ((_t("Lancer"), ["winlux-launch-exe", "desktop", str(exe)]),
                            (_t("Ouvrir l’emplacement"), ["winlux-shell-app", "explorer", str(exe.parent)])):
            button = Gtk.Button(label=title, valign=Gtk.Align.CENTER)
            button.connect("clicked", lambda *_, a=args: self.spawn(a))
            head.pack_start(button, False, False, 0)
        box.pack_start(head, False, False, 0)
        profile = profile_read(exe)
        self.combo(box, _t("Échelle"), [(k, _t(v)) for k, v in EXE_SCALES], profile.get("ui_scale", "auto"),
                   lambda value: self.save_program(exe, ui_scale=value))
        if self.hybrid:
            self.combo(box, _t("Carte graphique"), [(k, _t(v)) for k, v in EXE_GPUS], profile.get("gpu", "auto"),
                       lambda value: self.save_program(exe, gpu=None if value == "auto" else value))
        self.switch(box, _t("Journal de diagnostic"), profile.get("trace") == "1",
                    lambda on: self.save_program(exe, trace="1" if on else "0"))
        self.programs_box.pack_start(box, False, False, 0)
        if first: self.programs_box.reorder_child(box, 0)
        box.show_all()

    def save_program(self, exe, **changes):
        if not profile_write(exe, **changes):
            self.show_error(_t("L’opération a échoué"),
                            _t("Enregistrement impossible : le dossier du programme doit être accessible en écriture."))

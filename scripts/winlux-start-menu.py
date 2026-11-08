#!/usr/bin/python3
"""Windows 11 inspired native Start: real XDG apps, pins and context actions."""
import hashlib
import json
import os
import pwd
import re
from pathlib import Path
import shutil
from string import Template
import subprocess
import threading
import time
import unicodedata
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk, Gio, GLib


class FixedWidthBox(Gtk.Box):
    """Boîte dont la largeur est celle demandée par set_size_request, quel que
    soit son contenu : le volet « Pour vous » garde 320 ou 360 px même quand un
    nom d'application long (« Textverarbeitung ») voudrait l'élargir (#869).
    Les libellés se coupent alors avec « … » dans la place donnée."""

    def do_get_preferred_width(self):
        width = self.get_size_request()[0]
        if width <= 0:
            return Gtk.Box.do_get_preferred_width(self)
        return width, width


# ── Recherche : ni casse ni accents (« parametres » trouve « Paramètres ») ──

_LIGATURES = str.maketrans({"œ": "oe", "æ": "ae", "ø": "o", "đ": "d", "ł": "l", "ı": "i"})


def account_name():
    """Nom du compte affiché en bas du menu, comme sous Windows : le nom
    complet saisi à l'installation, sinon l'identifiant ; « Winlux » pour la
    session live."""
    try:
        entry = pwd.getpwuid(os.getuid())
    except KeyError:
        return "Winlux"
    if entry.pw_uid == 0:
        # Système installé : le bureau tourne encore sous root, le compte
        # créé à l'installation est noté dans install-config.json.
        try:
            config = json.loads(Path("/etc/winlux/install-config.json").read_text(encoding="utf-8"))
            return str(config.get("fullname") or config.get("username") or "Winlux")
        except (OSError, ValueError, AttributeError):
            return "Winlux"
    return entry.pw_gecos.split(",")[0].strip() or entry.pw_name


def fold(text):
    """Texte replié pour comparer : minuscules, sans accents ni cédilles,
    ß → ss, œ → oe. Vaut pour les quatre langues (fr, en, es, de)."""
    text = unicodedata.normalize("NFKD", text.casefold().translate(_LIGATURES))
    return "".join(c for c in text if not unicodedata.combining(c))


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
    'Système et alimentation': ('System and power', 'Sistema y energía', 'System und Energie'),
    'Applications et démarrage': ('Apps and startup', 'Aplicaciones e inicio', 'Apps und Autostart'),
    'Réseau Wi-Fi': ('Wi-Fi network', 'Red Wi-Fi', 'WLAN-Netzwerk'),
    'Luminosité écran': ('Screen brightness', 'Brillo de la pantalla', 'Bildschirmhelligkeit'),
    'Certificats sécurité': ('Security certificates', 'Certificados de seguridad', 'Sicherheitszertifikate'),
    'Date et heure': ('Date & time', 'Fecha y hora', 'Datum und Uhrzeit'),
    'Bluetooth et appareils': ('Bluetooth and devices', 'Bluetooth y dispositivos', 'Bluetooth und Geräte'),
    'Son': ('Sound', 'Sonido', 'Sound'),
    'Clavier tactile': ('Touch keyboard', 'Teclado táctil', 'Bildschirmtastatur'),
    'Alimentation batterie veille': ('Power battery sleep', 'Energía batería suspensión', 'Energie Akku Standby'),
    'Clavier': ('Keyboard', 'Teclado', 'Tastatur'),
    'Souris': ('Mouse', 'Ratón', 'Maus'),
    'Personnalisation fond écran thème': ('Personalization wallpaper theme', 'Personalización fondo pantalla tema', 'Personalisierung Hintergrund Design'),
    'Mises à jour': ('Updates', 'Actualizaciones', 'Updates'),
    'Mise à jour OS': ('OS update', 'Actualización del SO', 'OS-Update'),
    'Écran résolution': ('Display resolution', 'Pantalla resolución', 'Bildschirm Auflösung'),
    'À propos version': ('About version', 'Acerca de versión', 'Info Version'),
    'Navigateurs': ('Browsers', 'Navegadores', 'Browser'),
    'Boutiques de logiciels': ('Software stores', 'Tiendas de programas', 'Software-Stores'),
    'Partage de dossiers et transfert des réglages': ('Folder sharing and settings transfer', 'Compartir carpetas y transferir ajustes', 'Ordnerfreigabe und Übertragung der Einstellungen'),
    'Paramètres': ('Settings', 'Configuración', 'Einstellungen'),
    'Système Linux': ('Linux system', 'Sistema Linux', 'Linux-System'),
    'Applications installées': ('Installed apps', 'Aplicaciones instaladas', 'Installierte Apps'),
    'Gérer les applications': ('Manage apps', 'Administrar aplicaciones', 'Apps verwalten'),
    'Réseau': ('Network', 'Red', 'Netzwerk'),
    'Clavier et souris': ('Keyboard and mouse', 'Teclado y ratón', 'Tastatur und Maus'),
    'Écran': ('Display', 'Pantalla', 'Bildschirm'),
    'À propos': ('About', 'Acerca de', 'Info'),
    'Programmes Windows': ('Windows programs', 'Programas de Windows', 'Windows-Programme'),
    'Accessibilité': ('Accessibility', 'Accesibilidad', 'Barrierefreiheit'),
    'Effets visuels': ('Visual effects', 'Efectos visuales', 'Visuelle Effekte'),
    'Heure et langue': ('Time and language', 'Hora e idioma', 'Zeit und Sprache'),
    'Gestionnaire de fichiers': ('File manager', 'Administrador de archivos', 'Dateimanager'),
    'Mise à jour application': ('Application update', 'Actualización de aplicaciones', 'Anwendungsupdate'),
    'Confidentialité et sécurité': ('Privacy and security', 'Privacidad y seguridad', 'Datenschutz und Sicherheit'),
    'Processeur': ('Processor', 'Procesador', 'Prozessor'),
    'Lecteurs réseau': ('Network drives', 'Unidades de red', 'Netzlaufwerke'),
    'Fond d’écran et couleurs': ('Wallpaper and colours', 'Fondo y colores', 'Hintergrund und Farben'),
    'Bluetooth': ('Bluetooth', 'Bluetooth', 'Bluetooth'),
    'Alimentation et batterie': ('Power and battery', 'Energía y batería', 'Energie und Akku'),
    'Graphismes': ('Graphics', 'Gráficos', 'Grafik'),
    'Signaler un problème': ('Report a problem', 'Informar de un problema', 'Problem melden'),
    'Imprimantes et scanners': ('Printers and scanners', 'Impresoras y escáneres', 'Drucker und Scanner'),
    'Bureau à distance': ('Remote desktop', 'Escritorio remoto', 'Remotedesktop'),
    'Partage et transfert': ('Sharing and transfer', 'Compartir y transferir', 'Freigabe und Übertragung'),
    'Assistant IA': ('AI assistant', 'Asistente de IA', 'KI-Assistent'),
    'Récupération': ('Recovery', 'Recuperación', 'Wiederherstellung'),
    'Presse-papiers': ('Clipboard', 'Portapapeles', 'Zwischenablage'),
    'Comptes': ('Accounts', 'Cuentas', 'Konten'),
    'Matériel et pilotes': ('Hardware and drivers', 'Hardware y controladores', 'Hardware und Treiber'),
    'Wi-Fi': ('Wi-Fi', 'Wi-Fi', 'WLAN'),
    'Certificats': ('Certificates', 'Certificados', 'Zertifikate'),
    'Appareils mobiles': ('Mobile devices', 'Dispositivos móviles', 'Mobilgeräte'),
    'Sauvegarde': ('Backup', 'Copia de seguridad', 'Sicherung'),
    'VPN': ('VPN', 'VPN', 'VPN'),
    'Point d’accès sans fil mobile': ('Mobile hotspot', 'Zona con cobertura inalámbrica móvil', 'Mobiler Hotspot'),
    'Meilleur résultat': ('Best match', 'Mejor coincidencia', 'Höchste Übereinstimmung'),
    'Applications': ('Apps', 'Aplicaciones', 'Apps'),
    'Documents': ('Documents', 'Documentos', 'Dokumente'),
    'Ouvrir l’emplacement du fichier': ('Open file location', 'Abrir ubicación del archivo', 'Dateipfad öffnen'),
    'Copier le chemin': ('Copy path', 'Copiar ruta', 'Pfad kopieren'),
    'Rechercher': ('Search', 'Buscar', 'Suchen'),
    'Bureautique': ('Office', 'Oficina', 'Büro'),
    'Graphisme': ('Graphics', 'Gráficos', 'Grafik'),
    'Audio et vidéo': ('Audio and video', 'Audio y vídeo', 'Audio und Video'),
    'Jeux': ('Games', 'Juegos', 'Spiele'),
    'Jeux bibliothèque temps de jeu': ('Games library play time', 'Juegos biblioteca tiempo de juego', 'Spiele Bibliothek Spielzeit'),
    'Développement': ('Development', 'Desarrollo', 'Entwicklung'),
    'Système': ('System', 'Sistema', 'System'),
    'Accessoires': ('Accessories', 'Accesorios', 'Zubehör'),
    'Autres applications': ('Other apps', 'Otras aplicaciones', 'Weitere Apps'),
    'Démarrer Winlux': ('Winlux Start', 'Inicio de Winlux', 'Winlux-Start'),
    'Applications, fichiers et paramètres': ('Apps, files and settings', 'Aplicaciones, archivos y configuración', 'Apps, Dateien und Einstellungen'),
    'Épinglé': ('Pinned', 'Anclado', 'Angeheftet'),
    'Toutes les applications  ›': ('All apps  ›', 'Todas las aplicaciones  ›', 'Alle Apps  ›'),
    'Options d’alimentation': ('Power options', 'Opciones de energía', 'Energieoptionen'),
    'Impossible d’ouvrir l’application': ("Couldn't open the app", 'No se puede abrir la aplicación', 'Die App konnte nicht geöffnet werden'),
    'Résultats': ('Results', 'Resultados', 'Ergebnisse'),
    'Toutes les applications': ('All apps', 'Todas las aplicaciones', 'Alle Apps'),
    '‹  Retour': ('‹  Back', '‹  Volver', '‹  Zurück'),
    'Paramètres · ': ('Settings · ', 'Configuración · ', 'Einstellungen · '),
    'Fichier · ': ('File · ', 'Archivo · ', 'Datei · '),
    '30 premiers fichiers affichés. Précisez la recherche.': ('First 30 files shown. Refine your search.', 'Se muestran los 30 primeros archivos. Precise la búsqueda.', 'Die ersten 30 Dateien werden angezeigt. Verfeinern Sie die Suche.'),
    'Aucune application trouvée': ('No apps found', 'No se encontró ninguna aplicación', 'Keine Apps gefunden'),
    'Recommandations': ('Recommended', 'Recomendaciones', 'Empfohlen'),
    'Vos applications récemment ouvertes apparaîtront ici.': ('Your recently opened apps will show up here.', 'Sus aplicaciones abiertas recientemente aparecerán aquí.', 'Ihre zuletzt geöffneten Apps werden hier angezeigt.'),
    'Impossible d’ouvrir ce fichier': ("Couldn't open this file", 'No se puede abrir este archivo', 'Diese Datei konnte nicht geöffnet werden'),
    'Ouvrir': ('Open', 'Abrir', 'Öffnen'),
    'Détacher du menu Démarrer': ('Unpin from Start', 'Desanclar de Inicio', 'Von Start lösen'),
    'Épingler au menu Démarrer': ('Pin to Start', 'Anclar a Inicio', 'An Start anheften'),
    # Réordonner les épinglés (#925) : sous-menu « Déplacer » et glisser-déposer.
    'Déplacer': ('Move', 'Mover', 'Verschieben'),
    'Au début': ('To the front', 'Al principio', 'An den Anfang'),
    'Vers la gauche': ('Left', 'A la izquierda', 'Nach links'),
    'Vers la droite': ('Right', 'A la derecha', 'Nach rechts'),
    'À la fin': ('To the end', 'Al final', 'An das Ende'),
    'Créer un raccourci sur le bureau': ('Create desktop shortcut', 'Crear acceso directo en el escritorio', 'Desktopverknüpfung erstellen'),
    'Redémarrer': ('Restart', 'Reiniciar', 'Neu starten'),
    'Verrouiller': ('Lock', 'Bloquear', 'Sperren'),
    'Se déconnecter': ('Sign out', 'Cerrar sesión', 'Abmelden'),
    'Changer d’utilisateur': ('Switch user', 'Cambiar de usuario', 'Benutzer wechseln'),
    'Mettre en veille': ('Sleep', 'Suspender', 'Energie sparen'),
    'Arrêter': ('Shut down', 'Apagar', 'Herunterfahren'),
    # Menu à deux volets et volet « Pour vous » (#685).
    'Pour vous': ('For you', 'Para ti', 'Für Sie'),
    'Masquer': ('Hide', 'Ocultar', 'Ausblenden'),
    'Masquer le volet « Pour vous » (réactivable dans Paramètres)': ('Hide the “For you” pane (turn it back on in Settings)', 'Ocultar el panel «Para ti» (se reactiva en Configuración)', 'Bereich „Für Sie“ ausblenden (in den Einstellungen wieder einschaltbar)'),
    'Reprendre où vous en étiez': ('Pick up where you left off', 'Continuar donde lo dejó', 'Dort weitermachen, wo Sie aufgehört haben'),
    'Les fichiers que vous ouvrez apparaîtront ici.': ('Files you open will show up here.', 'Los archivos que abra aparecerán aquí.', 'Geöffnete Dateien werden hier angezeigt.'),
    'Souvent ouvert à cette heure-ci': ('Often opened around this time', 'Abierto a menudo a esta hora', 'Oft um diese Uhrzeit geöffnet'),
    'D’après vos habitudes {days}, {moment}': ('Based on your habits {days}, {moment}', 'Según sus costumbres {days}, {moment}', 'Nach Ihren Gewohnheiten {days}, {moment}'),
    'en semaine': ('on weekdays', 'entre semana', 'unter der Woche'),
    'le week-end': ('at weekends', 'el fin de semana', 'am Wochenende'),
    'le matin': ('in the morning', 'por la mañana', 'morgens'),
    'l’après-midi': ('in the afternoon', 'por la tarde', 'nachmittags'),
    'le soir': ('in the evening', 'por la noche', 'abends'),
    'la nuit': ('at night', 'de madrugada', 'nachts'),
    'Winlux apprend vos habitudes au fil des jours, sans rien envoyer.': ('Winlux learns your habits over the days, without sending anything.', 'Winlux aprende sus costumbres con los días, sin enviar nada.', 'Winlux lernt Ihre Gewohnheiten mit der Zeit, ohne etwas zu senden.'),
    'À faire': ('To do', 'Pendiente', 'Zu erledigen'),
    'Installer': ('Install', 'Instalar', 'Installieren'),
    'Voir': ('View', 'Ver', 'Anzeigen'),
    'Trier': ('Tidy up', 'Ordenar', 'Aufräumen'),
    'Ignorer': ('Dismiss', 'Descartar', 'Ignorieren'),
    'Pas encore installé': ('Not installed yet', 'Aún no instalado', 'Noch nicht installiert'),
    'Winlux {version} est disponible': ('Winlux {version} is available', 'Winlux {version} está disponible', 'Winlux {version} ist verfügbar'),
    'Mise à jour du système prête': ('System update ready', 'Actualización del sistema lista', 'Systemupdate bereit'),
    'Téléchargements : {size}': ('Downloads: {size}', 'Descargas: {size}', 'Downloads: {size}'),
    '1 fichier non ouvert depuis 3 mois': ('1 file untouched for 3 months', '1 archivo sin abrir desde hace 3 meses', '1 Datei seit 3 Monaten nicht geöffnet'),
    '{n} fichiers non ouverts depuis 3 mois': ('{n} files untouched for 3 months', '{n} archivos sin abrir desde hace 3 meses', '{n} Dateien seit 3 Monaten nicht geöffnet'),
    'Calculé sur ce PC à partir de votre usage, rien n’est envoyé. Désactivable dans Paramètres.': ('Worked out on this PC from your usage, nothing is sent. Can be turned off in Settings.', 'Calculado en este PC a partir de su uso, no se envía nada. Se puede desactivar en Configuración.', 'Auf diesem PC aus Ihrer Nutzung berechnet, nichts wird gesendet. In den Einstellungen abschaltbar.'),
    'à l’instant': ('just now', 'ahora mismo', 'gerade eben'),
    'il y a {n} min': ('{n} min ago', 'hace {n} min', 'vor {n} Min.'),
    'il y a {n} h': ('{n} h ago', 'hace {n} h', 'vor {n} Std.'),
    'hier': ('yesterday', 'ayer', 'gestern'),
    'il y a {n} jours': ('{n} days ago', 'hace {n} días', 'vor {n} Tagen'),
    'Utilisées souvent': ('Frequently used', 'Usadas a menudo', 'Häufig verwendet'),
    'Ajoutées récemment': ('Recently added', 'Agregadas recientemente', 'Kürzlich hinzugefügt'),
    'Catégories': ('Categories', 'Categorías', 'Kategorien'),
    'Grille': ('Grid', 'Cuadrícula', 'Raster'),
    'Liste A–Z': ('A–Z list', 'Lista A–Z', 'A–Z-Liste'),
    '‹  Catégories': ('‹  Categories', '‹  Categorías', '‹  Kategorien'),
    'Explorateur': ('File Explorer', 'Explorador', 'Explorer'),
    'Internet': ('Internet', 'Internet', 'Internet'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


STATE = Path.home() / ".config/winlux/start.json"
DEFAULTS = ["winlux-explorer.desktop", "winlux-browser.desktop", "winlux-store.desktop", "org.xfce.mousepad.desktop",
            "winlux-settings.desktop", "winlux-terminal.desktop", "winlux-updates.desktop"]
# Pages des Paramètres trouvées par la recherche (#461) : titre de la page et
# mots-clés dans les quatre langues, quelle que soit la langue du bureau
# (« wifi », « printer », « Drucker », « impresora »…). Pages 3, 5 et 6 :
# fondues dans 16, 4 et 21.
SETTINGS_INDEX = [
    (0, "Système Linux", "systeme linux noyau kernel services system sistema núcleo Kernel Dienste"),
    (39, "Applications installées", "applications installees logiciels installes programmes desinstaller supprimer taille liste installed apps programs uninstall remove size aplicaciones instaladas programas desinstalar installierte Apps Programme deinstallieren entfernen"),
    (1, "Gérer les applications", "applications apps gerer demarrage boutiques logiciels par defaut installer manage startup stores software aplicaciones administrar inicio tiendas verwalten Autostart"),
    (2, "Réseau", "reseau ethernet cable proxy ip dns internet network red Netzwerk Kabel"),
    (4, "Clavier et souris", "clavier souris azerty qwerty disposition pave tactile touchpad pointeur keyboard mouse layout teclado raton Tastatur Maus Layout"),
    (7, "Mise à jour OS", "mise a jour os mises a jour winlux systeme exploitation os update operating system actualizacion del so sistema operativo Betriebssystem OS-Update update updates actualizaciones actualizar Aktualisierung Updates"),
    (8, "Écran", "ecran affichage resolution luminosite echelle orientation projeter second ecran moniteur display brightness scale monitor project pantalla brillo escala Bildschirm Anzeige Helligkeit Auflosung Skalierung"),
    (9, "À propos", "a propos version nom du pc informations systeme about pc name specifications acerca de version Info Version Gerätename"),
    (10, "Programmes Windows", "programmes windows exe compatibilite windows programs compatibility programas Windows-Programme Kompatibilitat"),
    (11, "Navigateurs", "navigateur navigateurs par defaut firefox chrome browser default navegador predeterminado Browser Standard"),
    (12, "Accessibilité", "accessibilite loupe contraste narrateur taille du texte accessibility magnifier contrast narrator text size accesibilidad lupa contraste narrador Barrierefreiheit Lupe Kontrast Sprachausgabe"),
    (13, "Effets visuels", "effets visuels animations transparence visual effects transparency efectos visuales animaciones transparencia visuelle Effekte Animationen Transparenz"),
    (14, "Heure et langue", "langue region date heure fuseau horaire format clavier language time zone date idioma region fecha hora zona horaria Sprache Region Datum Uhrzeit Zeitzone"),
    (15, "Gestionnaire de fichiers", "fichiers explorateur extensions fichiers caches dossiers file explorer hidden files folders archivos explorador ocultos Dateien Explorer versteckte"),
    (16, "Son", "son volume haut parleurs micro microphone casque audio sound speakers headphones sonido volumen altavoces microfono auriculares Ton Lautstarke Lautsprecher Mikrofon Kopfhorer"),
    (17, "Mise à jour application", "mise a jour application applications logiciels mises a jour systeme paquets pilotes noyau app application update updates system packages apps actualizacion de aplicaciones actualizaciones del sistema paquetes Anwendungsupdate Anwendungen Systemaktualisierungen Pakete"),
    (18, "Confidentialité et sécurité", "confidentialite securite verrouillage mot de passe autorisations camera micro pare feu privacy security lock permissions firewall privacidad seguridad bloqueo permisos Datenschutz Sicherheit Sperre Berechtigungen Firewall"),
    (19, "Processeur", "processeur cpu performances frequence processor performance procesador rendimiento Prozessor Leistung"),
    (20, "Lecteurs réseau", "lecteurs reseau partage samba smb nas sftp ftp network drives share unidades de red Netzlaufwerke Freigabe"),
    (21, "Fond d’écran et couleurs", "personnalisation fond d ecran theme sombre clair couleurs accent personalization wallpaper background dark light colors personalizacion fondo tema oscuro claro colores Personalisierung Hintergrund dunkel hell Farben menu demarrer pour vous recommandations start menu for you recommended menu inicio para ti recomendaciones Startmenu Empfehlungen"),
    (22, "Bluetooth", "bluetooth appareils casque souris sans fil devices headset dispositivos auriculares Gerate Kopfhorer"),
    (23, "Alimentation et batterie", "alimentation batterie veille economie d energie mode power battery sleep saver energia bateria suspension ahorro Energie Akku Standby Energiesparen"),
    (24, "Graphismes", "graphismes carte graphique gpu nvidia amd intel hybride graphics card graficos tarjeta grafica Grafik Grafikkarte"),
    (25, "Signaler un problème", "signaler un probleme bug plantage rapport diagnostic report problem crash informar problema error Problem melden Absturz Fehler"),
    (26, "Imprimantes et scanners", "imprimantes imprimante scanner numeriser printers printer scan impresoras impresora escaner Drucker Scanner"),
    (27, "Bureau à distance", "bureau a distance rdp vnc remote desktop escritorio remoto Remotedesktop"),
    (28, "Partage et transfert", "partage transfert dossiers partages transfert des reglages sharing transfer shared folders compartir transferir carpetas compartidas Freigabe Ubertragung freigegebene Ordner"),
    (29, "Jeux", "jeux bibliotheque manette temps de jeu games gaming gamepad library juegos mando biblioteca Spiele Controller Bibliothek"),
    (30, "Assistant IA", "assistant ia intelligence artificielle claude chatgpt ai assistant asistente ia KI Assistent"),
    (31, "Récupération", "recuperation reinitialiser restaurer recovery reset restore recuperacion restablecer restaurar Wiederherstellung zurucksetzen"),
    (32, "Presse-papiers", "presse papiers historique copier coller clipboard history portapapeles historial Zwischenablage Verlauf"),
    (33, "Comptes", "comptes compte utilisateur utilisateurs mot de passe connexion famille accounts account user password sign in cuentas cuenta usuario contrasena Konten Konto Benutzer Kennwort Anmeldung"),
    (34, "Matériel et pilotes", "materiel pilotes peripheriques gestionnaire de peripheriques hardware drivers devices device manager controladores hardware administrador de dispositivos Treiber Hardware Gerate-Manager"),
    (35, "Wi-Fi", "wifi wi fi sans fil reseau sans fil wireless wlan inalambrica red WLAN Drahtlos"),
    (40, "VPN", "vpn reseau prive virtuel wireguard openvpn tunnel entreprise virtual private network red privada virtual virtuelles privates Netzwerk"),
    (41, "Point d’accès sans fil mobile", "point d acces sans fil mobile partage de connexion hotspot tethering partager internet mobile hotspot share internet zona con cobertura compartir conexion mobiler Hotspot Internetfreigabe Tethering"),
    (36, "Certificats", "certificats certificat securite ssl tls certificates certificate certificados certificado Zertifikate Zertifikat"),
    (37, "Appareils mobiles", "appareils mobiles telephone smartphone android mobile devices phone dispositivos moviles telefono movil Mobilgerate Telefon Handy Smartphone"),
    (38, "Sauvegarde", "sauvegarde sauvegarder documents copie de securite disque externe restaurer backup back up external drive restore copia de seguridad disco externo restaurar Sicherung sichern externe Festplatte wiederherstellen"),
]
SETTINGS = [(_t(label), page) for page, label, _ in SETTINGS_INDEX]
# Une seule application Paramètres : les panneaux de réglages du bureau XFCE et
# les raccourcis vers une page des Paramètres ne s'affichent pas à part (la
# recherche trouve les pages ci-dessus).
SETTINGS_KEPT = {"winlux-settings.desktop", "winlux-updates.desktop"}


def listed(app):
    if not app.should_show() or not app.get_id(): return False
    if app.get_id() in SETTINGS_KEPT: return True
    categories = set((app.get_categories() or "").split(";")) if hasattr(app, "get_categories") else set()
    return "Settings" not in categories

def category(app):
    """Use installed desktop metadata, including generated Windows launchers."""
    if (app.get_id() or "").startswith("winlux-pe-"): return _t("Programmes Windows")
    categories = set((app.get_categories() or "").split(";")) if hasattr(app, "get_categories") else set()
    for key, label in [("Settings", _t("Paramètres")), ("Network", "Internet"),
                       ("Office", _t("Bureautique")), ("Graphics", _t("Graphisme")),
                       ("AudioVideo", _t("Audio et vidéo")), ("Game", _t("Jeux")),
                       ("Development", _t("Développement")), ("System", _t("Système")),
                       ("Utility", _t("Accessoires"))]:
        if key in categories: return label
    return _t("Autres applications")

# Services de la session, et Thunar : l'Explorateur Winlux le remplace (« par »
# proposait « Gestionnaire de fichiers Thunar » juste après Paramètres).
# « Gestionnaire de fichiers » d'exo (xfce4-file-manager) ouvrait lui aussi
# l'Explorateur : doublon retiré (#721), même si APT le réinstalle.
SESSION_SERVICES = {"picom.desktop", "thunar.desktop", "org.xfce.thunar.desktop", "Thunar.desktop",
                    "Thunar-folder-handler.desktop", "thunar-folder-handler.desktop",
                    "xfce4-file-manager.desktop"}

# Couleurs du menu Démarrer : texte/fond au moins 4,5:1 (WCAG AA) dans les deux
# thèmes, vérifié par scripts/test-shell-contrast.py.
PALETTES = {
    "dark": {"bg": "#242238", "fg": "#f7f5ff", "border": "#4f4570", "field": "#171624", "dim": "#bdb6da",
             "hover": "#433b60", "footer": "#1d1a2d", "accent": "#b69cff", "selected": "#7c4dff", "on_selected": "#ffffff",
             "side": "#1d1a2d", "card": "#2b2843", "card_hover": "#3b3458", "chip": "#352f52", "on_accent": "#1a1030"},
    "light": {"bg": "#f6f4fc", "fg": "#1f1b2e", "border": "#d3cbee", "field": "#ffffff", "dim": "#5a5374",
              "hover": "#e4ddf7", "footer": "#ebe7f6", "accent": "#6a3df0", "selected": "#6a3df0", "on_selected": "#ffffff",
              "side": "#ece8f8", "card": "#ffffff", "card_hover": "#f1ecff", "chip": "#e6dffd", "on_accent": "#ffffff"},
}
MENU_CSS = Template("""
  #winlux-start { background: $bg; color: $fg; border: 1px solid $border; border-radius: 8px; }
  #winlux-start box, #winlux-start scrolledwindow, #winlux-start viewport { background: transparent; }
  #winlux-start label, #winlux-start image, #winlux-start button { color: $fg; text-shadow: none; -gtk-icon-shadow: none; }
  #winlux-start entry { border-radius: 18px; background: $field; color: $fg; padding: 9px 14px; border: 1px solid $border;
                        border-bottom: 2px solid $accent; box-shadow: none; }
  #winlux-start entry image { color: $dim; }
  #winlux-start entry selection { background: $selected; color: $on_selected; }
  #winlux-start button { background: transparent; border: none; box-shadow: none; border-radius: 6px; padding: 7px; }
  #winlux-start button:hover, #winlux-start button:focus { background: $hover; }
  #winlux-start #winlux-start-osk label { font-size: 20px; }
  /* Navigation au clavier (#28) : cadre visible seulement au clavier, comme Windows. */
  #winlux-start button { outline: 2px solid $accent; outline-offset: -2px; }
  #winlux-start .heading { font-weight: bold; font-size: 12pt; }
  #winlux-start .footer { background: $footer; padding: 12px 25px; border-top: 1px solid $border; }
  #winlux-start .app-name { font-size: 9pt; }
  #winlux-start label.dim { color: $dim; font-size: 8.5pt; }
  #winlux-start label.accent { color: $accent; font-weight: bold; }
  #winlux-start label.letter { color: $accent; font-weight: bold; font-size: 11pt; }
  #winlux-start button.link { background: $chip; padding: 6px 12px; }
  #winlux-start button.link:hover { background: $hover; }
  /* Volet « Pour vous » et cartes de « Toutes les applications » (#685). */
  #winlux-start box.side { background: $side; border-left: 1px solid $border; }
  #winlux-start box.side label.section { font-weight: bold; font-size: 10pt; }
  #winlux-start .card, #winlux-start box.side .card { background: $card; border: 1px solid $border; border-radius: 8px; }
  #winlux-start button.card:hover, #winlux-start button.card:focus { background: $card_hover; }
  #winlux-start box.avatar { background: $selected; border-radius: 17px; }
  #winlux-start box.avatar label { color: $on_selected; font-weight: bold; }
  #winlux-start box.views { background: $field; border: 1px solid $border; border-radius: 8px; padding: 2px; }
  #winlux-start box.views button { padding: 4px 10px; }
  #winlux-start box.views button.on { background: $accent; }
  #winlux-start box.views button.on label { color: $on_accent; font-weight: bold; }
  #winlux-start box.more { background: $chip; border-radius: 9px; }
  #winlux-start box.more label { color: $accent; font-weight: bold; }
  #winlux-start label.badge { background: $chip; color: $accent; font-size: 7pt; font-weight: bold; padding: 0 4px; border-radius: 4px; }
  #winlux-start button.letter-key { padding: 2px 0; min-width: 22px; }
  #winlux-start button.letter-key label { font-weight: bold; font-size: 9pt; }
  menu.winlux-menu, .winlux-menu.menu { background: $bg; border: 1px solid $border; border-radius: 8px; padding: 4px; }
  menu.winlux-menu menuitem { color: $fg; padding: 8px 18px 8px 12px; border-radius: 4px; min-width: 150px; }
  menu.winlux-menu menuitem label, menu.winlux-menu menuitem image { color: $fg; -gtk-icon-shadow: none; }
  menu.winlux-menu menuitem:hover { background: $hover; }
  menu.winlux-menu menuitem:disabled label { color: $dim; opacity: 0.6; }
  menu.winlux-menu menuitem arrow { color: $fg; min-width: 16px; min-height: 16px; }
  menu.winlux-menu separator { background: $border; min-height: 1px; margin: 4px 6px; }
  /* Glisser-déposer des épinglés (#925) : l'icône prise s'estompe, la place
     visée est entourée de l'accent. */
  #winlux-start button.pin-dragging { opacity: 0.35; }
  #winlux-start button.pin-drop { background: $chip; box-shadow: inset 0 0 0 2px $accent; }
""")



def dark_theme():
    """Thème sombre demandé : préférence GTK, thème « …-dark » ou GTK_THEME=…:dark."""
    forced = os.environ.get("GTK_THEME")
    if forced: return "dark" in forced.casefold()  # GTK ignore alors la préférence
    settings = Gtk.Settings.get_default()
    name = (settings.get_property("gtk-theme-name") or "").casefold()
    if name.startswith("winlux-"): return "-dark" in name  # thème de winlux-personalize
    return bool(settings.get_property("gtk-application-prefer-dark-theme")) or "dark" in name


def on_click(button, callback):
    """Clic fiable pendant la saisie du pointeur du menu.

    Sous cette saisie, GTK ne reçoit pas toujours l'entrée du pointeur dans le
    bouton : il n'est alors pas « survolé » et « clicked » n'est jamais émis
    (Paramètres ne s'ouvrait pas un essai sur deux). Le relâchement du bouton
    gauche dans ses limites suffit donc ; Entrée/Espace passent par clicked.
    """
    pending = []
    def run():
        pending.clear()
        callback()
        return False
    def released(widget, event):
        inside = 0 <= event.x < widget.get_allocated_width() and 0 <= event.y < widget.get_allocated_height()
        if getattr(widget, "winlux_dragged", False): return False
        if event.button == 1 and inside and not pending:
            # Après le traitement GTK : un « clicked » émis entre-temps est ignoré.
            pending.append(True)
            GLib.idle_add(run)
        return False
    def clicked(widget):
        if getattr(widget, "winlux_dragged", False): return
        if not pending: callback()
    button.connect("button-release-event", released)
    button.connect("clicked", clicked)


def osk_module():
    """Clavier tactile (/usr/bin/winlux-osk, ou le script voisin), ou None."""
    import importlib.machinery
    import importlib.util
    for path in (Path("/usr/bin/winlux-osk"), Path(__file__).resolve().with_name("winlux-osk.py")):
        if path.is_file():
            try:
                loader = importlib.machinery.SourceFileLoader("winlux_osk", str(path))
                spec = importlib.util.spec_from_loader("winlux_osk", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module
            except Exception:
                return None
    return None


class Start(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="org.winlux.Start", flags=Gio.ApplicationFlags.FLAGS_NONE)
        self.window = None
        self.context = None
        self.all_apps = False
        self.file_generation = 0
        self.file_query = ""
        self.file_results = []
        # Toutes les applications : catégorie dépliée, et volet « Pour vous »
        # calculé en arrière-plan à chaque ouverture (#685).
        self.open_category = None
        self.dragging = None
        self.side_generation = 0
        self.side_data = None
        try: self.state = json.loads(STATE.read_text())
        except (OSError, ValueError): self.state = {"pins": DEFAULTS, "recent": []}
        if not isinstance(self.state, dict): self.state = {"pins": DEFAULTS, "recent": []}
        self.state["pins"] = migrate_pins(self.state.get("pins", DEFAULTS))

    def save(self):
        STATE.parent.mkdir(parents=True, exist_ok=True)
        temporary = STATE.with_suffix(".tmp")
        temporary.write_text(json.dumps(self.state, ensure_ascii=False))
        temporary.replace(STATE)

    def do_activate(self):
        if self.window and self.window.get_visible():
            self.hide()
            return
        if not self.window: self.build()
        # Épingles ajoutées par une autre application (Jeux) depuis la
        # dernière ouverture : relues à chaque ouverture.
        try: self.state = json.loads(STATE.read_text())
        except (OSError, ValueError): pass
        # Services de la session livrés avec une entrée de menu par leur paquet.
        self.apps = {app.get_id(): app for app in Gio.AppInfo.get_all()
                     if listed(app) and app.get_id() not in SESSION_SERVICES}
        changed = auto_pin(self.state, self.apps)
        changed |= note_new_apps(self.state, self.apps, time.time())
        if changed: self.save()
        self.all_apps = False
        self.open_category = None
        self.search.set_text("")
        self.render()
        self.refresh_side()
        # Tablette sans clavier : le clavier tactile du menu s'ouvre avec lui.
        # Le clavier de la barre des tâches, fenêtre à part, fermait le menu au
        # premier appui et se trouvait en partie dessous.
        keyboard_shown = self.osk_widget is not None and self.osk_auto()
        if keyboard_shown:
            try: subprocess.Popen(["winlux-osk", "hide"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
            except OSError: pass
        area = self.workarea()
        # Comme Windows 11 : le menu suit l'alignement de la barre, au clic
        # comme à la touche Windows. Barre à gauche : au-dessus du bouton
        # Démarrer ; centrée ou en capsule flottante (toujours centrée) : au
        # milieu. Deviner d'après le pointeur ramenait le menu au centre dès
        # que le clic tombait dans la zone de travail (#732).
        self.menu_left = taskbar_on_left()
        self.place(keyboard_shown)
        self.window.show_all()
        self.window.present()
        self.search.grab_focus()
        # Comme Windows 11, chaque ouverture repart du haut de la liste : la
        # fenêtre cachée gardait le défilement de la fois précédente, qu'il
        # fallait remonter à la main (#673). Remis à zéro aussi une fois la
        # liste dessinée, GTK recalant la position à la nouvelle hauteur.
        self.scroll_top()
        GLib.idle_add(self.scroll_top)

    def workarea(self):
        display = self.window.get_display()
        monitor = display.get_primary_monitor() or display.get_monitor(0)
        return monitor.get_workarea()

    def side_wanted(self, area, keyboard_shown):
        """Volet « Pour vous » : sauf s'il est coupé, clavier tactile ouvert
        ou écran trop étroit pour deux volets lisibles (moins de 1100 px)."""
        return for_you_enabled(self.state) and not keyboard_shown and area.width >= 1100

    def place(self, keyboard_shown):
        """Taille et position du menu, avec ou sans son clavier tactile.

        Deux volets côte à côte : le menu à gauche, « Pour vous » à droite.
        Volet coupé, le menu redevient un seul volet, centré de la même façon."""
        area = self.workarea()
        large = area.width >= 1600 and area.height >= 900
        main_width = 640 if large else 560
        side_width = 360 if large else 320
        height = 700 if large else 650
        side = self.side_wanted(area, keyboard_shown)
        width = main_width + (side_width if side else 0)
        self.left.set_size_request(main_width, -1)
        self.side.set_size_request(side_width, -1)
        self.side.set_visible(side)
        # Clavier ouvert : marges resserrées, et touches moins hautes sur un
        # petit écran. En 1366x768 la 2e ligne des épinglés perdait son libellé
        # sous le clavier.
        compact = keyboard_shown
        self.body.set_spacing(6 if compact else 14)
        self.body.set_margin_top(8 if compact else 24)
        self.body.set_margin_bottom(4 if compact else 16)
        if self.osk_widget is not None:
            self.osk_height = 180 if area.height < 900 else 220
            self.osk_widget.set_size_request(-1, self.osk_height)
            self.osk_widget.set_visible(keyboard_shown)
            if keyboard_shown:
                self.osk_widget.show_all()
                width = max(width, self.osk_widget.get_preferred_width()[1] + 24)
                height += self.osk_height + 12
        width, height = min(width, area.width - 24), min(height, area.height - 24)
        self.window.set_default_size(width, height)
        self.window.set_size_request(width, height)
        self.window.resize(width, height)
        x = area.x + 12 if getattr(self, "menu_left", False) else area.x + (area.width - width) // 2
        self.window.move(x, area.y + area.height - height - 10)
        self.render()

    # ── Clavier tactile intégré (tablette sans clavier) ──

    def keyboard_open(self):
        return getattr(self, "osk_widget", None) is not None and self.osk_widget.get_visible()

    def osk_auto(self):
        try: return not self.osk.keyboards()
        except Exception: return False

    def osk_toggle(self):
        if self.osk_widget is None: return
        # Le menu change de taille et de place : la perte de focus ou le clic
        # vus pendant ce temps ne doivent pas le fermer (⌨ avec une recherche
        # remplie fermait tout le menu).
        self.resized_at = time.monotonic()
        self.place(not self.osk_widget.get_visible())
        # Curseur en fin de texte : grab_focus() sélectionnait toute la
        # recherche et la touche suivante remplaçait « par ».
        self.search.grab_focus_without_selecting()
        self.search.set_position(-1)

    def osk_send(self, key, _modifiers=()):
        """Touches du clavier intégré vers la recherche du menu."""
        entry = self.search
        position = entry.get_position()
        if key == "BackSpace":
            if position > 0: entry.delete_text(position - 1, position)
        elif key == "Return":
            self.first_result()
        elif key == "Escape":
            self.hide()
        elif key in ("Left", "Right"):
            entry.set_position(max(0, position + (1 if key == "Right" else -1)))
        elif len(key) == 1:
            entry.insert_text(key, position)
            entry.set_position(position + 1)

    def build(self):
        self.window = Gtk.ApplicationWindow(application=self, title=_t("Démarrer Winlux"))
        self.window.set_decorated(False)
        self.window.set_resizable(False)
        self.window.set_skip_taskbar_hint(True)
        self.window.set_skip_pager_hint(True)
        self.window.set_type_hint(Gdk.WindowTypeHint.UTILITY)
        self.window.set_keep_above(True)
        self.window.connect("delete-event", lambda *_: self.hide())
        self.window.connect("focus-out-event", self.blur)
        self.window.connect("key-press-event", self.key)
        self.window.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
        self.window.connect("button-press-event", self.outside_click)
        # Glisser d'un épinglé (#925) : mouvements et relâchement entre deux tuiles.
        self.window.add_events(Gdk.EventMask.BUTTON_MOTION_MASK | Gdk.EventMask.BUTTON_RELEASE_MASK)
        self.window.connect("motion-notify-event", self.drag_motion)
        self.window.connect("button-release-event", self.drag_release)
        # La saisie du pointeur n'aboutit que sur une fenêtre réellement
        # visible ; openbox mappe d'abord la fenêtre dans un cadre encore
        # caché (NOT_VIEWABLE), d'où quelques nouvelles tentatives.
        self.window.connect("map-event", lambda *_: self.grab_soon())
        self.css = Gtk.CssProvider()
        # Au-dessus du gtk.css utilisateur (winlux-personalize) : le menu garde
        # sa propre palette, lisible dans les deux thèmes.
        Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), self.css, Gtk.STYLE_PROVIDER_PRIORITY_USER + 1)
        settings = Gtk.Settings.get_default()
        for name in ("gtk-theme-name", "gtk-application-prefer-dark-theme"):
            settings.connect("notify::" + name, lambda *_: self.apply_theme())
        self.apply_theme()
        self.window.set_name("winlux-start")
        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self.window.add(outer)
        # Deux volets (#685) : le menu à gauche, « Pour vous » à droite.
        panes = Gtk.Box(spacing=0)
        outer.pack_start(panes, True, True, 0)
        self.left = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        panes.pack_start(self.left, True, True, 0)
        body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14, margin_start=26, margin_end=26)
        self.body = body
        self.left.pack_start(body, True, True, 0)
        self.search = Gtk.SearchEntry(placeholder_text=_t("Applications, fichiers et paramètres"))
        self.search.connect("search-changed", lambda *_: self.render())
        self.search.connect("activate", self.first_result)
        search_row = Gtk.Box(spacing=8)
        search_row.pack_start(self.search, True, True, 0)
        body.pack_start(search_row, False, False, 0)
        self.osk = osk_module()
        self.osk_widget, self.osk_height = None, 0
        if self.osk is not None:
            try:
                self.osk_widget = self.osk.keyboard_widget(send=self.osk_send, on_close=self.osk_toggle)
            except Exception:
                self.osk_widget = None
        if self.osk_widget is not None:
            self.osk_height = 220
            self.osk_widget.set_size_request(-1, self.osk_height)
            self.osk_widget.set_margin_start(12)
            self.osk_widget.set_margin_end(12)
            self.osk_widget.set_no_show_all(True)
            # Bouton clavier à droite de la recherche, toujours visible : l'ouvre
            # ou le ferme, avec ou sans clavier physique. (Une icône dans le
            # champ ne tenait pas : GtkSearchEntry y met et enlève sa croix
            # d'effacement.)
            self.osk_button = Gtk.Button(label="⌨")
            self.osk_button.set_name("winlux-start-osk")
            self.osk_button.set_can_focus(False)
            self.osk_button.set_tooltip_text(_t("Clavier tactile"))
            self.osk_button.set_size_request(48, -1)
            self.osk_button.connect("clicked", lambda *_: self.osk_toggle())
            search_row.pack_end(self.osk_button, False, False, 0)
        header = Gtk.Box(spacing=8)
        self.heading = Gtk.Label(label=_t("Épinglé"), xalign=0)
        self.heading.set_ellipsize(3)
        self.heading.get_style_context().add_class("heading")
        header.pack_start(self.heading, True, True, 0)
        self.toggle = Gtk.Button(label=_t("Toutes les applications  ›"))
        self.toggle.get_style_context().add_class("link")
        on_click(self.toggle, self.toggle_list)
        header.pack_end(self.toggle, False, False, 0)
        # Vues de « Toutes les applications » : catégories, grille, liste A–Z.
        self.views = Gtk.Box(spacing=2)
        self.views.get_style_context().add_class("views")
        self.view_buttons = {}
        for key, label in VIEWS:
            button = Gtk.Button(label=_t(label))
            on_click(button, lambda key=key: self.set_view(key))
            self.views.pack_start(button, False, False, 0)
            self.view_buttons[key] = button
            button.show_all()
        # Visible seulement dans « Toutes les applications » (render()) :
        # no_show_all, ses boutons sont donc montrés ci-dessus.
        self.views.set_no_show_all(True)
        header.pack_end(self.views, False, False, 0)
        body.pack_start(header, False, False, 0)
        scroll = self.scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        self.content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        scroll.add(self.content)
        body.pack_start(scroll, True, True, 0)
        footer = Gtk.Box(spacing=8)
        footer.get_style_context().add_class("footer")
        avatar = Gtk.Box()
        avatar.get_style_context().add_class("avatar")
        avatar.set_size_request(34, 34)
        initial = Gtk.Label(label=(account_name()[:1] or "W").upper())
        avatar.set_center_widget(initial)
        footer.pack_start(avatar, False, False, 0)
        footer.pack_start(Gtk.Label(label=account_name(), xalign=0, margin_start=4), True, True, 0)
        # Raccourcis du bas comme Windows 11 : Explorateur, Paramètres, alimentation.
        for icon, tip, action in (("folder-symbolic", "Explorateur", lambda: self.run_tool(["winlux-shell-app", "explorer"])),
                                  ("emblem-system-symbolic", "Paramètres", lambda: self.run_tool(["winlux-shell-app", "settings"]))):
            button = Gtk.Button.new_from_icon_name(icon, Gtk.IconSize.LARGE_TOOLBAR)
            button.set_tooltip_text(_t(tip))
            on_click(button, action)
            footer.pack_start(button, False, False, 0)
        power = Gtk.Button.new_from_icon_name("system-shutdown-symbolic", Gtk.IconSize.LARGE_TOOLBAR)
        power.set_tooltip_text(_t("Options d’alimentation"))
        on_click(power, lambda: self.power(power))
        footer.pack_end(power, False, False, 0)
        self.left.pack_end(footer, False, False, 0)
        # Volet « Pour vous » : rempli par render_side().
        self.side = FixedWidthBox(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        self.side.get_style_context().add_class("side")
        self.side.set_no_show_all(True)
        panes.pack_end(self.side, False, False, 0)
        if self.osk_widget is not None:
            outer.pack_end(self.osk_widget, False, False, 0)

    def run_tool(self, command):
        self.hide()
        try: subprocess.Popen(command, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError: pass

    def apply_theme(self):
        """Menu clair en thème clair, sombre en thème sombre, comme Windows 11.

        Chaque nœud qui porte du texte ou une icône symbolique reçoit sa couleur :
        sinon le thème GTK impose la sienne (Adwaita clair : texte gris foncé des
        boutons et des champs) sur le fond du menu et le texte disparaît."""
        self.css.load_from_data(MENU_CSS.substitute(PALETTES["dark" if dark_theme() else "light"]).encode())

    def scroll_top(self):
        self.scroll.get_vadjustment().set_value(0)
        return False

    def hide(self):
        if self.window.has_grab():self.window.grab_remove()
        self.window.get_display().get_default_seat().ungrab()
        self.window.hide()
        return True

    def grab_soon(self):
        self.grab_attempts = 0
        GLib.timeout_add(30, self.grab_pointer)
        return False

    def grab_pointer(self):
        if self.window.get_visible() and not self.context:
            # owner_events=True : les clics dans le menu restent livrés aux boutons ;
            # seuls les clics hors de nos fenêtres reviennent à outside_click.
            # Avec False, chaque clic partait vers la fenêtre principale et le
            # menu ne réagissait que lorsque la saisie avait échoué (1 fois sur 2).
            self.pointer_status=self.window.get_display().get_default_seat().grab(self.window.get_window(),Gdk.SeatCapabilities.POINTER,
                True,None,None,None,None)
            if self.pointer_status==Gdk.GrabStatus.SUCCESS:self.window.grab_add()
            elif self.pointer_status==Gdk.GrabStatus.NOT_VIEWABLE:
                self.grab_attempts = getattr(self, "grab_attempts", 0) + 1
                return self.grab_attempts < 30
        return False

    def settling(self):
        return time.monotonic() - getattr(self, "resized_at", 0) < 0.8

    def outside_click(self, _, event):
        if self.settling(): return False
        x,y=self.window.get_position();width,height=self.window.get_size()
        if not (x<=event.x_root<x+width and y<=event.y_root<y+height):return self.hide()
        return False

    def blur(self, *_):
        def later():
            if not self.window.is_active() and not self.context and not self.settling(): self.hide()
            return False
        GLib.timeout_add(150, later)
        return False

    def key(self, _, event):
        return self.hide() if event.keyval == Gdk.KEY_Escape else False

    def toggle_list(self, *_):
        # « Retour » depuis une recherche : effacer la recherche (le texte
        # tapé gardait sinon les résultats affichés, bouton sans effet).
        if self.search.get_text().strip():
            self.all_apps = False
            self.search.set_text("")
        else:
            self.all_apps = not self.all_apps
        self.open_category = None
        self.render()
        # Changement de vue (Épinglé ↔ Toutes les applications) : en haut.
        self.scroll_top()

    def first_result(self, *_):
        # Même ordre que la liste : applications, Paramètres, fichiers.
        query=fold(self.search.get_text().strip())
        found=settings_for(query) if query else []
        if self.results:self.launch(self.results[0])
        elif found:self.open_setting(found[0][1])
        elif self.file_results:self.open_file(self.file_results[0])

    def launch(self, app):
        try:
            app.launch([], self.window.get_display().get_app_launch_context())
            self.state["recent"] = ([app.get_id()] + [value for value in self.state.get("recent", []) if value != app.get_id()])[:6]
            # Historique local de lancement : « Utilisées souvent » et les
            # habitudes du volet « Pour vous » (#685). Rien ne sort du PC.
            record_use(self.state, app.get_id())
            self.save()
            self.hide()
        except GLib.Error as error:
            dialog = Gtk.MessageDialog(transient_for=self.window, modal=True, message_type=Gtk.MessageType.ERROR,
                                       buttons=Gtk.ButtonsType.CLOSE, text=_t("Impossible d’ouvrir l’application"))
            dialog.format_secondary_text(str(error))
            dialog.run()
            dialog.destroy()

    def app_button(self, app, grid=False, badge=False, pinned=False):
        button = Gtk.Button()
        if pinned:
            # Avant on_click : un relâchement qui termine un glisser ne lance rien.
            self.pin_drag(button, app.get_id())
        if grid: button.set_size_request(80, 82)  # six colonnes tiennent dans 560 px (1366x768)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL if grid else Gtk.Orientation.HORIZONTAL, spacing=6)
        icon = app.get_icon() or Gio.ThemedIcon.new("application-x-executable")
        image = Gtk.Image.new_from_gicon(icon, Gtk.IconSize.DIALOG if grid else Gtk.IconSize.LARGE_TOOLBAR)
        image.set_pixel_size(32 if grid else 24)
        box.pack_start(image, False, False, 0)
        label = Gtk.Label(label=app.get_display_name(), xalign=0.5 if grid else 0)
        label.set_ellipsize(3)
        label.set_max_width_chars(12 if grid else 50)
        if grid:
            label.set_line_wrap(True)
            # Un mot plus long que la tuile (« Textverarbeitungsprogramm ») se
            # coupe aussi en deux lignes au lieu d'élargir le menu (#869).
            label.set_line_wrap_mode(2)  # Pango.WrapMode.WORD_CHAR
            label.set_lines(2)
        label.get_style_context().add_class("app-name")
        box.pack_start(label, not badge, not badge, 0)
        if badge and (app.get_id() or "").startswith("winlux-pe-"):
            # Programme Windows (.exe) lancé par Winlux : petit repère, comme
            # les catégories qui les regroupent.
            mark = Gtk.Label(label=".exe", valign=Gtk.Align.CENTER)
            mark.get_style_context().add_class("badge")
            box.pack_start(mark, False, False, 0)
        button.add(box)
        button.set_tooltip_text(app.get_display_name())
        on_click(button, lambda: self.launch(app))
        button.connect("button-press-event", lambda widget, event: self.app_context(widget, event, app, pinned))
        return button

    def result_button(self, title, icon, tooltip, subtitle=None):
        """Ligne de résultat : icône, titre et, pour un fichier, son dossier en petit."""
        button = Gtk.Button()
        box = Gtk.Box(spacing=8)
        image = Gtk.Image.new_from_gicon(icon if isinstance(icon, Gio.Icon) else Gio.ThemedIcon.new(icon), Gtk.IconSize.LARGE_TOOLBAR)
        image.set_pixel_size(24)
        box.pack_start(image, False, False, 0)
        texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        label = Gtk.Label(label=title, xalign=0)
        label.set_ellipsize(3)
        label.get_style_context().add_class("app-name")
        texts.pack_start(label, False, False, 0)
        if subtitle:
            detail = Gtk.Label(label=subtitle, xalign=0)
            detail.set_ellipsize(1)
            detail.get_style_context().add_class("dim-label")
            texts.pack_start(detail, False, False, 0)
        box.pack_start(texts, True, True, 0)
        button.add(box)
        button.set_tooltip_text(tooltip)
        return button

    def file_context(self, widget, event, path):
        """Clic droit sur un fichier trouvé : ouvrir, ouvrir l'emplacement, copier le chemin."""
        if event.button != 3: return False
        self.menu(widget, event, [(_t("Ouvrir"), lambda: self.open_file(path)),
                                  (_t("Ouvrir l’emplacement du fichier"), lambda: self.open_location(path)),
                                  (_t("Copier le chemin"), lambda: Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).set_text(str(path), -1))])
        return True

    def open_location(self, path):
        self.hide()
        try: subprocess.Popen(["winlux-shell-app", "explorer", str(path.parent)], start_new_session=True)
        except OSError: pass

    def render(self):
        if not self.window or not hasattr(self, "apps"): return
        for widget in self.content.get_children(): widget.destroy()
        query = fold(self.search.get_text().strip())
        listing = self.all_apps or bool(query)
        self.heading.set_text(_t("Résultats") if query else _t("Toutes les applications") if listing else _t("Épinglé"))
        self.toggle.set_label(_t("‹  Retour") if listing else _t("Toutes les applications  ›"))
        self.results = sorted((app for app in self.apps.values() if matches(app, query)),
                              key=lambda app: (rank(app.get_display_name(), query), app.get_display_name().casefold()))
        self.views.set_visible(listing and not query)
        for key, button in self.view_buttons.items():
            context = button.get_style_context()
            (context.add_class if key == self.apps_view() else context.remove_class)("on")
        if listing and not query:
            self.render_all_apps()
        elif listing:
            # Ordre des résultats : applications (le nom qui commence par la
            # recherche d'abord), puis pages des Paramètres, puis fichiers. Avec
            # « par », un Parallel.dll téléchargé passait avant l'application
            # Paramètres et Entrée l'ouvrait.
            previous = None
            # Recherche : groupes comme Windows 11 (Meilleur résultat, Applications, Paramètres, Documents).
            pages = settings_for(query) if query else []
            if query and query != self.file_query:
                self.file_query = query
                self.file_generation += 1
                self.file_results = []
                if len(query) >= 2:
                    threading.Thread(target=self.find_files, args=(query, self.file_generation), daemon=True).start()
            for index, app in enumerate(self.results):
                group = (_t("Meilleur résultat") if index == 0 else _t("Applications")) if query else category(app)
                if group != previous:
                    heading = Gtk.Label(label=group, xalign=0, margin_top=10)
                    heading.get_style_context().add_class("heading")
                    self.content.pack_start(heading, False, False, 0)
                    previous = group
                self.content.pack_start(self.app_button(app), False, False, 0)
            if pages:
                heading = Gtk.Label(label=_t("Meilleur résultat") if not self.results else _t("Paramètres"), xalign=0, margin_top=10)
                heading.get_style_context().add_class("heading")
                self.content.pack_start(heading, False, False, 0)
                for number, (label, page) in enumerate(pages):
                    if number == 1 and not self.results:
                        heading = Gtk.Label(label=_t("Paramètres"), xalign=0, margin_top=10)
                        heading.get_style_context().add_class("heading")
                        self.content.pack_start(heading, False, False, 0)
                    button = self.result_button(label, "preferences-system", _t("Paramètres · ") + label)
                    on_click(button, lambda target=page: self.open_setting(target))
                    self.content.pack_start(button, False, False, 0)
            if self.file_results:
                heading = Gtk.Label(label=_t("Documents"), xalign=0, margin_top=10)
                heading.get_style_context().add_class("heading")
                self.content.pack_start(heading, False, False, 0)
            for path in self.file_results:
                relative = str(path.relative_to(Path.home()))
                folder = str(path.parent.relative_to(Path.home()))
                button = self.result_button(path.name, file_icon(path), _t("Fichier · ") + relative, subtitle="~" if folder == "." else "~/" + folder)
                on_click(button, lambda target=path: self.open_file(target))
                button.connect("button-press-event", lambda widget, event, target=path: self.file_context(widget, event, target))
                self.content.pack_start(button, False, False, 0)
            if len(self.file_results) == 30:
                self.content.pack_start(Gtk.Label(label=_t("30 premiers fichiers affichés. Précisez la recherche."), xalign=0), False, False, 0)
            if not self.results and not (query and (self.file_results or pages)):
                self.content.pack_start(Gtk.Label(label=_t("Aucune application trouvée")), False, False, 0)
        else:
            grid = Gtk.Grid(column_homogeneous=True, column_spacing=4, row_spacing=12)
            self.content.pack_start(grid, False, False, 0)
            pinned = [self.apps[value] for value in self.state.get("pins", DEFAULTS) if value in self.apps]
            for index, app in enumerate(pinned): grid.attach(self.app_button(app, True, pinned=True), index % 6, index // 6, 1, 1)
            # Clavier tactile ouvert : la place manque, les épinglés seuls (le
            # titre « Recommandations » était coupé par le bas de la liste).
            if self.keyboard_open():
                self.content.show_all()
                return
            # Sous les épinglés : les applications les plus lancées qui n'y
            # sont pas déjà (#685), calculées sur ce PC.
            pins = set(self.state.get("pins", DEFAULTS))
            frequent = [self.apps[ident] for ident in frequent_apps(usage_of(self.state), time.time(), exclude=pins, limit=12)
                        if ident in self.apps][:6]
            if frequent:
                heading = Gtk.Label(label=_t("Utilisées souvent"), xalign=0, margin_top=10)
                heading.get_style_context().add_class("heading")
                self.content.pack_start(heading, False, False, 0)
                self.content.pack_start(self.app_grid(frequent), False, False, 0)
        self.content.show_all()

    def apps_view(self):
        view = self.state.get("apps_view")
        return view if view in dict(VIEWS) else "categories"

    def set_view(self, key):
        self.state["apps_view"] = key
        self.open_category = None
        self.save()
        self.render()
        self.scroll_top()

    def app_grid(self, apps, columns=6):
        grid = Gtk.Grid(column_homogeneous=True, column_spacing=4, row_spacing=8)
        for index, app in enumerate(apps): grid.attach(self.app_button(app, True), index % columns, index // columns, 1, 1)
        for index in range(len(apps), columns):  # colonnes vides : même largeur de case
            grid.attach(Gtk.Box(), index, 0, 1, 1)
        return grid

    def section_label(self, text, margin_top=6):
        label = Gtk.Label(label=text, xalign=0, margin_top=margin_top)
        label.get_style_context().add_class("heading")
        self.content.pack_start(label, False, False, 0)
        return label

    def render_all_apps(self):
        """« Toutes les applications » mieux rangé (#685) : ajoutées récemment,
        puis catégories en cartes (un clic déplie la catégorie), grille ou
        liste A–Z avec une barre de lettres."""
        apps = sorted(self.apps.values(), key=lambda app: fold(app.get_display_name()))
        view = self.apps_view()
        fresh = [self.apps[ident] for ident in recently_added(self.state, self.apps, time.time())]
        if view != "list" and fresh and not self.open_category:
            self.section_label(_t("Ajoutées récemment"), 0)
            self.content.pack_start(self.app_grid(fresh), False, False, 0)
        if view == "grid":
            self.content.pack_start(self.app_grid(apps), False, False, 0)
            return
        if view == "list":
            self.render_letters(apps)
            return
        groups = {}
        for app in apps: groups.setdefault(category(app), []).append(app)
        order = [_t(name) for name in CATEGORY_ORDER]
        names = sorted(groups, key=lambda name: (order.index(name) if name in order else len(order), fold(name)))
        if self.open_category in groups:
            back = Gtk.Button(label=_t("‹  Catégories"), halign=Gtk.Align.START)
            back.get_style_context().add_class("link")
            on_click(back, lambda: self.unfold(None))
            self.content.pack_start(back, False, False, 0)
            self.section_label(self.open_category, 2)
            self.content.pack_start(self.app_grid(groups[self.open_category]), False, False, 0)
            return
        cards = Gtk.Grid(column_homogeneous=True, column_spacing=10, row_spacing=10, margin_top=4)
        for index, name in enumerate(names):
            cards.attach(self.category_card(name, groups[name]), index % 3, index // 3, 1, 1)
        for index in range(len(names), 3): cards.attach(Gtk.Box(), index, 0, 1, 1)
        self.content.pack_start(cards, False, False, 0)

    def unfold(self, name):
        self.open_category = name
        self.render()
        self.scroll_top()

    def category_card(self, name, apps):
        """Carte d'une catégorie : quatre icônes (ou « +N »), son nom et son nombre."""
        button = Gtk.Button()
        button.get_style_context().add_class("card")
        button.set_tooltip_text(name)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin=6)
        mini = Gtk.Grid(column_homogeneous=True, row_spacing=6, column_spacing=6)
        shown = apps[:3] if len(apps) > 4 else apps[:4]
        for index, app in enumerate(shown):
            image = Gtk.Image.new_from_gicon(app.get_icon() or Gio.ThemedIcon.new("application-x-executable"), Gtk.IconSize.DND)
            image.set_pixel_size(32)
            mini.attach(image, index % 2, index // 2, 1, 1)
        if len(apps) > 4:
            more = Gtk.Box(halign=Gtk.Align.CENTER)
            more.get_style_context().add_class("more")
            more.set_size_request(32, 32)
            more.set_center_widget(Gtk.Label(label=f"+{len(apps) - 3}"))
            mini.attach(more, 1, 1, 1, 1)
        for index in range(len(shown) + (len(apps) > 4), 4):
            mini.attach(Gtk.Box(height_request=32), index % 2, index // 2, 1, 1)
        box.pack_start(mini, False, False, 0)
        line = Gtk.Box(spacing=6)
        title = Gtk.Label(label=name, xalign=0)
        title.set_ellipsize(3)
        title.get_style_context().add_class("app-name")
        title.get_style_context().add_class("accent-title")
        line.pack_start(title, True, True, 0)
        count = Gtk.Label(label=str(len(apps)))
        count.get_style_context().add_class("dim")
        line.pack_end(count, False, False, 0)
        box.pack_start(line, False, False, 0)
        button.add(box)
        on_click(button, lambda: self.unfold(name))
        return button

    def render_letters(self, apps):
        """Liste A–Z : barre de lettres (saut direct), puis une section par lettre."""
        letters = {}
        for app in apps: letters.setdefault(letter_of(app.get_display_name()), []).append(app)
        bar = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, min_children_per_line=9, max_children_per_line=27,
                          homogeneous=True, column_spacing=0, row_spacing=0)
        headings = {}
        for letter in "#ABCDEFGHIJKLMNOPQRSTUVWXYZ":
            key = Gtk.Button(label=letter)
            key.get_style_context().add_class("letter-key")
            key.set_sensitive(letter in letters)
            on_click(key, lambda letter=letter: self.jump_to(headings.get(letter)))
            bar.add(key)
        self.content.pack_start(bar, False, False, 0)
        for letter in sorted(letters, key=lambda value: (value != "#", value)):
            group = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
            heading = Gtk.Label(label=letter, xalign=0, margin_start=8, margin_bottom=2)
            heading.get_style_context().add_class("letter")
            group.pack_start(heading, False, False, 0)
            headings[letter] = heading
            for app in letters[letter]:
                group.pack_start(self.app_button(app, badge=True), False, False, 0)
            self.content.pack_start(group, False, False, 0)

    def jump_to(self, widget):
        if widget is None: return
        point = widget.translate_coordinates(self.content, 0, 0)
        if point:
            adjustment = self.scroll.get_vadjustment()
            adjustment.set_value(min(point[1], adjustment.get_upper() - adjustment.get_page_size()))

    # ── Volet « Pour vous » ──

    def refresh_side(self):
        """Calcule le volet en arrière-plan (fichiers récents, Téléchargements)
        et le dessine dès que c'est prêt ; l'ancien contenu reste en attendant."""
        self.side_generation += 1
        generation = self.side_generation
        if not for_you_enabled(self.state):
            return
        state = json.loads(json.dumps(self.state))
        def work():
            now = time.time()
            data_home = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
            files = recent_files(data_home / "recently-used.xbel", limit=3)
            opened = {path for path, _, _ in recent_files(data_home / "recently-used.xbel", limit=200)}
            todo = todo_items(downloads_dir(), opened, set(state.get("dismissed", [])), now, update_ready())
            data = {"now": now, "files": files, "habits": habit_apps(usage_of(state), now), "todo": todo}
            def deliver():
                if generation == self.side_generation:
                    self.side_data = data
                    self.render_side()
                return False
            GLib.idle_add(deliver)
        threading.Thread(target=work, daemon=True).start()
        self.render_side()

    def side_section(self, title):
        label = Gtk.Label(label=title, xalign=0)
        label.get_style_context().add_class("section")
        self.side.pack_start(label, False, False, 0)

    def side_row(self, icon, title, detail, action=None, action_label=None, tooltip=None):
        button = Gtk.Button()
        box = Gtk.Box(spacing=10)
        image = Gtk.Image.new_from_gicon(icon if isinstance(icon, Gio.Icon) else Gio.ThemedIcon.new(icon), Gtk.IconSize.LARGE_TOOLBAR)
        image.set_pixel_size(28)
        box.pack_start(image, False, False, 0)
        texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=1, valign=Gtk.Align.CENTER)
        name = Gtk.Label(label=title, xalign=0)
        name.set_ellipsize(3)
        name.get_style_context().add_class("app-name")
        texts.pack_start(name, False, False, 0)
        if detail:
            sub = Gtk.Label(label=detail, xalign=0)
            sub.set_ellipsize(3)
            sub.get_style_context().add_class("dim")
            texts.pack_start(sub, False, False, 0)
        box.pack_start(texts, True, True, 0)
        if action_label:
            verb = Gtk.Label(label=action_label, valign=Gtk.Align.CENTER)
            verb.get_style_context().add_class("accent")
            box.pack_end(verb, False, False, 0)
        button.add(box)
        button.set_tooltip_text(tooltip or title)
        if action: on_click(button, action)
        return button

    def render_side(self):
        for widget in self.side.get_children(): widget.destroy()
        inner = self.side
        inner.set_margin_top(0)
        top = Gtk.Box(spacing=6, margin_top=20, margin_start=18, margin_end=14)
        spark = Gtk.Label(label="✦")
        spark.get_style_context().add_class("accent")
        top.pack_start(spark, False, False, 0)
        title = Gtk.Label(label=_t("Pour vous"), xalign=0)
        title.get_style_context().add_class("heading")
        top.pack_start(title, True, True, 0)
        hide = Gtk.Button(label=_t("Masquer"))
        hide.set_tooltip_text(_t("Masquer le volet « Pour vous » (réactivable dans Paramètres)"))
        on_click(hide, self.hide_side)
        top.pack_end(hide, False, False, 0)
        inner.pack_start(top, False, False, 0)
        body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin_start=16, margin_end=16)
        inner.pack_start(body, True, True, 0)
        data = self.side_data or {"now": time.time(), "files": [], "habits": [], "todo": []}
        now = data["now"]
        def section(text):
            label = Gtk.Label(label=text, xalign=0, margin_top=6, margin_start=2)
            label.get_style_context().add_class("section")
            body.pack_start(label, False, False, 0)
        section(_t("Reprendre où vous en étiez"))
        for path, application, when in data["files"]:
            detail = " · ".join(part for part in (self.app_label(application), ago(when, now)) if part)
            row = self.side_row(file_icon(path), path.name, detail, lambda target=path: self.open_file(target),
                                tooltip=str(path))
            row.connect("button-press-event", lambda widget, event, target=path: self.file_context(widget, event, target))
            body.pack_start(row, False, False, 0)
        if not data["files"]:
            hint = Gtk.Label(label=_t("Les fichiers que vous ouvrez apparaîtront ici."), xalign=0, wrap=True, margin_start=4, max_width_chars=28)
            hint.get_style_context().add_class("dim")
            body.pack_start(hint, False, False, 0)
        habits = [self.apps[ident] for ident in data["habits"] if ident in getattr(self, "apps", {})]
        section(_t("Souvent ouvert à cette heure-ci"))
        if habits:
            tiles = Gtk.Grid(column_homogeneous=True, column_spacing=8)
            for index, app in enumerate(habits[:3]):
                tile = self.app_button(app, True)
                tile.get_style_context().add_class("card")
                tile.set_size_request(-1, 76)
                tiles.attach(tile, index, 0, 1, 1)
            for index in range(len(habits[:3]), 3): tiles.attach(Gtk.Box(), index, 0, 1, 1)
            body.pack_start(tiles, False, False, 0)
            why = Gtk.Label(label=habit_reason(now), wrap=True, justify=Gtk.Justification.CENTER, max_width_chars=28)
            why.get_style_context().add_class("dim")
            body.pack_start(why, False, False, 0)
        else:
            hint = Gtk.Label(label=_t("Winlux apprend vos habitudes au fil des jours, sans rien envoyer."), xalign=0, wrap=True, margin_start=4, max_width_chars=28)
            hint.get_style_context().add_class("dim")
            body.pack_start(hint, False, False, 0)
        if data["todo"]:
            section(_t("À faire"))
            card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
            card.get_style_context().add_class("card")
            updates = getattr(self, "apps", {}).get("winlux-updates.desktop")
            for item in data["todo"]:
                icon = updates.get_icon() if item["kind"] == "update" and updates and updates.get_icon() else item["icon"]
                row = self.side_row(icon, item["title"], item["detail"], lambda item=item: self.todo(item), item["action"])
                row.connect("button-press-event", lambda widget, event, item=item: self.todo_context(widget, event, item))
                card.pack_start(row, False, False, 0)
            body.pack_start(card, False, False, 0)
        privacy = Gtk.Box(spacing=8, margin_start=18, margin_end=16, margin_bottom=16)
        lock = Gtk.Image.new_from_icon_name("changes-prevent-symbolic", Gtk.IconSize.MENU)
        privacy.pack_start(lock, False, False, 0)
        note = Gtk.Label(label=_t("Calculé sur ce PC à partir de votre usage, rien n’est envoyé. Désactivable dans Paramètres."),
                         xalign=0, wrap=True)
        note.set_max_width_chars(28)
        note.get_style_context().add_class("dim")
        privacy.pack_start(note, True, True, 0)
        inner.pack_end(privacy, False, False, 0)
        # Le volet a no_show_all (il n'apparaît que si place() le veut) :
        # show_all() ne descend pas dans ses enfants, ils sont montrés ici.
        for child in self.side.get_children(): child.show_all()

    def app_label(self, name):
        """Nom lisible du programme noté par GTK (« mousepad » → « Mousepad »)."""
        lowered = (name or "").casefold()
        if not lowered: return ""
        for app in getattr(self, "apps", {}).values():
            exe = Path(app.get_executable() or "").name.casefold() if hasattr(app, "get_executable") else ""
            if lowered in (exe, (app.get_id() or "").casefold().removesuffix(".desktop")):
                return app.get_display_name()
        return name[:1].upper() + name[1:]

    def todo(self, item):
        if item["kind"] == "update": self.open_setting(7)
        elif item["kind"] == "installer": self.open_file(item["path"])
        else: self.run_tool(["winlux-shell-app", "explorer", str(item["path"])])

    def todo_context(self, widget, event, item):
        """Clic droit : « Ignorer » retire la suggestion pour de bon."""
        if event.button != 3: return False
        def dismiss():
            self.state.setdefault("dismissed", []).append(item["key"])
            self.state["dismissed"] = self.state["dismissed"][-100:]
            self.save()
            self.side_data["todo"] = [entry for entry in self.side_data["todo"] if entry["key"] != item["key"]]
            self.render_side()
        self.menu(widget, event, [(_t("Ignorer"), dismiss)])
        return True

    def hide_side(self):
        """« Masquer » : même réglage que l'interrupteur de Paramètres ; le menu
        redevient un seul volet, recentré."""
        self.state["for_you"] = False
        self.save()
        self.resized_at = time.monotonic()
        self.place(self.keyboard_open())

    def open_setting(self, page):
        subprocess.Popen(["winlux-shell-app", "settings", str(page)])
        self.hide()

    def open_file(self, path):
        self.hide()
        try:Gio.AppInfo.launch_default_for_uri(path.as_uri(),self.window.get_display().get_app_launch_context())
        except GLib.Error as error:
            dialog=Gtk.MessageDialog(transient_for=self.window,modal=True,message_type=Gtk.MessageType.ERROR,
                buttons=Gtk.ButtonsType.CLOSE,text=_t("Impossible d’ouvrir ce fichier"),secondary_text=str(error))
            dialog.run();dialog.destroy()

    def find_files(self, query, generation):
        # Bounded name search in visible personal directories. Never follows
        # directory symlinks or scans the system disks from the UI thread.
        # Début de mot comme Windows (#607) : « son » trouve « Son.wav », pas
        # « application-json.png » ni « Personnalisation ». Jusqu'à 300
        # candidats, puis les 30 meilleurs : nom qui commence par la recherche,
        # puis un de ses mots, les plus récents d'abord (os.walk seul remplissait
        # la liste avec le premier dossier parcouru).
        matches, visited = [], 0
        for directory, folders, files in os.walk(Path.home(), followlinks=False):
            if generation != self.file_generation: return
            folders[:] = [name for name in folders if not name.startswith(".") and not Path(directory, name).is_symlink()]
            for name in folders + files:
                visited += 1
                if not name.startswith(".") and found_in(query, name): matches.append(Path(directory, name))
                if len(matches) >= 300 or visited >= 30000: break
            if len(matches) >= 300 or visited >= 30000: break
        matches = sorted(matches, key=lambda path: (rank(path.name, query), -modified(path), fold(path.name)))[:30]
        def deliver():
            if generation == self.file_generation:
                self.file_results = matches
                self.render()
            return False
        GLib.idle_add(deliver)

    def menu(self, widget, event, actions):
        if self.window.has_grab():self.window.grab_remove()
        self.window.get_display().get_default_seat().ungrab()
        menu = self.menu_items(actions)
        self.context = menu
        def deactivate(*_):
            self.context=None
            GLib.idle_add(self.grab_pointer)
        menu.connect("deactivate", deactivate)
        menu.show_all()
        if event: menu.popup_at_pointer(event)
        else: menu.popup_at_widget(widget, Gdk.Gravity.NORTH_EAST, Gdk.Gravity.SOUTH_EAST, None)

    def menu_items(self, actions):
        """Menu contextuel : (titre, action[, icône]). Une action None grise
        l'entrée, une liste d'entrées en fait un sous-menu, None seul sépare."""
        menu = Gtk.Menu()
        menu.get_style_context().add_class("winlux-menu")
        for entry in actions:
            if entry is None:
                menu.append(Gtk.SeparatorMenuItem())
                continue
            title, callback = entry[0], entry[1]
            item = Gtk.MenuItem()
            row = Gtk.Box(spacing=12)
            if len(entry) > 2:
                row.pack_start(Gtk.Image.new_from_icon_name(entry[2], Gtk.IconSize.MENU), False, False, 0)
            row.pack_start(Gtk.Label(label=title, xalign=0), True, True, 0)
            item.add(row)
            if isinstance(callback, list):
                item.set_submenu(self.menu_items(callback))
                item.set_sensitive(any(child and child[1] is not None for child in callback))
            elif callback is None:
                item.set_sensitive(False)
            else:
                item.connect("activate", lambda _, action=callback: action())
            menu.append(item)
        return menu

    def visible_pins(self):
        return [ident for ident in self.state.get("pins", DEFAULTS) if ident in self.apps]

    def move_pin(self, identifier, where):
        """Déplace un épinglé (#925) et enregistre l'ordre aussitôt."""
        pins = move_pin(self.state.get("pins", list(DEFAULTS)), identifier, where, self.apps)
        if pins == self.state.get("pins"): return
        self.state["pins"] = pins
        self.save()
        self.render()

    def move_actions(self, identifier):
        """Entrées du sous-menu « Déplacer », grisées quand elles ne changent rien."""
        shown = self.visible_pins()
        index = shown.index(identifier) if identifier in shown else -1
        last = len(shown) - 1
        def entry(title, where, possible):
            return (_t(title), (lambda: self.move_pin(identifier, where)) if possible and index >= 0 else None)
        return [entry("Au début", "first", index > 0), entry("Vers la gauche", "previous", index > 0),
                entry("Vers la droite", "next", index < last), entry("À la fin", "last", index < last)]

    def pin_drag(self, button, identifier):
        """Glisser-déposer d'un épinglé dans sa grille (#925).

        Fait à la main, sans le protocole XDND : le menu tient déjà la saisie
        du pointeur (owner_events), si bien que les mouvements et le
        relâchement arrivent à la tuile ou à la fenêtre sous le pointeur, pas
        à la tuile prise. L'état du glisser est donc partagé (self.dragging)
        et tout se calcule en coordonnées d'écran. Au relâchement l'icône
        prend la place visée, sans lancer d'application."""
        button.winlux_pin = identifier
        button.add_events(Gdk.EventMask.BUTTON_MOTION_MASK)
        def pressed(_, event):
            button.winlux_dragged = False
            self.dragging = {"ident": identifier, "button": button, "start": (event.x_root, event.y_root),
                             "active": False, "target": None} if event.button == 1 else None
            return False
        button.connect("button-press-event", pressed)
        button.connect("motion-notify-event", self.drag_motion)
        button.connect("button-release-event", self.drag_release)

    def pin_tile_at(self, grid, x_root, y_root):
        for child in grid.get_children():
            window = child.get_window()
            if window is None or not child.get_mapped(): continue
            area = child.get_allocation()
            left, top = window.get_root_coords(area.x, area.y)
            if left <= x_root < left + area.width and top <= y_root < top + area.height:
                return child
        return None

    def drag_motion(self, _, event):
        drag = getattr(self, "dragging", None)
        if not drag or not event.state & Gdk.ModifierType.BUTTON1_MASK: return False
        button = drag["button"]
        grid = button.get_parent()
        if not isinstance(grid, Gtk.Grid): return False
        if not drag["active"]:
            threshold = Gtk.Settings.get_default().get_property("gtk-dnd-drag-threshold") or 8
            if abs(event.x_root - drag["start"][0]) < threshold and abs(event.y_root - drag["start"][1]) < threshold:
                return False
            drag["active"] = True
            button.winlux_dragged = True
            button.get_style_context().add_class("pin-dragging")
            window = self.window.get_window()
            if window: window.set_cursor(Gdk.Cursor.new_from_name(window.get_display(), "grabbing"))
        target = self.pin_tile_at(grid, event.x_root, event.y_root) or drag["target"]
        if target is not drag["target"]:
            if drag["target"] is not None: drag["target"].get_style_context().remove_class("pin-drop")
            drag["target"] = target
            if target is not button: target.get_style_context().add_class("pin-drop")
        return True

    def drag_release(self, _, event):
        drag = getattr(self, "dragging", None)
        if event.button != 1 or not drag: return False
        self.dragging = None
        if not drag["active"]: return False
        window = self.window.get_window()
        if window: window.set_cursor(None)
        where = getattr(drag["target"], "winlux_pin", None)
        # La grille est reconstruite (ordre nouveau ou non) : aucune tuile ne
        # reste marquée ni enfoncée. Relâchement gardé : rien ne se lance.
        def drop():
            shown = self.visible_pins()
            if where in shown and where != drag["ident"]: self.move_pin(drag["ident"], shown.index(where))
            else: self.render()
            return False
        GLib.idle_add(drop)
        return True

    def app_context(self, widget, event, app, in_pins=False):
        if event.button != 3: return False
        identifier = app.get_id()
        def pin():
            pins = self.state.setdefault("pins", list(DEFAULTS))
            if identifier in pins: pins.remove(identifier)
            else: pins.append(identifier)
            self.save()
            self.render()
        def shortcut():
            source = app.get_filename()
            desktop = Path(GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_DESKTOP) or str(Path.home() / "Bureau"))
            desktop.mkdir(parents=True, exist_ok=True)
            target = desktop / Path(source).name
            if not target.exists():
                shutil.copyfile(source, target)
                target.chmod(0o755)
                subprocess.run(["gio", "set", "-t", "string", str(target), "metadata::trusted", "true"],
                               check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                # xfdesktop 4.20 ne fait confiance qu'à la somme SHA-256 du lanceur.
                checksum = hashlib.sha256(target.read_bytes()).hexdigest()
                subprocess.run(["gio", "set", "-t", "string", str(target), "metadata::xfce-exe-checksum", checksum],
                               check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        def action(name):
            self.hide()
            app.launch_action(name, None)
        # Actions déclarées par l'application (« Mettre à jour Claude Code »,
        # « Nouvelle fenêtre »…), comme la liste de raccourcis de Windows.
        actions = [(app.get_action_name(name), lambda name=name: action(name))
                   for name in (app.list_actions() if hasattr(app, "list_actions") else [])]
        self.menu(widget, event, [(_t("Ouvrir"), lambda: self.launch(app)), *actions,
                                 (_t("Détacher du menu Démarrer") if identifier in self.state.get("pins", DEFAULTS) else _t("Épingler au menu Démarrer"), pin),
                                 # Grille des épinglés : les ranger dans l'ordre voulu (#925).
                                 *([(_t("Déplacer"), self.move_actions(identifier))] if in_pins else []),
                                 (_t("Créer un raccourci sur le bureau"), shortcut)])
        return True

    def power(self, widget):
        # Confirmation, fermeture des applications et arrêt réel : winlux-power
        # (l'ancien chemin appelait le poweroff de systemd, absent, et rien ne
        # s'éteignait).
        def action(name):
            self.hide()
            subprocess.Popen(["/usr/bin/winlux-power", "ask", name], start_new_session=True)
        def lock(how="now"):
            self.hide()
            subprocess.Popen(["/usr/bin/winlux-lock", how], start_new_session=True)
        def sleep():
            # Veille : action « user » du relais root (winlux --power suspend) ;
            # l'écran de verrouillage revient au réveil (LOCK_ON_RESUME).
            self.hide()
            subprocess.Popen(["winlux-control", "power-suspend"], start_new_session=True,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        def logoff():
            # Ferme la session de ce compte (applications comprises), comme
            # « Se déconnecter » de Windows ; le relais root vérifie le compte.
            self.hide()
            subprocess.Popen(["winlux-control", "session-logoff", current_account()], start_new_session=True,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        # Ordre de Windows 11 : Verrouiller, Se déconnecter, Changer
        # d'utilisateur, Mettre en veille, Redémarrer, Arrêter.
        entries = [(_t("Verrouiller"), lock, "system-lock-screen-symbolic")]
        if power_menu_has_logoff():
            entries.append((_t("Se déconnecter"), logoff, "system-log-out-symbolic"))
        if power_menu_has_switch():
            entries.append((_t("Changer d’utilisateur"), lambda: lock("switch"), "system-switch-user-symbolic"))
        if power_menu_has_sleep():
            entries.append((_t("Mettre en veille"), sleep, "weather-clear-night-symbolic"))
        self.menu(widget, None, entries + [(_t("Redémarrer"), lambda: action("reboot"), "view-refresh-symbolic"),
                                           (_t("Arrêter"), lambda: action("shutdown"), "system-shutdown-symbolic")])


# Anciennes épingles par défaut remplacées : l'épingle Edge des premières
# versions devient le « Navigateur » générique (aucune marque imposée).
RENAMED_PINS = {"winlux-browser-edge.desktop": "winlux-browser.desktop"}


def rank(text, query):
    """0 : le nom commence par la recherche ; 1 : un de ses mots ; 2 : ailleurs."""
    text = fold(text)
    if text.startswith(query): return 0
    if any(word.startswith(query) for word in re.split(r"[\W_]+", text)): return 1
    return 2


def modified(path):
    """Date de modification (0 si le fichier a disparu entre-temps)."""
    try: return path.stat().st_mtime
    except OSError: return 0


def file_icon(path):
    """Icône du type de fichier (dossier, image, document…)."""
    try:
        if path.is_dir(): return Gio.ThemedIcon.new("folder")
        content_type, _ = Gio.content_type_guess(path.name, None)
        return Gio.content_type_get_icon(content_type)
    except (OSError, TypeError, GLib.Error):
        return Gio.ThemedIcon.new("text-x-generic")


def words(text):
    """Texte replié (sans accents ni casse) en mots : « Wi-Fi » → « wi fi »."""
    return re.sub(r"[^\w]+|_", " ", fold(text)).strip()


def found_in(query, text):
    """Chaque mot de la recherche commence un mot du texte, dans n'importe quel
    ordre ; « wifi » trouve aussi « Wi-Fi » (mots voisins recollés)."""
    query, tokens = words(query).split(), words(text).split()
    if not query: return False
    # Début de mot, comme Windows : « son » trouve Son, pas « personnalisation ».
    if all(any(token.startswith(word) for token in tokens) for word in query): return True
    joined = "".join(query)
    return any("".join(tokens[index:index + 3]).startswith(joined) for index in range(len(tokens)))


def settings_for(query):
    """Pages des Paramètres trouvées (titre traduit, titre français et mots-clés
    des quatre langues), les mieux classées d'abord : titre puis mots-clés."""
    found = []
    for (label, page), (_, french, keywords) in zip(SETTINGS, SETTINGS_INDEX):
        if found_in(query, label) or found_in(query, french):
            found.append((rank(label, query), label, page))
        elif found_in(query, keywords):
            found.append((3, label, page))
    return [(label, page) for _, label, page in sorted(found, key=lambda item: (item[0], fold(item[1])))]


def matches(app, query):
    """Nom affiché, nom générique ou mot-clé du lanceur (« boutique » trouve le Store)."""
    if not query: return True
    if found_in(query, app.get_display_name()): return True
    keywords = app.get_keywords() if hasattr(app, "get_keywords") else []
    generic = app.get_generic_name() if hasattr(app, "get_generic_name") else ""
    return found_in(query, " ".join(list(keywords or []) + [generic or ""]))


def auto_pin(state, apps):
    """Lanceurs marqués X-Winlux-Pin=true (Store, boutiques choisies) épinglés
    une seule fois par compte : détachés ensuite, ils le restent. Une boutique
    retirée puis choisie de nouveau est épinglée de nouveau."""
    done = [ident for ident in state.get("auto_pinned", []) if ident in apps]
    pins = state.setdefault("pins", list(DEFAULTS))
    for ident, app in apps.items():
        if ident in done or not hasattr(app, "get_boolean") or not app.has_key("X-Winlux-Pin"): continue
        if not app.get_boolean("X-Winlux-Pin"): continue
        if ident not in pins: pins.append(ident)
        done.append(ident)
    if done == state.get("auto_pinned"): return False
    state["auto_pinned"] = done
    return True


def move_pin(pins, identifier, where, shown=None):
    """Nouvel ordre des épinglés (#925) : identifier mis « first », « last »,
    « previous », « next » ou à la place n (0 = première) parmi les épinglés
    affichés (présents dans shown). Les épingles d'applications absentes,
    cachées dans la grille, gardent leur rang dans la liste enregistrée."""
    pins = list(pins)
    visible = [pin for pin in pins if shown is None or pin in shown]
    if identifier not in visible: return pins
    index = visible.index(identifier)
    target = {"first": 0, "last": len(visible) - 1, "previous": index - 1, "next": index + 1}.get(where, where)
    if not isinstance(target, int): return pins
    target = max(0, min(len(visible) - 1, target))
    visible.insert(target, visible.pop(index))
    order = iter(visible)
    return [next(order) if shown is None or pin in shown else pin for pin in pins]


def migrate_pins(pins):
    """Épingles enregistrées, anciennes épingles par défaut remplacées, sans doublon."""
    if not isinstance(pins, list):
        return list(DEFAULTS)
    result = []
    for pin in pins:
        pin = RENAMED_PINS.get(pin, pin)
        if pin not in result:
            result.append(pin)
    return result


# ── Volet « Pour vous » (#685) ─────────────────────────────────────────────
# Tout se calcule sur ce PC, à partir de ce que le système sait déjà : les
# lancements faits depuis le menu (start.json), les fichiers récents de GTK
# (recently-used.xbel), le dossier Téléchargements et la mise à jour déjà
# vérifiée. Rien n'est envoyé en ligne ; Paramètres > Fond d'écran et couleurs
# coupe le volet (clé « for_you » de start.json).

USAGE_LIMIT = 500              # lancements gardés (≈ quelques mois)
HALF_LIFE = 14 * 86400         # un lancement d'il y a deux semaines compte moitié
HABIT_WINDOW = 90 * 60         # ± 1 h 30 autour de l'heure actuelle
HABIT_DAYS = 56                # huit semaines d'habitudes
INSTALLER_DAYS = 30
OLD_DOWNLOAD_DAYS = 90
DOWNLOADS_HEAVY = 2 * 1024 ** 3
DAY = 86400
INSTALLER_WORDS = ("setup", "install", "installer", "installeur", "instalador", "einrichtung")


def taskbar_on_left():
    """Barre alignée à gauche (préférence système) et pas en capsule flottante (#732)."""
    align = "left"
    try:
        for line in Path(os.environ.get("WINLUX_PREFERENCES", "/etc/winlux/preferences.conf")).read_text(encoding="utf-8").splitlines():
            if line.startswith("TASKBAR_ALIGN="): align = line.split("=", 1)[1].strip()
    except (OSError, UnicodeDecodeError):
        pass
    style = "classic"
    try:
        conf = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "winlux/taskbar.conf"
        for line in conf.read_text(encoding="utf-8").splitlines():
            if line.startswith("STYLE="): style = line.split("=", 1)[1].strip()
    except (OSError, UnicodeDecodeError):
        pass
    return align == "left" and style != "floating"


def for_you_enabled(state):
    """Volet affiché sauf s'il a été coupé (Paramètres ou « Masquer »)."""
    return state.get("for_you", True) is not False


def usage_of(state):
    """Lancements enregistrés [identifiant, horodatage], les mal formés écartés."""
    return [(entry[0], int(entry[1])) for entry in state.get("usage", [])
            if isinstance(entry, (list, tuple)) and len(entry) == 2 and isinstance(entry[0], str)
            and isinstance(entry[1], (int, float))]


def record_use(state, ident, now=None):
    usage = [list(entry) for entry in usage_of(state)] + [[ident, int(now if now is not None else time.time())]]
    state["usage"] = usage[-USAGE_LIMIT:]


def frequent_apps(usage, now, exclude=(), limit=6):
    """Applications les plus lancées, les lancements récents pesant plus."""
    scores = {}
    for ident, when in usage:
        if ident in exclude: continue
        scores[ident] = scores.get(ident, 0) + 0.5 ** (max(0, now - when) / HALF_LIFE)
    return [ident for ident, _ in sorted(scores.items(), key=lambda item: (-item[1], item[0]))][:limit]


def _minutes(moment):
    return moment.tm_hour * 60 + moment.tm_min


def habit_apps(usage, now, limit=3):
    """Applications lancées d'habitude ce genre de jour (semaine ou week-end)
    à cette heure-ci, à ± 1 h 30, sur huit semaines. Il faut au moins deux
    jours différents : un lancement isolé n'est pas une habitude."""
    current = time.localtime(now)
    weekend = current.tm_wday >= 5
    days, scores = {}, {}
    for ident, when in usage:
        age = now - when
        if age < 0 or age > HABIT_DAYS * DAY: continue
        moment = time.localtime(when)
        if (moment.tm_wday >= 5) != weekend: continue
        delta = abs(_minutes(moment) - _minutes(current))
        delta = min(delta, 1440 - delta)
        if delta * 60 > HABIT_WINDOW: continue
        days.setdefault(ident, set()).add((moment.tm_year, moment.tm_yday))
        scores[ident] = scores.get(ident, 0) + (1 - delta * 60 / (2 * HABIT_WINDOW)) * 0.5 ** (age / (21 * DAY))
    ranked = sorted((ident for ident in scores if len(days[ident]) >= 2), key=lambda ident: (-scores[ident], ident))
    return ranked[:limit]


def habit_reason(now):
    """« D'après vos habitudes en semaine, le soir »."""
    moment = time.localtime(now)
    hour = moment.tm_hour
    part = "le matin" if 5 <= hour < 12 else "l’après-midi" if 12 <= hour < 18 else "le soir" if 18 <= hour < 23 else "la nuit"
    days = "le week-end" if moment.tm_wday >= 5 else "en semaine"
    return _t("D’après vos habitudes {days}, {moment}").format(days=_t(days), moment=_t(part))


def _stamp(text):
    """Horodatage ISO 8601 de recently-used.xbel (« 2026-11-07T15:12:00.1Z »)."""
    import datetime
    try:
        text = re.sub(r"\.\d+", "", text or "").replace("Z", "+00:00")
        return int(datetime.datetime.fromisoformat(text).timestamp())
    except ValueError:
        return 0


def recent_files(path, limit=3, home=None):
    """Derniers fichiers ouverts d'après GTK (recently-used.xbel) : chemin,
    application qui l'a ouvert, horodatage. Fichiers disparus, cachés ou hors
    du dossier personnel écartés."""
    import urllib.parse
    import xml.etree.ElementTree as ET
    home = Path(home or Path.home())
    try:
        root = ET.parse(path).getroot()
    except (OSError, ET.ParseError):
        return []
    namespace = "{http://www.freedesktop.org/standards/desktop-bookmarks}"
    found = {}
    for bookmark in root.iter("bookmark"):
        href = bookmark.get("href", "")
        if not href.startswith("file://"): continue
        target = Path(urllib.parse.unquote(href[7:]))
        try:
            relative = target.relative_to(home)
        except ValueError:
            continue
        if any(part.startswith(".") for part in relative.parts) or not target.is_file(): continue
        when = max(_stamp(bookmark.get(key)) for key in ("added", "modified", "visited"))
        application = ""
        for entry in bookmark.iter(namespace + "application"):
            stamp = _stamp(entry.get("modified"))
            if stamp >= when or not application:
                application = entry.get("name", "")
            when = max(when, stamp)
        if target not in found or found[target][1] < when:
            found[target] = (application, when)
    ranked = sorted(found.items(), key=lambda item: -item[1][1])
    return [(target, application, when) for target, (application, when) in ranked[:limit]]


def ago(when, now):
    seconds = int(max(0, now - when))
    if seconds < 60: return _t("à l’instant")
    if seconds < 3600: return _t("il y a {n} min").format(n=seconds // 60)
    if time.localtime(when).tm_yday == time.localtime(now).tm_yday and seconds < DAY:
        return _t("il y a {n} h").format(n=seconds // 3600)
    days = max(1, (int(now // DAY) - int(when // DAY)))
    return _t("hier") if days == 1 else _t("il y a {n} jours").format(n=days)


def human_size(size):
    for unit in ("o", "Ko", "Mo", "Go", "To"):
        if size < 1024 or unit == "To":
            text = f"{size:.1f}".rstrip("0").rstrip(".") if unit in ("Go", "To") else f"{size:.0f}"
            return f"{text.replace('.', ',') if _LANG in ('fr', 'es', 'de') else text} {unit if _LANG == 'fr' else {'o': 'B', 'Ko': 'KB', 'Mo': 'MB', 'Go': 'GB', 'To': 'TB'}[unit]}"
        size /= 1024
    return ""


def downloads_dir(home=None):
    home = Path(home or Path.home())
    try:
        for line in (home / ".config/user-dirs.dirs").read_text(encoding="utf-8").splitlines():
            if line.startswith("XDG_DOWNLOAD_DIR="):
                return Path(line.split("=", 1)[1].strip().strip('"').replace("$HOME", str(home)))
    except OSError:
        pass
    for name in ("Téléchargements", "Downloads", "Descargas"):
        if (home / name).is_dir(): return home / name
    return home / "Téléchargements"


def update_ready(state_dir=None, version_file=None):
    """Version proposée par la dernière vérification (winlux-update --check),
    ou None si Winlux est à jour ou n'a rien vérifié."""
    state_dir = Path(state_dir or os.environ.get("WINLUX_UPDATE_STATE", "/var/lib/winlux"))
    version_file = Path(version_file or os.environ.get("WINLUX_VERSION_FILE", "/usr/share/winlux/version"))
    try:
        fields = dict(line.split("=", 1) for line in (state_dir / "available-manifest").read_text().splitlines() if "=" in line)
        installed = version_file.read_text().strip()
    except OSError:
        return None
    available = fields.get("version", "").strip()
    return available if available and available != installed else None


def todo_items(downloads, opened, dismissed, now, update=None, limit=3):
    """Petites choses à faire, chacune avec son action :
    installateur .exe/.msi téléchargé depuis moins d'un mois et jamais ouvert,
    mise à jour de Winlux déjà vérifiée, dossier Téléchargements très chargé."""
    items = []
    if update:
        items.append({"key": "update:" + update, "kind": "update", "title": _t("Winlux {version} est disponible").format(version=update),
                      "detail": _t("Mise à jour du système prête"), "action": _t("Voir"), "icon": "software-update-available"})
    installers, total, old, visited = [], 0, 0, 0
    try:
        for directory, folders, files in os.walk(downloads, followlinks=False):
            folders[:] = [name for name in folders if not name.startswith(".")]
            for name in files:
                visited += 1
                if visited > 5000: break
                path = Path(directory, name)
                try: info = path.lstat()
                except OSError: continue
                total += info.st_size
                if now - info.st_mtime > OLD_DOWNLOAD_DAYS * DAY: old += 1
                lowered = fold(name)
                if (directory == str(downloads) and (lowered.endswith(".msi") or (lowered.endswith(".exe") and any(word in lowered for word in INSTALLER_WORDS)))
                        and now - info.st_mtime <= INSTALLER_DAYS * DAY and path not in opened):
                    installers.append((info.st_mtime, path))
            if visited > 5000: break
    except OSError:
        pass
    for _, path in sorted(installers, reverse=True)[:2]:
        items.append({"key": "installer:" + path.name, "kind": "installer", "title": path.name, "detail": _t("Pas encore installé"),
                      "action": _t("Installer"), "path": path, "icon": "application-x-msdownload"})
    if total >= DOWNLOADS_HEAVY:
        items.append({"key": "downloads", "kind": "downloads", "title": _t("Téléchargements : {size}").format(size=human_size(total)),
                      "detail": _t("1 fichier non ouvert depuis 3 mois") if old == 1 else _t("{n} fichiers non ouverts depuis 3 mois").format(n=old), "action": _t("Trier"),
                      "path": Path(downloads), "icon": "folder-download"})
    return [item for item in items if item["key"] not in dismissed][:limit]


VIEWS = (("categories", "Catégories"), ("grid", "Grille"), ("list", "Liste A–Z"))
# Ordre des cartes de « Toutes les applications » ; les autres catégories suivent.
CATEGORY_ORDER = ("Bureautique", "Internet", "Programmes Windows", "Audio et vidéo", "Graphisme", "Jeux",
                  "Accessoires", "Développement", "Système", "Paramètres", "Autres applications")
NEW_APP_DAYS = 7


def note_new_apps(state, apps, now):
    """Date de première apparition de chaque application, pour « Ajoutées
    récemment ». À la première ouverture, tout ce qui est déjà là compte comme
    ancien (0) : sinon tout le système passerait pour nouveau."""
    seen = state.get("seen") if isinstance(state.get("seen"), dict) else None
    first = seen is None
    seen = dict(seen or {})
    changed = first
    for ident in apps:
        if ident not in seen:
            seen[ident] = 0 if first else int(now)
            changed = True
    if changed: state["seen"] = seen
    return changed


def recently_added(state, apps, now, limit=6):
    seen = state.get("seen") if isinstance(state.get("seen"), dict) else {}
    fresh = [(when, ident) for ident, when in seen.items()
             if ident in apps and isinstance(when, (int, float)) and when and now - when <= NEW_APP_DAYS * DAY]
    return [ident for _, ident in sorted(fresh, reverse=True)][:limit]


def letter_of(name):
    first = fold(name)[:1].upper()
    return first if "A" <= first <= "Z" else "#"


def power_menu_has_sleep():
    """Veille proposée seulement si le noyau la sait faire (/sys/power/state)."""
    try:
        states = Path(os.environ.get("WINLUX_POWER_STATE", "/sys/power/state")).read_text().split()
    except OSError:
        return False
    return "mem" in states or "freeze" in states


def current_account():
    """Compte de la session (USER, sinon celui du processus)."""
    return os.environ.get("USER") or pwd.getpwuid(os.getuid()).pw_name


def power_menu_has_logoff():
    """« Se déconnecter » dans une session installée ; la session Live n'a
    pas d'écran de connexion vers lequel revenir."""
    return not Path(os.environ.get("WINLUX_LIVE_MARKER", "/etc/winlux/live-session")).exists()


def power_menu_has_switch():
    """« Changer d'utilisateur » dès qu'un autre compte existe (comme Windows)."""
    try:
        lines = Path(os.environ.get("WINLUX_USERS_CONF", "/etc/winlux/users.conf")).read_text(errors="replace").splitlines()
    except OSError:
        return False
    names = {line.split(":", 1)[0] for line in lines if line.split(":", 1)[0]} - {"root"}
    return len(names) >= 2


if __name__ == "__main__":
    raise SystemExit(Start().run())

#!/usr/bin/python3
"""Native settings: nonblocking control actions and live Linux status."""
import os
import sys
import time
from pathlib import Path
import subprocess
import unicodedata
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gio, Gtk, GLib


_LIGATURES = str.maketrans({"œ": "oe", "æ": "ae", "ø": "o", "đ": "d", "ł": "l", "ı": "i"})



def control_file(name):
    """Message ou journal d'une action de winlux-control : celui du compte
    (/run/winlux/control-<uid>, /run/winlux étant collant), sinon le partagé."""
    mine = f"/run/winlux/control-{os.getuid()}/{name}"
    return mine if os.path.exists(mine) else f"/run/winlux/{name}"

def fold(text):
    """Recherche sans casse ni accents : « reseau » trouve « Réseau »."""
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
    'Comptes': ('Accounts', 'Cuentas', 'Konten'),
    'Révision : {value}': ('Revision: {value}', 'Revisión: {value}', 'Revision: {value}'),
    'Compilée le : {value}': ('Built on: {value}', 'Compilada el: {value}', 'Erstellt am: {value}'),
    # Refonte des Paramètres (catégories, réglages intégrés)
    'Paramètres': ('Settings', 'Configuración', 'Einstellungen'),
    'Gérer les applications': ('Manage apps', 'Administrar aplicaciones', 'Apps verwalten'),
    'Bluetooth': ('Bluetooth', 'Bluetooth', 'Bluetooth'),
    'Clavier et souris': ('Keyboard & mouse', 'Teclado y mouse', 'Tastatur und Maus'),
    'Fond d’écran et couleurs': ('Background & colors', 'Fondo y colores', 'Hintergrund und Farben'),
    'Heure et langue': ('Time & language', 'Hora e idioma', 'Zeit und Sprache'),
    'Système Linux': ('Linux system', 'Sistema Linux', 'Linux-System'),
    'Echec de la mise à jour : ': ('Update failed: ', 'Error en la actualización: ', 'Update fehlgeschlagen: '),
    'aucun détail dans le journal': ('no details in the log', 'ningún detalle en el registro', 'keine Details im Protokoll'),
    "Échec de l’opération. Le journal indique la cause.": ('The operation failed. The log shows the cause.', 'La operación ha fallado. El registro indica la causa.', 'Der Vorgang ist fehlgeschlagen. Die Ursache steht im Protokoll.'),
    'Installation des mises à jour': ('Installing updates', 'Instalando actualizaciones', 'Updates werden installiert'),
    'Opération terminée.': ('Operation complete.', 'Operación terminada.', 'Vorgang abgeschlossen.'),
    'Une opération est déjà en cours. Consultez le journal.': ('An operation is already in progress. See the log.', 'Ya hay una operación en curso. Consulte el registro.', 'Es läuft bereits ein Vorgang. Siehe Protokoll.'),
    'Version installée : ': ('Installed version: ', 'Versión instalada: ', 'Installierte Version: '),
    'Version disponible : ': ('Available version: ', 'Versión disponible: ', 'Verfügbare Version: '),
    'Action en attente : ': ('Pending action: ', 'Acción pendiente: ', 'Ausstehende Aktion: '),
    'Portée : ': ('Scope: ', 'Alcance: ', 'Umfang: '),
    'Composants : ': ('Components: ', 'Componentes: ', 'Komponenten: '),
    'Taille à télécharger : ': ('Download size: ', 'Tamaño de descarga: ', 'Downloadgröße: '),
    'Taille décompressée : ': ('Unpacked size: ', 'Tamaño descomprimido: ', 'Entpackte Größe: '),
    'Action après installation : ': ('After installation: ', 'Tras la instalación: ', 'Nach der Installation: '),
    ' octets': (' bytes', ' bytes', ' Bytes'),
    ' Kio (hors sauvegarde et marge)': (' KiB (excluding backup and margin)', ' KiB (sin copia de seguridad ni margen)', ' KiB (ohne Sicherung und Reserve)'),
    'Winlux est à jour.': ('Winlux is up to date.', 'Winlux está actualizado.', 'Winlux ist auf dem neuesten Stand.'),
    'Aucune vérification disponible. Lancez une recherche de mises à jour (--check).': ('No check available yet. Search for updates (--check).', 'Todavía no hay ninguna comprobación. Busque actualizaciones (--check).', 'Noch keine Prüfung verfügbar. Suchen Sie nach Updates (--check).'),
    'Aucune vérification disponible. Lancez Rechercher.': ('No check available yet. Click Check.', 'Todavía no hay ninguna comprobación. Haga clic en Buscar.', 'Noch keine Prüfung verfügbar. Klicken Sie auf Suchen.'),
    'redémarrage du système': ('system restart', 'reinicio del sistema', 'Systemneustart'),
    'redémarrage de la session graphique': ('graphical session restart', 'reinicio de la sesión gráfica', 'Neustart der grafischen Sitzung'),
    'redémarrage conseillé': ('restart recommended', 'se recomienda reiniciar', 'Neustart empfohlen'),
    'bureau et runtime Winlux (APT)': ('Winlux desktop and runtime (APT)', 'escritorio y runtime de Winlux (APT)', 'Winlux-Desktop und -Runtime (APT)'),
    'Mio': ('MiB', 'MiB', 'MiB'),
    'Installation automatique suspendue : ': ('Automatic installation paused: ', 'Instalación automática suspendida: ', 'Automatische Installation ausgesetzt: '),
    'Version écartée (le bureau n’a pas démarré) : ': ('Version set aside (the desktop did not start): ', 'Versión descartada (el escritorio no se inició): ', 'Version zurückgestellt (Desktop startete nicht): '),
    'Version restaurée automatiquement : ': ('Version restored automatically: ', 'Versión restaurada automáticamente: ', 'Automatisch wiederhergestellte Version: '),
    'Validation au prochain démarrage du bureau : ': ('Confirmed at the next desktop start: ', 'Se confirmará en el próximo inicio del escritorio: ', 'Wird beim nächsten Desktopstart bestätigt: '),
    'Point de restauration avant mise à jour : ': ('Restore point before update: ', 'Punto de restauración antes de actualizar: ', 'Wiederherstellungspunkt vor dem Update: '),
    'Version précédente conservée : ': ('Previous version kept: ', 'Versión anterior conservada: ', 'Vorherige Version aufbewahrt: '),
    'Point de restauration': ('Restore point', 'Punto de restauración', 'Wiederherstellungspunkt'),
    'Système': ('System', 'Sistema', 'System'),
    'Applications': ('Apps', 'Aplicaciones', 'Apps'),
    'Réseau': ('Network', 'Red', 'Netzwerk'),
    'Son': ('Sound', 'Sonido', 'Sound'),
    'Clavier': ('Keyboard', 'Teclado', 'Tastatur'),
    'Souris': ('Mouse', 'Mouse', 'Maus'),
    'Personnalisation': ('Personalization', 'Personalización', 'Personalisierung'),
    'Mises à jour': ('Updates', 'Actualizaciones', 'Updates'),
    'Mise à jour OS': ('OS update', 'Actualización del SO', 'OS-Update'),
    'Écran': ('Display', 'Pantalla', 'Bildschirm'),
    'À propos': ('About', 'Acerca de', 'Info'),
    'Logiciel libre (GNU GPL v3). Projet indépendant, ni affilié à Microsoft ni approuvé par elle ; Windows est une marque de Microsoft Corporation.': ('Free software (GNU GPL v3). Independent project, neither affiliated with nor endorsed by Microsoft; Windows is a trademark of Microsoft Corporation.', 'Software libre (GNU GPL v3). Proyecto independiente, no afiliado a Microsoft ni aprobado por ella; Windows es una marca de Microsoft Corporation.', 'Freie Software (GNU GPL v3). Unabhängiges Projekt, weder mit Microsoft verbunden noch von Microsoft unterstützt; Windows ist eine Marke der Microsoft Corporation.'),
    'Programmes Windows': ('Windows programs', 'Programas de Windows', 'Windows-Programme'),
    'Navigateurs': ('Browsers', 'Navegadores', 'Browser'),
    'Accessibilité': ('Accessibility', 'Accesibilidad', 'Barrierefreiheit'),
    'Effets visuels': ('Visual effects', 'Efectos visuales', 'Visuelle Effekte'),
    'Localisation': ('Language & region', 'Idioma y región', 'Sprache und Region'),
    'Gestionnaire de fichiers': ('File manager', 'Administrador de archivos', 'Dateimanager'),
    'Son avancé': ('Advanced sound', 'Sonido avanzado', 'Erweiterter Sound'),
    'Mise à jour application': ('Application update', 'Actualización de aplicaciones', 'Anwendungsupdate'),
    'Confidentialité': ('Privacy', 'Privacidad', 'Datenschutz'),
    # Confidentialité et sécurité : verrouillage de l'écran.
    'Confidentialité et sécurité': ('Privacy & security', 'Privacidad y seguridad', 'Datenschutz und Sicherheit'),
    "Verrouillage de l’écran": ('Lock screen', 'Pantalla de bloqueo', 'Sperrbildschirm'),
    'Verrouiller automatiquement après': ('Lock automatically after', 'Bloquear automáticamente después de', 'Automatisch sperren nach'),
    'Jamais': ('Never', 'Nunca', 'Nie'),
    '1 minute': ('1 minute', '1 minuto', '1 Minute'),
    '{n} minutes': ('{n} minutes', '{n} minutos', '{n} Minuten'),
    '1 heure': ('1 hour', '1 hora', '1 Stunde'),
    'Demander le mot de passe à la sortie de veille': ('Ask for the password when waking from sleep', 'Pedir la contraseña al salir de la suspensión', 'Beim Aufwachen aus dem Energiesparmodus nach dem Kennwort fragen'),
    'Verrouiller maintenant (Windows+L)': ('Lock now (Windows+L)', 'Bloquear ahora (Windows+L)', 'Jetzt sperren (Windows+L)'),
    'Compte protégé : {account}': ('Protected account: {account}', 'Cuenta protegida: {account}', 'Geschütztes Konto: {account}'),
    'Verrouillage automatique : {delay}': ('Automatic lock: {delay}', 'Bloqueo automático: {delay}', 'Automatische Sperre: {delay}'),
    'Mot de passe au réveil : {state}': ('Password on wake: {state}', 'Contraseña al despertar: {state}', 'Kennwort beim Aufwachen: {state}'),
    'oui': ('yes', 'sí', 'ja'),
    'non': ('no', 'no', 'nein'),
    'Ce compte n’a pas de mot de passe : l’écran se déverrouille d’un clic et ne se verrouille jamais tout seul.': ("This account has no password: the screen unlocks with a click and never locks by itself.", 'Esta cuenta no tiene contraseña: la pantalla se desbloquea con un clic y nunca se bloquea sola.', 'Dieses Konto hat kein Kennwort: Der Bildschirm wird per Klick entsperrt und sperrt sich nie von selbst.'),
    # Confidentialité et sécurité : autorisations des applications Windows.
    'Autorisations des applications Windows': ('Windows app permissions', 'Permisos de las aplicaciones de Windows', 'Berechtigungen von Windows-Apps'),
    'Chaque application demande la première fois. Votre choix est retenu ; changez-le ici à tout moment.': ('Each app asks the first time. Your choice is remembered; change it here at any time.', 'Cada aplicación pregunta la primera vez. Su elección se recuerda; cámbiela aquí cuando quiera.', 'Jede App fragt beim ersten Mal. Ihre Wahl wird gespeichert; Sie können sie hier jederzeit ändern.'),
    'Caméra pour toutes les applications': ('Camera for all apps', 'Cámara para todas las aplicaciones', 'Kamera für alle Apps'),
    'Micro pour toutes les applications': ('Microphone for all apps', 'Micrófono para todas las aplicaciones', 'Mikrofon für alle Apps'),
    'Aucune application Windows n’a encore demandé d’autorisation.': ('No Windows app has asked for permission yet.', 'Ninguna aplicación de Windows ha pedido permiso todavía.', 'Noch keine Windows-App hat eine Berechtigung angefordert.'),
    'Fichiers personnels': ('Personal files', 'Archivos personales', 'Persönliche Dateien'),
    'Caméra': ('Camera', 'Cámara', 'Kamera'),
    'Micro': ('Microphone', 'Micrófono', 'Mikrofon'),
    'Capture de l’écran': ('Screen capture', 'Captura de pantalla', 'Bildschirmaufnahme'),
    'Réseau et Internet': ('Network & Internet', 'Red e Internet', 'Netzwerk und Internet'),
    'Tout redemander': ('Ask again for everything', 'Volver a preguntar todo', 'Alles erneut fragen'),
    'Processeur': ('Processor', 'Procesador', 'Prozessor'),
    'Lecteurs réseau': ('Network drives', 'Unidades de red', 'Netzlaufwerke'),
    'Personnalisation du bureau': ('Desktop personalization', 'Personalización del escritorio', 'Desktop-Personalisierung'),
    'Appareils mobiles': ('Mobile devices', 'Dispositivos móviles', 'Mobile Geräte'),
    'Bluetooth et appareils': ('Bluetooth & devices', 'Bluetooth y dispositivos', 'Bluetooth und Geräte'),
    'Alimentation et batterie': ('Power & battery', 'Energía y batería', 'Energie und Akku'),
    'Graphismes': ('Graphics', 'Gráficos', 'Grafik'),
    'Version': ('Version', 'Versión', 'Version'),
    'Noyau': ('Kernel', 'Kernel', 'Kernel'),
    'Connexion': ('Connection', 'Conexión', 'Verbindung'),
    'Volume': ('Volume', 'Volumen', 'Lautstärke'),
    'Batterie': ('Battery', 'Batería', 'Akku'),
    'Alimentation': ('Power', 'Energía', 'Energie'),
    'Mode': ('Mode', 'Modo', 'Modus'),
    'Écran éteint après': ('Turn off screen after', 'Apagar pantalla tras', 'Bildschirm aus nach'),
    'Veille après': ('Sleep after', 'Suspender tras', 'Energiesparmodus nach'),
    'Capot': ('Lid', 'Tapa', 'Deckel'),
    "Bouton d'alimentation": ('Power button', 'Botón de encendido', 'Netzschalter'),
    'Luminosité (%)': ('Brightness (%)', 'Brillo (%)', 'Helligkeit (%)'),
    'Disposition': ('Layout', 'Distribución', 'Layout'),
    'Toucher-clic': ('Tap to click', 'Tocar para hacer clic', 'Tippen zum Klicken'),
    'Vitesse': ('Speed', 'Velocidad', 'Geschwindigkeit'),
    'Alignement': ('Alignment', 'Alineación', 'Ausrichtung'),
    'Définition': ('Resolution', 'Resolución', 'Auflösung'),
    'Échelle': ('Scale', 'Escala', 'Skalierung'),
    'En attente d’Internet, installé au prochain démarrage connecté : {names}': ('Waiting for Internet, installed at the next startup with a connection: {names}', 'Esperando Internet, se instalará en el próximo arranque con conexión: {names}', 'Wartet auf Internet, wird beim nächsten Start mit Verbindung installiert: {names}'),
    'Pas de connexion Internet': ('No Internet connection', 'Sin conexión a Internet', 'Keine Internetverbindung'),
    'Rien n’est perdu : Winlux le téléchargera chez l’éditeur et l’installera automatiquement au prochain démarrage connecté. Vous pouvez aussi réessayer ici une fois connecté.': ('Nothing is lost: Winlux will download it from the publisher and install it automatically at the next startup with a connection. You can also try again here once connected.', 'No se pierde nada: Winlux lo descargará del editor y lo instalará automáticamente en el próximo arranque con conexión. También puede volver a intentarlo aquí una vez conectado.', 'Nichts geht verloren: Winlux lädt es beim Herausgeber herunter und installiert es beim nächsten Start mit Verbindung automatisch. Sie können es auch hier erneut versuchen, sobald Sie verbunden sind.'),
    'Navigateur par défaut': ('Default browser', 'Navegador predeterminado', 'Standardbrowser'),
    'Relancer le bureau': ('Restart the desktop', 'Reiniciar el escritorio', 'Desktop neu starten'),
    'Redémarrer': ('Restart', 'Reiniciar', 'Neu starten'),
    'Éteindre': ('Shut down', 'Apagar', 'Herunterfahren'),
    'Signaler un problème': ('Report a problem', 'Informar de un problema', 'Problem melden'),
    'Renouveler la connexion': ('Renew the connection', 'Renovar la conexión', 'Verbindung erneuern'),
    'Volume −': ('Volume −', 'Volumen −', 'Lautstärke −'),
    'Muet / rétablir': ('Mute / unmute', 'Silenciar / restablecer', 'Stumm / Ton an'),
    'Volume +': ('Volume +', 'Volumen +', 'Lautstärke +'),
    'Français AZERTY': ('French AZERTY', 'Francés AZERTY', 'Französisch AZERTY'),
    'Anglais QWERTY': ('English QWERTY', 'Inglés QWERTY', 'Englisch QWERTY'),
    'Activer toucher-clic': ('Turn on tap to click', 'Activar tocar para hacer clic', 'Tippen zum Klicken aktivieren'),
    'Désactiver toucher-clic': ('Turn off tap to click', 'Desactivar tocar para hacer clic', 'Tippen zum Klicken deaktivieren'),
    'Lente': ('Slow', 'Lenta', 'Langsam'),
    'Normale': ('Normal', 'Normal', 'Normal'),
    'Rapide': ('Fast', 'Rápida', 'Schnell'),
    'À gauche': ('Left', 'A la izquierda', 'Links'),
    'Centrer': ('Center', 'Centrar', 'Zentriert'),
    'Ajouter Corbeille': ('Add Recycle Bin', 'Agregar Papelera de reciclaje', 'Papierkorb hinzufügen'),
    'Ajouter Explorateur': ('Add File Explorer', 'Agregar Explorador', 'Explorer hinzufügen'),
    'Ajouter Ce PC': ('Add This PC', 'Agregar Este equipo', 'Dieser PC hinzufügen'),
    'Ajouter Documents': ('Add Documents', 'Agregar Documentos', 'Dokumente hinzufügen'),
    'Rechercher': ('Check', 'Buscar', 'Suchen'),
    'Installer': ('Install', 'Instalar', 'Installieren'),
    'Restaurer': ('Restore', 'Restaurar', 'Wiederherstellen'),
    'Mises à jour auto : activer': ('Automatic updates: turn on', 'Actualizaciones automáticas: activar', 'Automatische Updates: aktivieren'),
    'Désactiver': ('Turn off', 'Desactivar', 'Deaktivieren'),
    'Performances maximales': ('Best performance', 'Máximo rendimiento', 'Höchstleistung'),
    'Équilibré': ('Balanced', 'Equilibrado', 'Ausbalanciert'),
    "Économie d'énergie": ('Power saver', 'Ahorro de energía', 'Energiesparmodus'),
    'Écran (secteur)': ('Screen (plugged in)', 'Pantalla (enchufado)', 'Bildschirm (Netzbetrieb)'),
    'Écran (batterie)': ('Screen (on battery)', 'Pantalla (batería)', 'Bildschirm (Akku)'),
    'Veille (secteur)': ('Sleep (plugged in)', 'Suspensión (enchufado)', 'Energiesparmodus (Netzbetrieb)'),
    'Veille (batterie)': ('Sleep (on battery)', 'Suspensión (batería)', 'Energiesparmodus (Akku)'),
    'Luminosité −': ('Brightness −', 'Brillo −', 'Helligkeit −'),
    'Luminosité +': ('Brightness +', 'Brillo +', 'Helligkeit +'),
    'Mettre en veille': ('Sleep', 'Suspender', 'Energie sparen'),
    'Veille prolongée': ('Hibernate', 'Hibernar', 'Ruhezustand'),
    'Suivre la taille de la VM': ('Follow the VM size', 'Seguir el tamaño de la VM', 'VM-Größe folgen'),
    'Définition recommandée': ('Recommended resolution', 'Resolución recomendada', 'Empfohlene Auflösung'),
    'Machine virtuelle détectée ({name}) : affichage adapté à la VM (curseur logiciel).': ('Virtual machine detected ({name}): display adapted to the VM (software cursor).', 'Máquina virtual detectada ({name}): pantalla adaptada a la VM (cursor por software).', 'Virtuelle Maschine erkannt ({name}): Anzeige an die VM angepasst (Software-Cursor).'),
    'Définition fixe': ('Fixed resolution', 'Resolución fija', 'Feste Auflösung'),
    'Échelle −': ('Scale −', 'Escala −', 'Skalierung −'),
    'Automatique': ('Automatic', 'Automática', 'Automatisch'),
    'Échelle +': ('Scale +', 'Escala +', 'Skalierung +'),
    'Informations graphiques indisponibles (runtime Winlux introuvable).': ('Graphics information unavailable (Winlux runtime not found).', 'Información gráfica no disponible (no se encuentra el runtime de Winlux).', 'Grafikinformationen nicht verfügbar (Winlux-Runtime nicht gefunden).'),
    'Aucune carte graphique détectée (machine virtuelle ou affichage de secours).': ('No graphics card found (virtual machine or fallback display).', 'No se detectó ninguna tarjeta gráfica (máquina virtual o pantalla de emergencia).', 'Keine Grafikkarte erkannt (virtuelle Maschine oder Notfallanzeige).'),
    "GPU intégré (économie d'énergie)": ('Integrated GPU (power saving)', 'GPU integrada (ahorro de energía)', 'Integrierte GPU (Energiesparen)'),
    'GPU dédié (hautes performances)': ('Dedicated GPU (high performance)', 'GPU dedicada (alto rendimiento)', 'Dedizierte GPU (Hochleistung)'),
    'Carte graphique': ('Graphics card', 'Tarjeta gráfica', 'Grafikkarte'),
    "\nGPU par application : clic droit sur un .exe > Réglages de l'exécutable.": ('\nGPU per app: right-click an .exe > Executable settings.', '\nGPU por aplicación: clic derecho en un .exe > Configuración del ejecutable.', '\nGPU pro App: Rechtsklick auf eine .exe > Einstellungen der ausführbaren Datei.'),
    'Paramètres Winlux': ('Winlux Settings', 'Configuración de Winlux', 'Winlux-Einstellungen'),
    'WINLUX · PARAMÈTRES': ('WINLUX · SETTINGS', 'WINLUX · CONFIGURACIÓN', 'WINLUX · EINSTELLUNGEN'),
    'Rechercher un réglage': ('Find a setting', 'Buscar una configuración', 'Einstellung suchen'),
    'Vos préférences et l’état réel de votre ordinateur.': ('Your preferences and the real state of your computer.', 'Sus preferencias y el estado real de su equipo.', 'Ihre Einstellungen und der tatsächliche Zustand Ihres Computers.'),
    'Chargement…': ('Loading…', 'Cargando…', 'Wird geladen…'),
    'Installer, désinstaller et gérer le démarrage': ('Install, uninstall and manage startup', 'Instalar, desinstalar y administrar el inicio', 'Installieren, deinstallieren und Autostart verwalten'),
    'Wi-Fi et Bluetooth': ('Wi-Fi and Bluetooth', 'Wi-Fi y Bluetooth', 'WLAN und Bluetooth'),
    'Wi-Fi': ('Wi-Fi', 'Wi-Fi', 'WLAN'),
    'Certificats': ('Certificates', 'Certificados', 'Zertifikate'),
    'Réglages rapides': ('Quick settings', 'Configuración rápida', 'Schnelleinstellungen'),
    'Thèmes, accent, fond et barre des tâches': ('Themes, accent, background and taskbar', 'Temas, color de énfasis, fondo y barra de tareas', 'Designs, Akzentfarbe, Hintergrund und Taskleiste'),
    'Appliquer la définition': ('Apply resolution', 'Aplicar resolución', 'Auflösung übernehmen'),
    'Ouvrir les réglages détaillés de cette catégorie :': ('Open the detailed settings for this category:', 'Abrir la configuración detallada de esta categoría:', 'Detaillierte Einstellungen dieser Kategorie öffnen:'),
    'Réglages avancés': ('Advanced settings', 'Configuración avanzada', 'Erweiterte Einstellungen'),
    'Imprimantes et scanners': ('Printers and scanners', 'Impresoras y escáneres', 'Drucker und Scanner'),
    'Bureau à distance': ('Remote Desktop', 'Escritorio remoto', 'Remotedesktop'),
    'Partage et transfert': ('Sharing and transfer', 'Compartir y transferir', 'Freigabe und Übertragung'),
    'Presse-papiers': ('Clipboard', 'Portapapeles', 'Zwischenablage'),
    'Assistant IA': ('AI assistant', 'Asistente de IA', 'KI-Assistent'),
    'Dossiers partagés sur le réseau et transfert de vos réglages vers un autre PC Winlux :': ('Folders shared on the network and moving your settings to another Winlux PC:', 'Carpetas compartidas en la red y transferencia de sus ajustes a otro PC con Winlux:', 'Im Netzwerk freigegebene Ordner und Übertragung Ihrer Einstellungen auf einen anderen Winlux-PC:'),
    'La préparation des mises à jour utilise le disque. Gardez l’ordinateur allumé pendant l’installation.': ('Preparing updates uses the disk. Keep the computer on during installation.', 'La preparación de las actualizaciones usa el disco. Mantenga el ordenador encendido durante la instalación.', 'Die Vorbereitung der Updates nutzt den Datenträger. Lassen Sie den Computer während der Installation eingeschaltet.'),
    'Secours et restauration': ('Recovery and restore', 'Recuperación y restauración', 'Wiederherstellung'),
    'Récupération': ('Recovery', 'Recuperación', 'Wiederherstellung'),
    'Comptes': ('Accounts', 'Cuentas', 'Konten'),
    'Matériel et pilotes': ('Hardware and drivers', 'Hardware y controladores', 'Hardware und Treiber'),
    'Redémarrer après installation': ('Restart after installation', 'Reiniciar tras la instalación', 'Nach der Installation neu starten'),
    'Confirmer cette opération ?': ('Confirm this operation?', '¿Confirmar esta operación?', 'Diesen Vorgang bestätigen?'),
    'Enregistrez vos documents avant de continuer.': ('Save your documents before continuing.', 'Guarde sus documentos antes de continuar.', 'Speichern Sie Ihre Dokumente, bevor Sie fortfahren.'),
    'Impossible de lancer l’opération': ('Unable to start the operation', 'No se puede iniciar la operación', 'Der Vorgang kann nicht gestartet werden'),
    'Opération en cours…': ('Operation in progress…', 'Operación en curso…', 'Vorgang läuft…'),
    'Aucun détail n’a été écrit dans le journal.': ('No details were written to the log.', 'No se escribió ningún detalle en el registro.', 'Es wurden keine Details ins Protokoll geschrieben.'),
    'L’opération a échoué': ('The operation failed', 'La operación ha fallado', 'Der Vorgang ist fehlgeschlagen'),
    'Indisponible': ('Unavailable', 'No disponible', 'Nicht verfügbar'),
    'Installation': ('Installing', 'Instalación', 'Installation'),
    'Sauvegarde': ('Backup', 'Copia de seguridad', 'Sicherung'),
    'Applications installées': ('Installed apps', 'Aplicaciones instaladas', 'Installierte Apps'),
    'Recherche et préparation': ('Checking and preparing', 'Búsqueda y preparación', 'Suche und Vorbereitung'),
    'Téléchargement': ('Downloading', 'Descarga', 'Download'),
    'Vérification': ('Verifying', 'Verificación', 'Überprüfung'),
    'Décompression': ('Unpacking', 'Descompresión', 'Entpacken'),
    'Finalisation': ('Finishing', 'Finalización', 'Abschluss'),
    'Restauration': ('Restore', 'Restauración', 'Wiederherstellung'),
    'Recherche': ('Check', 'Búsqueda', 'Suche'),
    'Mise à jour': ('Update', 'Actualización', 'Update'),
    'La mise à jour a échoué': ('The update failed', 'La actualización ha fallado', 'Das Update ist fehlgeschlagen'),
    'Installer {name}': ('Install {name}', 'Instalar {name}', '{name} installieren'),
    'Définir {name} par défaut': ('Set {name} as default', 'Establecer {name} como predeterminado', '{name} als Standard festlegen'),
    '{name} (seul navigateur installé)': ('{name} (only browser installed)', '{name} (único navegador instalado)', '{name} (einziger installierter Browser)'),
    'Aucun : le navigateur vous sera demandé à chaque lien': ('None: you will be asked which browser to use for each link',
        'Ninguno: se le preguntará qué navegador usar en cada enlace', 'Keiner: Sie werden bei jedem Link nach dem Browser gefragt'),
    'Winlux ne fournit ni n’impose aucun navigateur, dans l’esprit des règles européennes sur le choix du navigateur. Choisissez celui que vous préférez : Winlux le télécharge sur le site officiel de son éditeur, vérifie le fichier et l’installe pour vous.': ('Winlux neither ships nor imposes any browser, in the spirit of the European rules on browser choice. Pick the one you prefer: Winlux downloads it from its publisher’s official website, checks the file and installs it for you.', 'Winlux no incluye ni impone ningún navegador, en el espíritu de las normas europeas sobre la elección del navegador. Elija el que prefiera: Winlux lo descarga del sitio oficial de su editor, comprueba el archivo y lo instala por usted.', 'Winlux liefert und erzwingt keinen Browser, im Sinne der europäischen Regeln zur Browserwahl. Wählen Sie Ihren bevorzugten Browser: Winlux lädt ihn von der offiziellen Website des Herstellers herunter, prüft die Datei und installiert ihn für Sie.'),
    'Aucun : choisissez-en un ci-dessous': ('None: choose one below', 'Ninguno: elija uno abajo', 'Keiner: unten einen auswählen'),
    'Ouvrir {name}': ('Open {name}', 'Abrir {name}', '{name} öffnen'),
    '{label} : {value}': ('{label}: {value}', '{label}: {value}', '{label}: {value}'),
    'Pilote {driver}': ('Driver {driver}', 'Controlador {driver}', 'Treiber {driver}'),
    'inconnu': ('unknown', 'desconocido', 'unbekannt'),
    'Charge {load} %': ('Load {load}%', 'Carga {load} %', 'Last {load} %'),
    'VRAM {used} / {total} Mo': ('VRAM {used} / {total} MB', 'VRAM {used} / {total} MB', 'VRAM {used} / {total} MB'),
    'Fréquence {freq} MHz': ('Clock {freq} MHz', 'Frecuencia {freq} MHz', 'Takt {freq} MHz'),
    'Encodeur {value} %': ('Encoder {value}%', 'Codificador {value} %', 'Encoder {value} %'),
    'Décodeur {value} %': ('Decoder {value}%', 'Decodificador {value} %', 'Decoder {value} %'),
    'Code : {code}\n\n{detail}': ('Code: {code}\n\n{detail}', 'Código: {code}\n\n{detail}', 'Code: {code}\n\n{detail}'),
    'Étape : {stage} (code {code})\n\nDernières lignes du journal :\n{detail}': ('Step: {stage} (code {code})\n\nLast lines of the log:\n{detail}', 'Etapa: {stage} (código {code})\n\nÚltimas líneas del registro:\n{detail}', 'Schritt: {stage} (Code {code})\n\nLetzte Zeilen des Protokolls:\n{detail}'),
    '{done}/{total} fichiers': ('{done}/{total} files', '{done}/{total} archivos', '{done}/{total} Dateien'),
    'Applications par défaut': ('Default apps', 'Aplicaciones predeterminadas', 'Standard-Apps'),
    # Page Processeur : temps de démarrage.
    'Temps de démarrage': ('Startup time', 'Tiempo de arranque', 'Startzeit'),
    'Dernier démarrage : {value}': ('Last startup: {value}', 'Último arranque: {value}', 'Letzter Start: {value}'),
    'précédent {value}': ('previous {value}', 'anterior {value}', 'vorher {value}'),
    'meilleur {value}': ('best {value}', 'mejor {value}', 'bester {value}'),
    'Étape la plus longue : {name} ({value})': ('Longest step: {name} ({value})', 'Paso más largo: {name} ({value})', 'Längster Schritt: {name} ({value})'),
    'Mesuré de l’allumage jusqu’au lancement du bureau.': ('Measured from power-on to the desktop starting.', 'Medido desde el encendido hasta el inicio del escritorio.', 'Gemessen vom Einschalten bis zum Start des Desktops.'),
    'Temps de démarrage pas encore mesuré.': ('Startup time not measured yet.', 'Tiempo de arranque aún no medido.', 'Startzeit noch nicht gemessen.'),
    'Noyau et micrologiciel': ('Kernel and firmware', 'Núcleo y firmware', 'Kernel und Firmware'),
    'Vérification du système': ('System check', 'Comprobación del sistema', 'Systemprüfung'),
    'Pilotes': ('Drivers', 'Controladores', 'Treiber'),
    'Détection du matériel': ('Hardware detection', 'Detección del hardware', 'Hardwareerkennung'),
    'Services': ('Services', 'Servicios', 'Dienste'),
    'Affichage': ('Display', 'Pantalla', 'Anzeige'),
    'Session': ('Session', 'Sesión', 'Sitzung'),
    'Bureau': ('Desktop', 'Escritorio', 'Desktop'),
    # Page Processeur : mode jeu.
    'Mode jeu': ('Game mode', 'Modo juego', 'Spielmodus'),
    'Jeux': ('Games', 'Juegos', 'Spiele'),
    'Mode jeu automatique': ('Automatic game mode', 'Modo juego automático', 'Automatischer Spielmodus'),
    'Désactiver le mode jeu': ('Turn off game mode', 'Desactivar el modo juego', 'Spielmodus ausschalten'),
    'Automatique : s’active quand un jeu ou une application 3D tourne.': ('Automatic: turns on while a game or 3D app is running.', 'Automático: se activa cuando se ejecuta un juego o una aplicación 3D.', 'Automatisch: aktiv, solange ein Spiel oder eine 3D-App läuft.'),
    'Désactivé.': ('Off.', 'Desactivado.', 'Aus.'),
    'Actif pour {name}.': ('Active for {name}.', 'Activo para {name}.', 'Aktiv für {name}.'),
    'Pendant une partie : {list}.': ('During a game: {list}.', 'Durante una partida: {list}.', 'Während eines Spiels: {list}.'),
    'processeur en mode performances (sur secteur)': ('processor in performance mode (plugged in)', 'procesador en modo rendimiento (con corriente)', 'Prozessor im Leistungsmodus (am Netz)'),
    'priorité du jeu relevée': ('game priority raised', 'prioridad del juego elevada', 'Spielpriorität erhöht'),
    'sauvegardes et mises à jour en pause': ('backups and updates paused', 'copias de seguridad y actualizaciones en pausa', 'Sicherungen und Updates pausiert'),
    'mémoire libérée pour le jeu': ('memory freed for the game', 'memoria liberada para el juego', 'Speicher für das Spiel freigegeben'),
    'veille bloquée': ('sleep blocked', 'suspensión bloqueada', 'Ruhezustand blockiert'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# Les numéros de page ne changent jamais : barre des tâches, menu Démarrer,
# assistant d’accueil et Paramètres PE ouvrent une page par son numéro.
PAGES = [_t("Système Linux"), _t("Gérer les applications"), _t("Réseau"), _t("Son"), _t("Clavier et souris"),
         _t("Souris"), _t("Personnalisation"), _t("Mise à jour OS"), _t("Écran"), _t("À propos"),
         _t("Programmes Windows"), _t("Navigateurs"), _t("Accessibilité"), _t("Effets visuels"),
         _t("Localisation"), _t("Gestionnaire de fichiers"), _t("Son"), _t("Mise à jour application"),
         _t("Confidentialité et sécurité"), _t("Processeur"), _t("Lecteurs réseau"), _t("Fond d’écran et couleurs"),
         _t("Bluetooth"), _t("Alimentation et batterie"), _t("Graphismes"),
         _t("Signaler un problème"), _t("Imprimantes et scanners"), _t("Bureau à distance"),
         _t("Partage et transfert"), _t("Jeux"), _t("Assistant IA"), _t("Récupération"), _t("Presse-papiers"),
         _t("Comptes"), _t("Matériel et pilotes"), _t("Wi-Fi"), _t("Certificats"), _t("Appareils mobiles"),
         _t("Sauvegarde"), _t("Applications installées")]
GRAPHICS_PAGE = 24
# Menu façon Windows 11 : catégories et sous-pages (même ordre que les
# Paramètres PE, testexe/src/shell/settings.rs). Tout se règle ici : plus de
# fenêtre « Réglages avancés » ni d’outil séparé.
# Pages 29 à 39 n’existent que dans les Paramètres natifs.
CATEGORIES = [(_t("Système"), [8, 16, 23, 24, 32, 19, 34, 0, 27, 31, 9]),
              (_t("Bluetooth et appareils"), [22, 4, 26, 37]),
              (_t("Réseau et Internet"), [35, 2, 20, 28]),
              (_t("Personnalisation"), [21, 13]),
              (_t("Applications"), [39, 1, 10, 11, 15, 30]),
              (_t("Comptes"), [33, 38]),
              (_t("Heure et langue"), [14]),
              (_t("Jeux"), [29]),
              (_t("Accessibilité"), [12]),
              # Catégorie à part, juste au-dessus de la confidentialité (voir l’issue du signalement par compte GitHub).
              (_t("Signaler un problème"), [25]),
              (_t("Confidentialité et sécurité"), [18, 36]),
              (_t("Mises à jour"), [7, 17])]
# Anciennes pages fondues dans une autre : Son (3) dans Son, Souris (5) dans
# Clavier et souris, Personnalisation (6) dans Fond d’écran et couleurs.
MERGED = {3: 16, 5: 4, 6: 21}
SOUND_PAGE = 16


def category_of(page):
    page = MERGED.get(page, page)
    return next((index for index, (_, pages) in enumerate(CATEGORIES) if page in pages), 0)
CPU_PAGE = 19
PRIVACY_PAGE = 18
LOCK_DELAYS = (0, 60, 180, 300, 600, 900, 1800, 3600)


def lock_delay_text(seconds):
    if seconds <= 0: return _t("Jamais")
    if seconds == 60: return _t("1 minute")
    if seconds == 3600: return _t("1 heure")
    return _t("{n} minutes").format(n=seconds // 60)


PERMISSION_ORDER = ("files", "camera", "microphone", "screen", "network")
PERMISSION_NAMES = {"files": "Fichiers personnels", "camera": "Caméra", "microphone": "Micro",
                    "screen": "Capture de l’écran", "network": "Réseau et Internet"}


def permission_tool():
    import shutil, sys
    if shutil.which("winlux-permission"): return ["winlux-permission"]
    return [sys.executable, str(Path(__file__).with_name("winlux-permission.py"))]


def permission_apps():
    """{programme: {capacité: allow|deny}} d’après winlux-permission list."""
    try:
        out = subprocess.run(permission_tool() + ["list"], capture_output=True, text=True, timeout=5).stdout
    except (OSError, subprocess.SubprocessError):
        return {}
    apps = {}
    for line in out.splitlines():
        parts = line.split("\t", 2)
        if len(parts) == 3 and parts[0] in PERMISSION_NAMES: apps.setdefault(parts[2], {})[parts[0]] = parts[1]
    return dict(sorted(apps.items(), key=lambda item: Path(item[0]).name.casefold()))


def privacy_summary(status):
    try: delay = int(status.get("lock_after", "300"))
    except ValueError: delay = 300
    lines = [_t("Compte protégé : {account}").format(account=status.get("lock_account") or "root"),
             _t("Verrouillage automatique : {delay}").format(delay=lock_delay_text(delay)),
             _t("Mot de passe au réveil : {state}").format(state=_t("oui") if status.get("lock_on_resume", "1") != "0" else _t("non"))]
    if status.get("lock_password") == "0":
        lines.append(_t("Ce compte n’a pas de mot de passe : l’écran se déverrouille d’un clic et ne se verrouille jamais tout seul."))
    return "\n".join(lines)
# Nom de l'hyperviseur écrit par winlux-hardware ; vide sur un vrai PC.
try:
    with open("/run/winlux/hw/virtual-machine", encoding="utf-8") as _vm: VIRTUAL_MACHINE = _vm.read().strip()
except OSError:
    VIRTUAL_MACHINE = ""
_VM_NAMES = {"qemu": "QEMU/KVM", "virtualbox": "VirtualBox", "vmware": "VMware", "hyperv": "Hyper-V",
             "xen": "Xen", "parallels": "Parallels"}
FIELDS = {
    0: [(_t("Version"), "version"), (_t("Noyau"), "kernel")],
    2: [(_t("Connexion"), "network")], 3: [(_t("Volume"), "audio")],
    23: [(_t("Batterie"), "battery"), (_t("Mode"), "power_profile"), (_t("Écran éteint après"), "power_screen_now"),
         (_t("Veille après"), "power_sleep_now"), (_t("Capot"), "power_lid"), (_t("Bouton d'alimentation"), "power_button"),
         (_t("Luminosité (%)"), "power_brightness")],
    8: [(_t("Définition"), "display_current"), (_t("Échelle"), "display_scale")],
    9: [(_t("Version"), "version"), (_t("Noyau"), "kernel")],
    11: [(_t("Navigateur par défaut"), "browser_default")],
}
ACTIONS = {
    0: [(_t("Relancer le bureau"), "restart-session"), (_t("Redémarrer"), "reboot"), (_t("Éteindre"), "poweroff"),
        (_t("Signaler un problème"), "report-open")],
    2: [(_t("Renouveler la connexion"), "network-renew")],
    3: [(_t("Volume −"), "volume-down"), (_t("Muet / rétablir"), "volume-mute"), (_t("Volume +"), "volume-up")],
    # Clavier, souris et personnalisation : réglages complets dans winlux-settings-pages.
    7: [(_t("Rechercher"), "update-check"), (_t("Installer"), "update-apply"), (_t("Restaurer"), "update-rollback"),
        (_t("Mises à jour auto : activer"), "update-auto-on"), (_t("Désactiver"), "update-auto-off")],
    23: [(_t("Performances maximales"), "power-profile-performance"), (_t("Équilibré"), "power-profile-balanced"),
         (_t("Économie d'énergie"), "power-profile-saver"), (_t("Écran (secteur)"), "power-screen-ac"),
         (_t("Écran (batterie)"), "power-screen-dc"), (_t("Veille (secteur)"), "power-sleep-ac"),
         (_t("Veille (batterie)"), "power-sleep-dc"), (_t("Capot"), "power-lid"), (_t("Bouton d'alimentation"), "power-button"),
         (_t("Luminosité −"), "power-brightness-down"), (_t("Luminosité +"), "power-brightness-up"),
         (_t("Mettre en veille"), "power-suspend"), (_t("Veille prolongée"), "power-hibernate")],
    19: [(_t("Mode jeu automatique"), "game-mode-auto"), (_t("Désactiver le mode jeu"), "game-mode-off")],
    8: [(_t("Suivre la taille de la VM") if VIRTUAL_MACHINE else _t("Définition recommandée"), "display-follow"), (_t("Définition fixe"), "display-fixed"),
        (_t("Échelle −"), "display-scale-down"), (_t("Automatique"), "display-scale-auto"), (_t("Échelle +"), "display-scale-up")],
}

def gpu_summary():
    """Résumé lisible du rapport GPU : seules les mesures exposées par le pilote."""
    try:
        out = subprocess.run(["winlux", "--gpu-status"], capture_output=True, text=True, timeout=3).stdout
    except (OSError, subprocess.SubprocessError):
        return _t("Informations graphiques indisponibles (runtime Winlux introuvable).")
    report = dict(line.split("=", 1) for line in out.splitlines() if "=" in line)
    try: count = min(8, int(report.get("gpu_count", "0")))
    except ValueError: count = 0
    if not count:
        return _t("Aucune carte graphique détectée (machine virtuelle ou affichage de secours).")
    roles = {"integrated": _t("GPU intégré (économie d'énergie)"), "dedicated": _t("GPU dédié (hautes performances)")}
    lines = []
    for i in range(count):
        get = lambda key: report.get(f"gpu{i}_{key}")
        lines.append(get("name") or _t("Carte graphique"))
        kind = [roles[get("role")]] if get("role") in roles else []
        lines.append("   " + " · ".join(kind + [_t("Pilote {driver}").format(driver=get("driver") or _t("inconnu"))]))
        metrics = []
        if get("load"): metrics.append(_t("Charge {load} %").format(load=get("load")))
        if get("vram_used_mb") and get("vram_total_mb"): metrics.append(_t("VRAM {used} / {total} Mo").format(used=get("vram_used_mb"), total=get("vram_total_mb")))
        if get("temp_c"): metrics.append(f"{get('temp_c')} °C")
        if get("freq_mhz"): metrics.append(_t("Fréquence {freq} MHz").format(freq=get("freq_mhz")))
        if get("power_w"): metrics.append(f"{get('power_w')} W")
        if get("encoder"): metrics.append(_t("Encodeur {value} %").format(value=get("encoder")))
        if get("decoder"): metrics.append(_t("Décodeur {value} %").format(value=get("decoder")))
        if metrics: lines.append("   " + " · ".join(metrics))
    if report.get("gpu_hybrid") == "1":
        lines.append(_t("\nGPU par application : clic droit sur un .exe > Réglages de l'exécutable."))
    return "\n".join(lines)

BOOT_STEPS = {"init": "Noyau et micrologiciel", "recovery": "Vérification du système", "drivers": "Pilotes",
              "hardware": "Détection du matériel", "services": "Services", "xorg": "Affichage",
              "session": "Session", "desktop": "Bureau"}


def seconds(ms):
    """Durée lisible en secondes, virgule décimale sauf en anglais."""
    try: text = f"{int(ms) / 1000:.1f} s"
    except ValueError: return "?"
    return text if _LANG == "en" else text.replace(".", ",")


def boot_summary(report):
    """Temps du dernier démarrage, mesuré par l'init (winlux --boot-time)."""
    if report.get("boot_known") != "1" or not report.get("boot_total_ms"):
        return _t("Temps de démarrage pas encore mesuré.")
    total = _t("Dernier démarrage : {value}").format(value=seconds(report["boot_total_ms"]))
    extra = []
    if report.get("boot_previous_ms"): extra.append(_t("précédent {value}").format(value=seconds(report["boot_previous_ms"])))
    if report.get("boot_best_ms") and report.get("boot_count", "1") != "1":
        extra.append(_t("meilleur {value}").format(value=seconds(report["boot_best_ms"])))
    lines = [total + (f" ({', '.join(extra)})" if extra else "")]
    steps = []
    for item in report.get("boot_phases", "").split(","):
        name, _, ms = item.partition(":")
        if name and ms: steps.append(f"{_t(BOOT_STEPS.get(name, name))} {seconds(ms)}")
    if report.get("boot_slowest") and report.get("boot_slowest_ms"):
        lines.append(_t("Étape la plus longue : {name} ({value})").format(
            name=_t(BOOT_STEPS.get(report["boot_slowest"], report["boot_slowest"])), value=seconds(report["boot_slowest_ms"])))
    if steps: lines.append(" · ".join(steps))
    lines.append(_t("Mesuré de l’allumage jusqu’au lancement du bureau."))
    return "\n".join(lines)


def game_summary(report):
    """Mode jeu : réglage, partie en cours et actions que ce matériel permet."""
    if not report.get("game_mode"):
        return ""
    lines = [_t("Mode jeu")]
    if report["game_mode"] == "off":
        lines.append(_t("Désactivé."))
    else:
        lines.append(_t("Automatique : s’active quand un jeu ou une application 3D tourne."))
        if report.get("game_active") == "1":
            lines.append(_t("Actif pour {name}.").format(name=report.get("game_name") or "?"))
    actions = []
    if report.get("cap_game_governor") == "1": actions.append(_t("processeur en mode performances (sur secteur)"))
    if report.get("cap_game_priority") == "1": actions.append(_t("priorité du jeu relevée"))
    actions.append(_t("sauvegardes et mises à jour en pause"))
    if report.get("cap_game_memory") == "1": actions.append(_t("mémoire libérée pour le jeu"))
    actions.append(_t("veille bloquée"))
    lines.append(_t("Pendant une partie : {list}.").format(list=", ".join(actions)))
    return "\n".join(lines)


def cpu_summary():
    report = dict(line.split("=", 1) for line in read("/run/winlux/cpu.txt").splitlines() if "=" in line)
    parts = [_t("Temps de démarrage") + "\n" + boot_summary(report), game_summary(report)]
    return "\n\n".join(part for part in parts if part)


# Rapport de winlux-update (toujours écrit en français) : traduit à l'affichage.
UPDATE_PREFIXES = ("Version installée : ", "Version disponible : ", "Action en attente : ", "Portée : ",
                   "Composants : ", "Taille à télécharger : ", "Taille décompressée : ",
                   "Action après installation : ", "Echec de la mise à jour : ",
                   "Installation automatique suspendue : ", "Version écartée (le bureau n’a pas démarré) : ",
                   "Version restaurée automatiquement : ", "Validation au prochain démarrage du bureau : ",
                   "Point de restauration avant mise à jour : ", "Version précédente conservée : ")
PREVIOUS_PACKAGE = "/var/cache/winlux/previous.deb"
UPDATE_ACTIONS = ("update-apply", "update-rollback")
UPDATE_SUFFIXES = (" octets", " Kio (hors sauvegarde et marge)")


def localize_update_text(text):
    lines = []
    for line in text.split("\n"):
        for prefix in UPDATE_PREFIXES:
            if line.startswith(prefix):
                value = line[len(prefix):]
                for suffix in UPDATE_SUFFIXES:
                    if value.endswith(suffix):
                        value = value[:-len(suffix)] + _t(suffix)
                line = _t(prefix) + _t(value)
                break
        else:
            line = _t(line)
        lines.append(line)
    return "\n".join(lines)

def read(name):
    try: return Path(name).read_text(errors="replace")
    except OSError: return ""


def power_hardware(root=None):
    """Matériel d'alimentation réellement présent : (batterie, capot, rétroéclairage).

    Sans batterie (ordinateur de bureau, VM), les réglages « sur batterie »
    sont masqués, jamais grisés ; de même pour le capot et la luminosité."""
    base = Path(root or os.environ.get("WINLUX_HW_ROOT", "/"))
    battery = False
    for supply in (base / "sys/class/power_supply").glob("*"):
        try:
            kind = (supply / "type").read_text().strip()
            present = (supply / "present").read_text().strip() if (supply / "present").exists() else "1"
        except OSError:
            continue
        if kind == "Battery" and present == "1" and (supply / "capacity").exists():
            battery = True
    lid = any((base / "proc/acpi/button/lid").glob("*/state"))
    backlight = any((base / "sys/class/backlight").glob("*/brightness"))
    return battery, lid, backlight


def power_page_items(fields, actions, hardware):
    """Champs et boutons de la page Alimentation selon le matériel détecté."""
    battery, lid, backlight = hardware
    def keep(key):
        if not battery and (key == "battery" or key.endswith("-dc")):
            return False
        if not lid and key in ("power_lid", "power-lid"):
            return False
        if not backlight and (key == "power_brightness" or key.startswith("power-brightness-")):
            return False
        return True
    return [f for f in fields if keep(f[1])], [a for a in actions if keep(a[1])]


def _power_modes():
    """Modes d'alimentation disponibles (winlux-power-modes, installé à côté)."""
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    for candidate in (here.with_name("winlux-power-modes.py"), here.with_name("winlux-power-modes")):
        if candidate.is_file():
            try:
                loader = SourceFileLoader("winlux_power_modes", str(candidate))
                spec = importlib.util.spec_from_loader("winlux_power_modes", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module.detect()[1]
            except Exception:
                return []
    return []


def _extra_pages():
    """Pages branchées sur les services (winlux-settings-pages, installé à côté)."""
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    for candidate in (here.with_name("winlux-settings-pages.py"), here.with_name("winlux-settings-pages")):
        if candidate.is_file():
            loader = SourceFileLoader("winlux_settings_pages", str(candidate))
            spec = importlib.util.spec_from_loader("winlux_settings_pages", loader)
            module = importlib.util.module_from_spec(spec)
            loader.exec_module(module)
            return module
    raise ImportError("winlux-settings-pages introuvable")


EXTRA = _extra_pages()


def about_build(info):
    """BUILD-INFO lisible (« Révision », « Compilée le ») au lieu des clés brutes
    commit=, tracked_sources=, scope=…"""
    values = dict(line.split("=", 1) for line in info.splitlines() if "=" in line)
    lines = []
    if values.get("commit"):
        lines.append(_t("Révision : {value}").format(value=values["commit"][:10]))
    built = values.get("built_at_utc", "")
    if len(built) >= 10 and built[4] == "-":
        lines.append(_t("Compilée le : {value}").format(value=f"{built[8:10]}/{built[5:7]}/{built[:4]}"))
    lines.append(_t("Logiciel libre (GNU GPL v3). Projet indépendant, ni affilié à Microsoft ni approuvé par elle ; Windows est une marque de Microsoft Corporation."))
    return "".join("\n" + line for line in lines)


def dark_theme():
    """Thème sombre demandé : GTK_THEME, préférence GTK ou thème « …-dark »/« …Inverse »."""
    forced = os.environ.get("GTK_THEME")
    if forced:
        return forced.lower().endswith((":dark", "-dark", "inverse"))
    settings = Gtk.Settings.get_default()
    name = (settings.get_property("gtk-theme-name") or "").lower()
    if name == "highcontrast":
        return False
    if name.startswith("winlux-"):  # thème de winlux-personalize : le nom fait foi
        return "-dark" in name
    return bool(settings.get_property("gtk-application-prefer-dark-theme")) or "dark" in name or "inverse" in name


# Chaque nœud qui porte du texte reçoit sa couleur : rien n'est laissé au
# thème GTK ni au gtk.css utilisateur, qui pourraient poser un fond différent.
SETTINGS_CSS = """
  window, window box, window viewport, window list, window label {{ background-color: transparent; }}
  window {{ background: {bg}; color: {fg}; }}
  label {{ color: {fg}; }}
  #sidebar {{ background: {side}; padding: 16px; }}
  #sidebar list, #sidebar viewport {{ background: {side}; color: {fg}; }}
  #sidebar row {{ padding: 10px; border-radius: 6px; color: {fg}; background: transparent; }}
  #sidebar row:hover {{ background: {hover}; }}
  #sidebar row:selected, #sidebar row:selected:hover {{ background: {selected}; color: {selected_fg}; }}
  #sidebar row:selected label {{ color: {selected_fg}; }}
  #sidebar .sub {{ color: {dim}; font-size: 13px; }}
  #sidebar row:selected .sub {{ color: {selected_dim}; }}
  #sidebar .open {{ font-weight: bold; }}
  #content, #content-frame {{ background: {bg}; }}
  #content list, #content list row {{ background: {card}; color: {fg}; }}
  #content list row:hover {{ background: {card_hover}; }}
  #content list row:selected, #content list row:selected label {{ background: {selected}; color: {selected_fg}; }}
  entry {{ background: {entry}; color: {fg}; border: 1px solid {border}; border-radius: 6px; padding: 8px; }}
  entry:disabled {{ color: {disabled_fg}; }}
  selection, entry selection {{ background: {selected}; color: {selected_fg}; }}
  button {{ background: {button}; color: {fg}; border: 1px solid {accent}; border-radius: 6px; padding: 10px 14px;
           box-shadow: none; background-image: none; text-shadow: none; }}
  button label {{ color: {fg}; }}
  button:hover {{ background: {button_hover}; }}
  button:disabled, button:disabled label {{ color: {disabled_fg}; background: {disabled_bg}; }}
  button:checked, button:checked label {{ background: {selected}; color: {selected_fg}; }}
  combobox button, combobox button label, menu, menuitem, menuitem label {{ background: {button}; color: {fg}; }}
  menuitem:hover, menuitem:hover label {{ background: {selected}; color: {selected_fg}; }}
  progressbar text {{ color: {fg}; }}
  .heading {{ font-size: 24px; font-weight: bold; }}
  .subtitle, label.subtitle {{ color: {dim}; }}
  .card, label.card {{ background: {card}; color: {fg}; border-radius: 8px; padding: 16px; }}
  .section {{ font-size: 16px; font-weight: bold; margin-top: 6px; }}
"""


class Settings(EXTRA.ExtraPages, Gtk.Window):
    def __init__(self):
        super().__init__(title=_t("Paramètres"))
        self.set_icon_name("preferences-system")
        self.set_default_size(1020, 740)
        self.set_size_request(720, 500)
        # Seule fenêtre de l'application en session ; sous Gtk.main() (tests),
        # la fermer quitte encore la boucle.
        self.connect("destroy", lambda *_: Gtk.main_level() and Gtk.main_quit())
        try: wanted = int(os.environ.get("WINLUX_SETTINGS_PAGE", "8"))
        except ValueError: wanted = 8
        wanted = MERGED.get(min(len(PAGES) - 1, max(0, wanted)), min(len(PAGES) - 1, max(0, wanted)))
        # Une page sans contenu possible sur cette machine n'apparaît pas.
        status = dict(line.split("=", 1) for line in read("/run/winlux/control.txt").splitlines() if "=" in line)
        self.hidden_pages = {page for page in EXTRA.ExtraPages.EXTRA_PAGES if not EXTRA.page_available(page, status)}
        # Sans batterie, la page s'appelle « Alimentation » (comme un PC de bureau).
        if not power_hardware()[0]: PAGES[23] = _t("Alimentation")
        # Sans PipeWire, « Son » garde les commandes simples de l’ancienne page.
        self.sound_fallback = SOUND_PAGE in self.hidden_pages
        self.hidden_pages.discard(SOUND_PAGE)
        self.hidden_pages |= set(MERGED)
        # Page 33 (Comptes, #309) : cachée tant que sa page n’est pas livrée.
        # Wi-Fi (35) sans carte Wi-Fi : la page Réseau la remplace.
        if wanted == 35 and wanted in self.hidden_pages: wanted = 2
        if wanted in self.hidden_pages: wanted = 8
        self.current = wanted
        self.page = wanted
        self.process = None
        self.active_action = None
        # Couleurs qui suivent le thème clair/sombre (et l'accent), à une priorité
        # au-dessus du gtk.css de winlux-personalize : sinon son fond clair passait
        # sous les gris clairs de cette feuille (1,6:1 en thème clair).
        self.css = Gtk.CssProvider()
        Gtk.StyleContext.add_provider_for_screen(self.get_screen(), self.css, Gtk.STYLE_PROVIDER_PRIORITY_USER + 1)
        self.apply_theme()
        gtk_settings = Gtk.Settings.get_default()
        for name in ("gtk-theme-name", "gtk-application-prefer-dark-theme"):
            gtk_settings.connect(f"notify::{name}", lambda *_: self.apply_theme())
        outer = Gtk.Box()
        self.add(outer)
        side = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        side.set_name("sidebar")
        side.set_size_request(240, -1)
        outer.pack_start(side, False, False, 0)
        heading = Gtk.Label(label=_t("Paramètres"), xalign=0)
        heading.get_style_context().add_class("heading")
        side.pack_start(heading, False, False, 0)
        search = Gtk.SearchEntry(placeholder_text=_t("Rechercher un réglage"))
        side.pack_start(search, False, False, 0)
        nav_scroll = Gtk.ScrolledWindow()
        side.pack_start(nav_scroll, True, True, 0)
        self.nav = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
        nav_scroll.add(self.nav)
        self.search = search
        self.rows = {}
        for index, (title, pages) in enumerate(CATEGORIES):
            row = Gtk.ListBoxRow()
            row.entry = ("category", index)
            row.add(Gtk.Label(label=title, xalign=0))
            self.nav.add(row)
            self.rows[("category", index)] = row
            for page in pages:
                row = Gtk.ListBoxRow()
                row.entry = ("page", page)
                label = Gtk.Label(label=PAGES[page], xalign=0, margin_start=18)
                label.get_style_context().add_class("sub")
                row.add(label)
                self.nav.add(row)
                self.rows[("page", page)] = row
        self.nav.set_filter_func(self.nav_visible)
        search.connect("search-changed", lambda *_: self.nav.invalidate_filter())
        self.nav.connect("row-selected", self.nav_selected)
        scroll = Gtk.ScrolledWindow()
        outer.pack_start(scroll, True, True, 0)
        self.body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=18, margin=28)
        self.body.set_name("content")
        # La vue défilante ne peint pas son fond quand la page dépasse la
        # fenêtre (cache de pixels de GtkViewport) : les marges de la page
        # sortaient en bande noire. Un cadre peint remplit toute la vue.
        page_frame = Gtk.Box()
        page_frame.set_name("content-frame")
        page_frame.pack_start(self.body, True, True, 0)
        scroll.add(page_frame)
        self.open_page(self.current)
        GLib.timeout_add(1000, self.poll)
        self.show_all()
        self.progress.set_visible(self.page == 7)

    def apply_theme(self):
        p = EXTRA.settings_palette(dark_theme(), EXTRA.appearance().get("accent", "#6a3df0"))
        self.css.load_from_data(SETTINGS_CSS.format(**p).encode())

    def label(self, text, style=None):
        label = Gtk.Label(label=text, xalign=0, wrap=True, selectable=True)
        if style: label.get_style_context().add_class(style)
        self.body.pack_start(label, False, False, 0)
        return label

    def shown_pages(self, category):
        return [page for page in CATEGORIES[category][1] if page not in self.hidden_pages]

    def nav_visible(self, row):
        kind, key = row.entry
        query = fold(self.search.get_text().strip())
        if kind == "category":
            pages = self.shown_pages(key)
            return bool(pages) and (not query or query in fold(CATEGORIES[key][0])
                                    or any(query in fold(PAGES[page]) for page in pages))
        if key in self.hidden_pages: return False
        category = category_of(key)
        if query: return query in fold(PAGES[key]) or query in fold(CATEGORIES[category][0])
        return category == category_of(self.current) and len(self.shown_pages(category)) > 1

    def nav_selected(self, _, row):
        if row is None or getattr(self, "_navigating", False): return
        kind, key = row.entry
        if kind == "page": self.open_page(key)
        elif key != category_of(self.current) or len(self.shown_pages(key)) == 1:
            pages = self.shown_pages(key)
            if pages: self.open_page(pages[0])
        else: self.open_page(self.current)

    def open_page(self, page):
        """Ouvre une sous-page : menu déplié sur sa catégorie, puis son contenu."""
        page = MERGED.get(page, page)
        if page in self.hidden_pages: return
        self.current = page
        category = category_of(page)
        self._navigating = True
        try:
            self.nav.invalidate_filter()
            single = len(self.shown_pages(category)) == 1
            for (kind, key), row in self.rows.items():
                if kind == "category":
                    style = row.get_child().get_style_context()
                    if key == category and not single: style.add_class("open")
                    else: style.remove_class("open")
            self.nav.select_row(self.rows[("category", category) if single else ("page", page)])
        finally:
            self._navigating = False
        self.select(self.nav, None)

    def select(self, _, row):
        # Page visible lisible de l'extérieur (xprop WM_WINDOW_ROLE).
        self.set_role(f"page-{self.current}")
        # Le message de la dernière opération ne suit pas d'une page à l'autre
        # (« Opération terminée. » s'affichait partout, même sans rien faire).
        self.page_opened = time.time()
        self.page = 3 if self.current == SOUND_PAGE and self.sound_fallback else self.current
        for child in self.body.get_children(): child.destroy()
        self.recovery_label = None
        category = category_of(self.current)
        if len(self.shown_pages(category)) > 1:
            self.label(CATEGORIES[category][0] + "  ›", "subtitle")
        self.label(PAGES[self.current], "heading")
        extra = self.page in EXTRA.ExtraPages.EXTRA_PAGES
        # Les pages branchées affichent leurs propres réglages : pas de cadre d'état vide.
        self.state = Gtk.Label() if extra else self.label(_t("Chargement…"), "card")
        actions = list(ACTIONS.get(self.page, []))
        if self.page == 23:
            # Modes Économie / Normal / Turbo : seulement ceux que la machine sait appliquer.
            available = _power_modes()
            actions = [(title, action) for title, action in actions
                       if not action.startswith("power-profile-") or action[len("power-profile-"):] in available]
            _, actions = power_page_items([], actions, power_hardware())
        if self.page == 11:
            # Ordre alphabétique (chrome, edge, firefox) : aucun navigateur mis en avant.
            self.label(_t("Winlux ne fournit ni n’impose aucun navigateur, dans l’esprit des règles européennes sur le choix du navigateur. Choisissez celui que vous préférez : Winlux le télécharge sur le site officiel de son éditeur, vérifie le fichier et l’installe pour vous."), "subtitle")
            # Les vrais noms, pas les identifiants internes (chrome, edge, firefox).
            actions = [(_t(verb).format(name=BROWSER_NAMES[ident]), f"browser-{action}-{ident}") for ident in BROWSER_NAMES
                       for verb, action in (("Installer {name}", "install"), ("Définir {name} par défaut", "default"), ("Ouvrir {name}", "launch"))]
        flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=3,
                           row_spacing=8, column_spacing=8)
        self.body.pack_start(flow, False, False, 0)
        self.install_button = None
        self.rollback_button = None
        self.browser_buttons = {}
        for title, action in actions:
            button = Gtk.Button(label=title)
            button.connect("clicked", lambda _, value=action: self.execute(value))
            flow.add(button)
            if action == "update-apply": self.install_button = button
            if action == "update-rollback": self.rollback_button = button
            if action.startswith("browser-"): self.browser_buttons[action] = button
        self.buttons = flow
        display_status = dict(line.split("=", 1) for line in read("/run/winlux/control.txt").splitlines() if "=" in line) if self.page == 8 else {}
        # Écrans détaillés (définition, fréquence, échelle par écran) : sections_screens.
        if self.page == 8 and not display_status.get("display_1_name"):
            self.modes = Gtk.ComboBoxText()
            status = display_status
            for mode in status.get("display_modes", "").split(","):
                if mode: self.modes.append_text(mode)
            self.modes.set_active(0)
            self.body.pack_start(self.modes, False, False, 0)
            button = Gtk.Button(label=_t("Appliquer la définition"))
            button.connect("clicked", lambda *_: self.execute("display-mode-" + self.modes.get_active_text()) if self.modes.get_active_text() else None)
            self.body.pack_start(button, False, False, 0)
        if extra: self.extra_page(self.page)
        sections = EXTRA.ExtraPages.EXTRA_SECTIONS.get(self.page)
        if sections:
            self.status = dict(line.split("=", 1) for line in read("/run/winlux/control.txt").splitlines() if "=" in line)
            getattr(self, sections)()
        if self.page == PRIVACY_PAGE:
            self.privacy_controls()
            self.permission_controls()
        self.notice = self.label("")
        self.progress = Gtk.ProgressBar(show_text=True)
        self.body.pack_start(self.progress, False, False, 0)
        self.details = self.label("")
        if self.page == 7:
            self.label(_t("La préparation des mises à jour utilise le disque. Gardez l’ordinateur allumé pendant l’installation."), "subtitle")
            restart = Gtk.Button(label=_t("Redémarrer après installation"))
            restart.connect("clicked", lambda *_: self.execute("reboot"))
            self.body.pack_start(restart, False, False, 0)
        self.poll()
        self.body.show_all()
        self.progress.set_visible(self.page == 7)
        if self.page == 11:
            # show_all() vient de tout réafficher : on remasque selon l'état réel.
            self.show_browser_buttons(dict(line.split("=", 1) for line in read("/run/winlux/control.txt").splitlines() if "=" in line))
        if self.install_button is not None:
            self.install_button.get_parent().set_visible("Winlux est à jour." not in read("/run/winlux/updates.txt"))
        if self.rollback_button is not None:
            self.rollback_button.get_parent().set_visible(bool(EXTRA.previous_version_kept()))

    def privacy_controls(self):
        """Verrouillage de l’écran : délai d’inactivité, réveil, verrouiller tout de suite."""
        status = dict(line.split("=", 1) for line in read("/run/winlux/control.txt").splitlines() if "=" in line)
        self.label(_t("Verrouillage de l’écran"), "subtitle")
        row = Gtk.Box(spacing=12)
        row.pack_start(Gtk.Label(label=_t("Verrouiller automatiquement après"), xalign=0), True, True, 0)
        delays = Gtk.ComboBoxText()
        for seconds in LOCK_DELAYS: delays.append(str(seconds), lock_delay_text(seconds))
        try: current = int(status.get("lock_after", "300"))
        except ValueError: current = 300
        delays.set_active_id(str(current if current in LOCK_DELAYS else 300))
        delays.connect("changed", lambda combo: self.execute("lock-after-" + combo.get_active_id()))
        row.pack_start(delays, False, False, 0)
        self.body.pack_start(row, False, False, 0)
        row = Gtk.Box(spacing=12)
        row.pack_start(Gtk.Label(label=_t("Demander le mot de passe à la sortie de veille"), xalign=0), True, True, 0)
        resume = Gtk.Switch(active=status.get("lock_on_resume", "1") != "0", valign=Gtk.Align.CENTER)
        resume.connect("notify::active", lambda switch, _: self.execute("lock-resume-on" if switch.get_active() else "lock-resume-off"))
        row.pack_start(resume, False, False, 0)
        self.body.pack_start(row, False, False, 0)
        lock = Gtk.Button(label=_t("Verrouiller maintenant (Windows+L)"), halign=Gtk.Align.START)
        lock.connect("clicked", lambda *_: self.execute("lock-now"))
        self.body.pack_start(lock, False, False, 0)

    def permission_controls(self):
        """Autorisations des .exe : interrupteurs globaux, puis une carte par application."""
        status = dict(line.split("=", 1) for line in read("/run/winlux/control.txt").splitlines() if "=" in line)
        self.label(_t("Autorisations des applications Windows"), "subtitle")
        self.label(_t("Chaque application demande la première fois. Votre choix est retenu ; changez-le ici à tout moment."))
        for key, action, title in (("privacy_camera", "privacy-camera", _t("Caméra pour toutes les applications")),
                                   ("privacy_microphone", "privacy-microphone", _t("Micro pour toutes les applications"))):
            row = Gtk.Box(spacing=12)
            row.pack_start(Gtk.Label(label=title, xalign=0), True, True, 0)
            switch = Gtk.Switch(active=status.get(key, "allow") != "deny", valign=Gtk.Align.CENTER)
            switch.connect("notify::active", lambda widget, _, base=action: self.execute(base + ("-allow" if widget.get_active() else "-deny")))
            row.pack_start(switch, False, False, 0)
            self.body.pack_start(row, False, False, 0)
        apps = permission_apps()
        if not apps:
            self.label(_t("Aucune application Windows n’a encore demandé d’autorisation."), "subtitle")
        for exe, caps in apps.items():
            card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
            card.get_style_context().add_class("card")
            head = Gtk.Box(spacing=12)
            names = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
            names.pack_start(Gtk.Label(label=Path(exe).name, xalign=0), False, False, 0)
            where = Gtk.Label(label=exe, xalign=0, ellipsize=3)
            where.get_style_context().add_class("subtitle")
            names.pack_start(where, False, False, 0)
            head.pack_start(names, True, True, 0)
            again = Gtk.Button(label=_t("Tout redemander"), valign=Gtk.Align.CENTER)
            again.connect("clicked", lambda *_, path=exe: self.change_permission(["forget", path], refresh=True))
            head.pack_start(again, False, False, 0)
            card.pack_start(head, False, False, 0)
            for cap in PERMISSION_ORDER:
                if cap not in caps: continue
                row = Gtk.Box(spacing=12)
                row.pack_start(Gtk.Label(label=_t(PERMISSION_NAMES[cap]), xalign=0), True, True, 0)
                switch = Gtk.Switch(active=caps[cap] == "allow", valign=Gtk.Align.CENTER)
                switch.connect("notify::active", lambda widget, _, c=cap, path=exe:
                               self.change_permission(["set", c, "allow" if widget.get_active() else "deny", path]))
                row.pack_start(switch, False, False, 0)
                card.pack_start(row, False, False, 0)
            self.body.pack_start(card, False, False, 0)

    def change_permission(self, args, refresh=False):
        try:
            subprocess.run(permission_tool() + args, check=True, timeout=5,
                           stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        except (OSError, subprocess.SubprocessError) as error:
            self.show_error(_t("L’opération a échoué"), str(error))
        if refresh: self.select(self.nav, None)

    def execute(self, action, *extra):
        if self.process is not None or self.busy: return
        if action in ("reboot", "poweroff", "restart-session", "update-rollback"):
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
                                       text=_t("Confirmer cette opération ?"), secondary_text=_t("Enregistrez vos documents avant de continuer."))
            answer = dialog.run()
            dialog.destroy()
            if answer != Gtk.ResponseType.OK: return
        try:
            self.process = subprocess.Popen(["winlux-control", action, *extra], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError as error:
            self.show_error(_t("Impossible de lancer l’opération"), str(error))
            return
        self.active_action = action
        self.notice.set_text(_t("Opération en cours…"))
        self.buttons.set_sensitive(False)

    def show_browser_buttons(self, status):
        for action, button in getattr(self, "browser_buttons", {}).items():
            child = button.get_parent() or button
            shown = browser_buttons_shown(status, action)
            if child.get_visible() != shown: child.set_visible(shown)

    def keep_screen_on(self, wanted):
        """Demande org.freedesktop.ScreenSaver.Inhibit tenue pendant l'installation
        (économiseur et DPMS, winlux-screensaver-inhibit) ; rendue après."""
        cookie = getattr(self, "screen_cookie", None)
        if wanted == (cookie is not None): return
        try:
            bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
            if wanted:
                reply = bus.call_sync("org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver", "org.freedesktop.ScreenSaver",
                                      "Inhibit", GLib.Variant("(ss)", ("Paramètres", _t("Installation des mises à jour"))),
                                      GLib.VariantType("(u)"), Gio.DBusCallFlags.NONE, 3000, None)
                self.screen_cookie = reply.unpack()[0]
            else:
                self.screen_cookie = None
                bus.call_sync("org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver", "org.freedesktop.ScreenSaver",
                              "UnInhibit", GLib.Variant("(u)", (cookie,)), None, Gio.DBusCallFlags.NONE, 3000, None)
        except GLib.Error as error:
            print(f"Demande « écran allumé » impossible : {error.message}", file=sys.stderr)
            if not wanted: self.screen_cookie = None

    def poll(self):
        status = dict(line.split("=", 1) for line in read("/run/winlux/control.txt").splitlines() if "=" in line)
        self.busy = status.get("update_busy") == "1"
        if self.process is not None and self.process.poll() is not None:
            result = self.process.returncode
            action = self.active_action
            self.process = None
            self.active_action = None
            if result == 75 and action and action.startswith(("browser-", "store-catalog-")):
                # Reporté faute de réseau : une information, pas une erreur technique.
                self.show_info(_t("Pas de connexion Internet"), _t("Rien n’est perdu : Winlux le téléchargera chez l’éditeur et l’installera automatiquement au prochain démarrage connecté. Vous pouvez aussi réessayer ici une fois connecté."))
            elif result and action in ("update-check", "update-apply", "update-rollback"):
                self.show_update_error(action, result)
            elif result:
                detail = read(control_file("action.log")).strip()[-3000:] or _t("Aucun détail n’a été écrit dans le journal.")
                self.show_error(_t("L’opération a échoué"), _t("Code : {code}\n\n{detail}").format(code=result, detail=detail))
            if result == 0 and action and action.startswith(("browser-install-", "browser-default-")):
                # Lien demandé avant qu'un navigateur soit là : ouvert maintenant,
                # sous le compte de l'utilisateur (l'installation passe par le relais root).
                open_pending_link()
            # Page branchée : l'état relu après l'action (appareil appairé, mode changé…).
            GLib.idle_add(self.extra_refresh)
        self.buttons.set_sensitive(self.process is None and not self.busy)
        # Installation en cours : l'écran ne s'éteint pas (#634, le DPMS coupait
        # l'écran au milieu d'une mise à jour lancée ici).
        self.keep_screen_on(self.busy or (self.process is not None and self.active_action in UPDATE_ACTIONS))
        fields = FIELDS.get(self.page, [])
        if self.page == 23:
            fields, _ = power_page_items(fields, [], power_hardware())
        def shown(key):
            value = status.get(key)
            # Échelle non réglée : automatique (et non « Indisponible »), en %.
            if key == "display_scale":
                try:
                    return f"{round(float(value) * 100)} %" if value else _t("Automatique")
                except ValueError:
                    return value
            if key == "browser_default":
                return browser_default_text(status)
            return value or _t("Indisponible")
        text = "\n".join(_t("{label} : {value}").format(label=label, value=shown(key)) for label, key in fields)
        if self.page == 7: text = read("/run/winlux/updates.txt")
        if self.install_button is not None:
            self.install_button.get_parent().set_visible("Winlux est à jour." not in text)
        if self.rollback_button is not None:
            # Rien à restaurer tant qu'aucune version précédente n'est conservée.
            self.rollback_button.get_parent().set_visible(bool(EXTRA.previous_version_kept()))
        if self.page == 7:
            text = localize_update_text(text)
            # « Secours et restauration » relu avec l'encadré : un seul avis (#636).
            self.refresh_recovery()
        if self.page == 9: text += about_build(read("/usr/share/winlux/BUILD-INFO"))
        if self.page == 11:
            self.show_browser_buttons(status)
            waiting = browsers_waiting()
            if waiting:
                text += "\n" + _t("En attente d’Internet, installé au prochain démarrage connecté : {names}").format(names=", ".join(waiting))
        if self.page == GRAPHICS_PAGE: text = gpu_summary()
        if self.page == CPU_PAGE: text = cpu_summary()
        if self.page == PRIVACY_PAGE: text = privacy_summary(status)
        if self.page in (8, GRAPHICS_PAGE) and VIRTUAL_MACHINE:
            name = _VM_NAMES.get(VIRTUAL_MACHINE, "VM")
            text = _t("Machine virtuelle détectée ({name}) : affichage adapté à la VM (curseur logiciel).").format(name=name) + "\n" + text
        if self.state.get_text() != text: self.state.set_text(text)
        if self.process is None:
            try:
                fresh = os.stat(control_file("action.txt")).st_mtime >= getattr(self, "page_opened", 0) - 1
            except OSError:
                fresh = False
            self.notice.set_text(localize_update_text(read(control_file("action.txt")).strip()) if fresh else "")
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
                mib = _t("Mio")
                amount = f"{done / 1048576:.1f} {mib}" + (f" / {total / 1048576:.1f} {mib}" if total else "")
                item = f"{item} · {amount}" + (f" · {percent} %" if percent >= 0 else "")
            except ValueError: pass
        elif progress.get("phase") in ("backing-up", "installing"):
            try:
                done, total = int(progress.get("done", "0")), int(progress.get("total", "0"))
                if total: item = f"{progress.get('phase') == 'installing' and _t('Installation') or _t('Sauvegarde')} · {_t('{done}/{total} fichiers').format(done=done, total=total)} · {done * 100 // total} %\n{item}"
            except ValueError: pass
        self.progress.set_text(item)
        self.progress.set_visible(self.page == 7)
        if self.page == 7:
            journal = read(control_file("update.log"))[-12000:]
            if self.details.get_text() != journal: self.details.set_text(journal)
        return True

    def show_update_error(self, action, result):
        stages = {"checking": _t("Recherche et préparation"), "downloading": _t("Téléchargement"), "verifying": _t("Vérification"),
                  "extracting": _t("Décompression"), "backing-up": _t("Sauvegarde"),
                  "restore-point": _t("Point de restauration"),
                  "installing": _t("Installation"), "finalizing": _t("Finalisation"), "restoring": _t("Restauration")}
        phase = read("/run/winlux/update-error-phase").strip()
        stage = stages.get(phase, phase or {"update-check": _t("Recherche"), "update-apply": _t("Mise à jour"),
                                           "update-rollback": _t("Restauration")}[action])
        log = read(control_file("update.log")).strip()
        detail = log[-3000:] if log else _t("Aucun détail n’a été écrit dans le journal.")
        self.show_error(_t("La mise à jour a échoué"), _t("Étape : {stage} (code {code})\n\nDernières lignes du journal :\n{detail}").format(stage=stage, code=result, detail=detail))

    def show_info(self, title, detail):
        dialog = Gtk.MessageDialog(transient_for=self, modal=False, message_type=Gtk.MessageType.INFO,
                                   buttons=Gtk.ButtonsType.CLOSE, text=title)
        dialog.format_secondary_text(detail)
        dialog.connect("response", lambda window, _: window.destroy())
        dialog.show_all()

    def show_error(self, title, detail):
        dialog = Gtk.MessageDialog(transient_for=self, modal=False, message_type=Gtk.MessageType.ERROR,
                                   buttons=Gtk.ButtonsType.CLOSE, text=title)
        dialog.format_secondary_text(detail)
        for child in dialog.get_message_area().get_children():
            if isinstance(child, Gtk.Label): child.set_selectable(True)
        dialog.connect("response", lambda window, _: window.destroy())
        dialog.show_all()

# Ordre alphabétique : aucun navigateur mis en avant.
BROWSER_NAMES = {"chrome": "Google Chrome", "edge": "Microsoft Edge", "firefox": "Mozilla Firefox"}


def browser_buttons_shown(status, action):
    """« Définir par défaut » et « Ouvrir » seulement pour un navigateur installé
    (« Définir par défaut » pas pour celui qui l'est déjà) ; « Installer »
    seulement pour un navigateur absent (ou à mettre à jour)."""
    verb, _, ident = action[len("browser-"):].partition("-")
    installed = status.get(f"browser_{ident}") in ("installed", "update")
    if verb == "install":
        return not installed or status.get(f"browser_{ident}") == "update"
    if verb == "default" and status.get("browser_default") == ident:
        return False
    return installed


def browser_default_text(status):
    """Navigateur qui ouvre vraiment les liens (comme winlux-browser) : le
    défaut choisi, sinon le seul navigateur installé ; avec plusieurs
    navigateurs et aucun défaut, le choix est demandé à chaque lien."""
    default = status.get("browser_default")
    installed = [i for i in BROWSER_NAMES if status.get(f"browser_{i}") in ("installed", "update")]
    if default in installed:
        return BROWSER_NAMES[default]
    if len(installed) == 1:
        return _t("{name} (seul navigateur installé)").format(name=BROWSER_NAMES[installed[0]])
    if installed:
        return _t("Aucun : le navigateur vous sera demandé à chaque lien")
    return _t("Aucun : choisissez-en un ci-dessous")


def open_pending_link():
    """winlux-browser --pending : ouvre le lien gardé pendant le choix du navigateur."""
    try:
        subprocess.Popen(["/usr/bin/winlux-browser", "--pending"], stdin=subprocess.DEVNULL,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    except OSError:
        pass


def browsers_waiting():
    """Navigateurs demandés sans réseau, que l'init installera (winlux-browsers)."""
    names = BROWSER_NAMES
    ids = read("/etc/winlux/browser-install-pending").split() + read("/etc/winlux/first-boot-browser").split()[:1]
    return [names[i] for n, i in enumerate(ids) if i in names and i not in ids[:n]]


class SettingsApp(Gtk.Application):
    """Une seule fenêtre Paramètres, comme sous Windows : un nouveau lancement
    (menu Démarrer, raccourci, autre page) la ramène devant et ouvre la page
    demandée au lieu d'empiler une copie."""
    def __init__(self):
        super().__init__(application_id="org.winlux.Settings", flags=Gio.ApplicationFlags.HANDLES_COMMAND_LINE | Gio.ApplicationFlags.SEND_ENVIRONMENT)
        self.window = None

    def do_command_line(self, line):
        page = line.getenv("WINLUX_SETTINGS_PAGE") or ""
        if line.getenv("WINLUX_INSTALLED_FILTER"): os.environ["WINLUX_INSTALLED_FILTER"] = line.getenv("WINLUX_INSTALLED_FILTER")
        if self.window is None:
            if page: os.environ["WINLUX_SETTINGS_PAGE"] = page
            self.window = Settings()
            self.add_window(self.window)
        elif page.isdigit():
            self.window.open_page(int(page))
        self.window.present()
        return 0

# `winlux-settings about`, `winlux-settings 9`, `ms-settings:about`… : page
# ouverte d'emblée (mêmes numéros que WINLUX_SETTINGS_PAGE).
PAGE_NAMES = {
    "system": 0, "systeme": 0, "apps": 1, "applications": 1, "network": 2, "reseau": 2, "network-status": 2,
    "sound": 16, "son": 16, "keyboard": 4, "clavier": 4, "mouse": 5, "souris": 5,
    "personalization": 6, "personnalisation": 6, "windowsupdate": 7, "updates": 7, "mises-a-jour": 7,
    "display": 8, "ecran": 8, "about": 9, "apropos": 9, "a-propos": 9, "browsers": 11, "navigateurs": 11,
    "defaultapps": 11, "easeofaccess": 12, "accessibility": 12, "accessibilite": 12, "privacy": 18,
    "confidentialite": 18, "bluetooth": 22, "power": 23, "powersleep": 23, "batterysaver": 23,
    "alimentation": 23, "graphics": 24, "graphismes": 24, "printers": 26, "imprimantes": 26,
    "gaming": 29, "jeux": 29, "recovery": 31, "recuperation": 31, "clipboard": 32,
    "presse-papiers": 32, "accounts": 33, "yourinfo": 33, "otherusers": 33, "comptes": 33,
    "hardware": 34, "drivers": 34, "pilotes": 34,
    "backup": 38, "sauvegarde": 38, "filehistory": 38,
    # ms-settings:appsfeatures ouvre « Applications installées » (#722).
    "appsfeatures": 39, "installed-apps": 39, "installedapps": 39, "applications-installees": 39,
}


def page_argument(value):
    """Numéro de page pour un argument de la ligne de commande, sinon None."""
    value = fold(value.strip())
    for prefix in ("ms-settings:", "winlux-settings:"):
        if value.startswith(prefix):
            value = value[len(prefix):]
    value = value.strip("/").replace("_", "-").replace(" ", "-")
    if value.isdigit():
        return int(value) if int(value) < len(PAGES) else None
    return PAGE_NAMES.get(value, PAGE_NAMES.get(value.replace("-", "")))


if __name__ == "__main__":
    if len(sys.argv) > 1:
        page = page_argument(sys.argv[1])
        if page is None:
            print(f"winlux-settings : page inconnue « {sys.argv[1]} »", file=sys.stderr)
        else:
            os.environ["WINLUX_SETTINGS_PAGE"] = str(page)
    raise SystemExit(SettingsApp().run(None))

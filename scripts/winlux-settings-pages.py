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
from gi.repository import GdkPixbuf, Gio, GLib, Gtk


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
    # Jeux (application Jeux, temps de jeu, dossiers de jeux)
    'Bibliothèque de jeux': ('Game library', 'Biblioteca de juegos', 'Spielebibliothek'),
    'L’application Jeux rassemble vos jeux Windows installés : GOG, itch.io, dossiers de jeux et programmes 3D.': ('The Games app gathers your installed Windows games: GOG, itch.io, game folders and 3D programs.', 'La aplicación Juegos reúne sus juegos de Windows instalados: GOG, itch.io, carpetas de juegos y programas 3D.', 'Die App „Spiele“ sammelt Ihre installierten Windows-Spiele: GOG, itch.io, Spieleordner und 3D-Programme.'),
    'Ouvrir Jeux': ('Open Games', 'Abrir Juegos', 'Spiele öffnen'),
    'Rechercher les nouveaux jeux à l’ouverture': ('Look for new games when opening', 'Buscar juegos nuevos al abrir', 'Beim Öffnen nach neuen Spielen suchen'),
    'Compter le temps de jeu': ('Count play time', 'Contar el tiempo de juego', 'Spielzeit zählen'),
    'Enregistré sur ce PC uniquement, pour chaque jeu lancé.': ('Stored on this PC only, for each game started.', 'Se guarda solo en este equipo, para cada juego iniciado.', 'Nur auf diesem PC gespeichert, für jedes gestartete Spiel.'),
    'Effacer le temps de jeu': ('Clear play time', 'Borrar el tiempo de juego', 'Spielzeit löschen'),
    'Effacer le temps de jeu de tous les jeux ?': ('Clear the play time of every game?', '¿Borrar el tiempo de juego de todos los juegos?', 'Spielzeit aller Spiele löschen?'),
    'Dossiers de jeux': ('Game folders', 'Carpetas de juegos', 'Spieleordner'),
    'Toujours parcourus : C:\\Games, C:\\Jeux, C:\\GOG Games, Program Files et les jeux itch.io. Ajoutez ici un disque ou un dossier où vous rangez d’autres jeux.': ('Always searched: C:\\Games, C:\\Jeux, C:\\GOG Games, Program Files and itch.io games. Add a drive or folder here where you keep other games.', 'Siempre se buscan: C:\\Games, C:\\Jeux, C:\\GOG Games, Program Files y los juegos de itch.io. Agregue aquí un disco o una carpeta donde guarde otros juegos.', 'Immer durchsucht: C:\\Games, C:\\Jeux, C:\\GOG Games, Program Files und itch.io-Spiele. Fügen Sie hier ein Laufwerk oder einen Ordner mit weiteren Spielen hinzu.'),
    'Aucun dossier ajouté.': ('No folder added.', 'No se agregó ninguna carpeta.', 'Kein Ordner hinzugefügt.'),
    'Ajouter un dossier…': ('Add a folder…', 'Agregar una carpeta…', 'Ordner hinzufügen…'),
    'Retirer': ('Remove', 'Quitar', 'Entfernen'),
    'Télécharger': ('Download', 'Descargar', 'Herunterladen'),
    'Jeux retirés de la bibliothèque': ('Games removed from the library', 'Juegos quitados de la biblioteca', 'Aus der Bibliothek entfernte Spiele'),
    'Remettre': ('Restore', 'Restaurar', 'Wiederherstellen'),
    'Performances en jeu': ('In-game performance', 'Rendimiento en juego', 'Leistung im Spiel'),
    'Le mode jeu, le choix de la carte graphique et l’overlay de performances s’appliquent tout seuls à chaque jeu.': ('Game mode, graphics card choice and the performance overlay apply automatically to every game.', 'El modo juego, la elección de la tarjeta gráfica y la superposición de rendimiento se aplican solos a cada juego.', 'Spielmodus, Grafikkartenwahl und Leistungs-Overlay gelten automatisch für jedes Spiel.'),
    'Mode jeu (Processeur)': ('Game mode (Processor)', 'Modo juego (Procesador)', 'Spielmodus (Prozessor)'),
    'Overlay et carte graphique (Graphismes)': ('Overlay and graphics card (Graphics)', 'Superposición y tarjeta gráfica (Gráficos)', 'Overlay und Grafikkarte (Grafik)'),
    # Refonte des Paramètres (catégories, réglages intégrés)
    'Aucun emplacement d’export pour l’instant : branchez une clé USB ou ouvrez Documents.': ('No export location yet: plug in a USB drive or open Documents.', 'Todavía no hay ubicación de exportación: conecte una memoria USB o abra Documentos.', 'Noch kein Exportziel: Schließen Sie einen USB-Stick an oder öffnen Sie Dokumente.'),
    'Priorité du processeur': ('Processor priority', 'Prioridad del procesador', 'Prozessorpriorität'),
    'Cœurs utilisés': ('Cores used', 'Núcleos usados', 'Verwendete Kerne'),
    'Basse': ('Low', 'Baja', 'Niedrig'),
    'Haute': ('High', 'Alta', 'Hoch'),
    'Temps réel': ('Realtime', 'Tiempo real', 'Echtzeit'),
    'Cœurs performants (P)': ('Performance cores (P)', 'Núcleos de rendimiento (P)', 'Leistungskerne (P)'),
    'Cœurs économes (E)': ('Efficient cores (E)', 'Núcleos eficientes (E)', 'Effizienzkerne (E)'),
    'Maj + F12': ('Shift + F12', 'Mayús + F12', 'Umschalt + F12'),
    'Ctrl + Maj + O': ('Ctrl + Shift + O', 'Ctrl + Mayús + O', 'Strg + Umschalt + O'),
    'Actualiser': ('Refresh', 'Actualizar', 'Aktualisieren'),
    'Adresse': ('Address', 'Dirección', 'Adresse'),
    'Affichage seul (sans souris ni clavier)': ('View only (no mouse or keyboard)', 'Solo ver (sin mouse ni teclado)', 'Nur anzeigen (ohne Maus und Tastatur)'),
    'Afficher au lancement d’un jeu': ('Show when a game starts', 'Mostrar al iniciar un juego', 'Beim Spielstart anzeigen'),
    'Afficher l’overlay dans les jeux': ('Show the overlay in games', 'Mostrar la superposición en los juegos', 'Overlay in Spielen anzeigen'),
    'Ajoute l’icône sur le bureau ; un clic droit sur l’icône la retire.': ('Adds the icon to the desktop; right-click the icon to remove it.', 'Agrega el icono al escritorio; haga clic derecho en el icono para quitarlo.', 'Fügt das Symbol zum Desktop hinzu; ein Rechtsklick auf das Symbol entfernt es.'),
    'Ajouter': ('Add', 'Agregar', 'Hinzufügen'),
    'Ajouter une imprimante': ('Add a printer', 'Agregar una impresora', 'Drucker hinzufügen'),
    'Annuler le dernier import': ('Undo the last import', 'Deshacer la última importación', 'Letzten Import rückgängig machen'),
    'Annuler les impressions': ('Cancel print jobs', 'Cancelar las impresiones', 'Druckaufträge abbrechen'),
    'Applications par défaut': ('Default apps', 'Aplicaciones predeterminadas', 'Standard-Apps'),
    'Arrêter le partage': ('Stop sharing', 'Dejar de compartir', 'Freigabe beenden'),
    'Aucun programme ne démarre avec la session.': ('No program starts with the session.', 'Ningún programa se inicia con la sesión.', 'Kein Programm startet mit der Sitzung.'),
    'Aucun réseau Wi-Fi visible pour l’instant.': ('No Wi-Fi network visible right now.', 'Ninguna red Wi-Fi visible por ahora.', 'Zurzeit ist kein WLAN sichtbar.'),
    'Aucune imprimante pour l’instant.': ('No printer yet.', 'Todavía no hay impresoras.', 'Noch kein Drucker.'),
    'Aucune imprimante trouvée. Vérifiez qu’elle est allumée et branchée.': ('No printer found. Check that it is on and connected.', 'No se encontró ninguna impresora. Compruebe que esté encendida y conectada.', 'Kein Drucker gefunden. Prüfen Sie, ob er eingeschaltet und verbunden ist.'),
    'Aucune version précédente conservée.': ('No previous version kept.', 'No se conserva ninguna versión anterior.', 'Keine vorherige Version gespeichert.'),
    'Automatique (recommandé)': ('Automatic (recommended)', 'Automático (recomendado)', 'Automatisch (empfohlen)'),
    'Bureau à distance': ('Remote Desktop', 'Escritorio remoto', 'Remotedesktop'),
    'Ce PC': ('This PC', 'Este equipo', 'Dieser PC'),
    'Choisir un dossier': ('Choose a folder', 'Elegir una carpeta', 'Ordner auswählen'),
    'Parcourir…': ('Browse…', 'Examinar…', 'Durchsuchen…'),
    'Images': ('Pictures', 'Imágenes', 'Bilder'),
    'Choisissez « Winlux secours » au démarrage, ou démarrez sur la clé USB d’installation. Gardez une copie de vos documents sur un autre support.': ('Choose “Winlux recovery” at startup, or start from the installation USB drive. Keep a copy of your documents on another drive.', 'Elija «Winlux recuperación» al arrancar o inicie desde la memoria USB de instalación. Guarde una copia de sus documentos en otro soporte.', 'Wählen Sie beim Start „Winlux-Wiederherstellung“ oder starten Sie vom Installations-USB-Stick. Bewahren Sie eine Kopie Ihrer Dokumente auf einem anderen Datenträger auf.'),
    'Clic droit à deux doigts': ('Two-finger right-click', 'Clic derecho con dos dedos', 'Rechtsklick mit zwei Fingern'),
    'Compatibilité VNC classique': ('Classic VNC compatibility', 'Compatibilidad con VNC clásico', 'Klassische VNC-Kompatibilität'),
    'Comptes autorisés': ('Allowed accounts', 'Cuentas autorizadas', 'Zugelassene Konten'),
    'Conservée : le bouton Restaurer ci-dessus y revient.': ('Kept: the Restore button above goes back to it.', 'Conservada: el botón Restaurar de arriba vuelve a ella.', 'Gespeichert: Die Schaltfläche Wiederherstellen oben kehrt zu ihr zurück.'),
    'Corbeille': ('Recycle Bin', 'Papelera de reciclaje', 'Papierkorb'),
    'Courbe des ventilateurs': ('Fan curve', 'Curva de los ventiladores', 'Lüfterkurve'),
    'Cœurs performants et cœurs économes': ('Performance and efficient cores', 'Núcleos de rendimiento y núcleos eficientes', 'Leistungs- und Effizienzkerne'),
    'Dernier plantage': ('Last crash', 'Último bloqueo', 'Letzter Absturz'),
    'Documents': ('Documents', 'Documentos', 'Dokumente'),
    'Dossiers partagés': ('Shared folders', 'Carpetas compartidas', 'Freigegebene Ordner'),
    'Droits par compte : clic droit sur le dossier › « Partager ce dossier… ».': ('Per-account rights: right-click the folder › “Share this folder…”.', 'Permisos por cuenta: clic derecho en la carpeta › «Compartir esta carpeta…».', 'Rechte pro Konto: Rechtsklick auf den Ordner › „Diesen Ordner freigeben…“.'),
    'Défilement naturel': ('Natural scrolling', 'Desplazamiento natural', 'Natürliches Scrollen'),
    'Défiler avec deux doigts': ('Two-finger scrolling', 'Desplazar con dos dedos', 'Mit zwei Fingern scrollen'),
    'Démarrage': ('Startup', 'Inicio', 'Autostart'),
    'Désactiver pendant la frappe': ('Turn off while typing', 'Desactivar al escribir', 'Beim Tippen deaktivieren'),
    'Désinstaller': ('Uninstall', 'Desinstalar', 'Deinstallieren'),
    'Empreinte': ('Fingerprint', 'Huella', 'Fingerabdruck'),
    'En attente de connexion': ('Waiting for a connection', 'Esperando una conexión', 'Warten auf Verbindung'),
    'En bas à droite': ('Bottom right', 'Abajo a la derecha', 'Unten rechts'),
    'En bas à gauche': ('Bottom left', 'Abajo a la izquierda', 'Unten links'),
    'En haut à droite': ('Top right', 'Arriba a la derecha', 'Oben rechts'),
    'En haut à gauche': ('Top left', 'Arriba a la izquierda', 'Oben links'),
    'Enregistrer un fichier': ('Save a file', 'Guardar un archivo', 'Datei speichern'),
    'Envoi du rapport': ('Sending the report', 'Envío del informe', 'Senden des Berichts'),
    'Explorateur': ('File Explorer', 'Explorador', 'Explorer'),
    'Exporter mes réglages': ('Export my settings', 'Exportar mi configuración', 'Meine Einstellungen exportieren'),
    'Exporter vers': ('Export to', 'Exportar a', 'Exportieren nach'),
    'Icônes du bureau': ('Desktop icons', 'Iconos del escritorio', 'Desktopsymbole'),
    'Identifiant Flatpak, par exemple org.videolan.VLC': ('Flatpak ID, for example org.videolan.VLC', 'Identificador Flatpak, por ejemplo org.videolan.VLC', 'Flatpak-ID, zum Beispiel org.videolan.VLC'),
    'Importer': ('Import', 'Importar', 'Importieren'),
    'Importer ces réglages ?': ('Import these settings?', '¿Importar esta configuración?', 'Diese Einstellungen importieren?'),
    'Imprimante par défaut': ('Default printer', 'Impresora predeterminada', 'Standarddrucker'),
    'Imprimantes du réseau et USB trouvées par CUPS (pilote sans installation).': ('Network and USB printers found by CUPS (driverless).', 'Impresoras de red y USB encontradas por CUPS (sin instalar controlador).', 'Von CUPS gefundene Netzwerk- und USB-Drucker (ohne Treiberinstallation).'),
    'Imprimantes installées': ('Installed printers', 'Impresoras instaladas', 'Installierte Drucker'),
    'Installer': ('Install', 'Instalar', 'Installieren'),
    'Installer {name} ?': ('Install {name}?', '¿Instalar {name}?', '{name} installieren?'),
    'Désinstaller {name} ?': ('Uninstall {name}?', '¿Desinstalar {name}?', '{name} deinstallieren?'),
    'Installer des applications Linux': ('Install Linux apps', 'Instalar aplicaciones de Linux', 'Linux-Apps installieren'),
    'Lancer aussi au démarrage': ('Also start at sign-in', 'Iniciar también al arrancar', 'Auch beim Start ausführen'),
    'Langue et fuseau horaire : Heure et langue.': ('Language and time zone: Time & language.', 'Idioma y zona horaria: Hora e idioma.', 'Sprache und Zeitzone: Zeit und Sprache.'),
    'Le contenu suit les doigts, comme sur un téléphone.': ('Content follows your fingers, like on a phone.', 'El contenido sigue a los dedos, como en un teléfono.', 'Inhalt folgt den Fingern wie beim Telefon.'),
    'Lente': ('Slow', 'Lenta', 'Langsam'),
    'Les applications zooment comme avec Ctrl + molette.': ('Apps zoom as with Ctrl + wheel.', 'Las aplicaciones hacen zoom como con Ctrl + rueda.', 'Apps zoomen wie mit Strg + Mausrad.'),
    'Les programmes Windows s’installent en ouvrant leur installateur .exe.': ('Windows programs install by opening their .exe installer.', 'Los programas de Windows se instalan abriendo su instalador .exe.', 'Windows-Programme werden durch Öffnen ihres .exe-Installationsprogramms installiert.'),
    'Masquer les informations personnelles': ('Hide personal information', 'Ocultar la información personal', 'Persönliche Informationen ausblenden'),
    'Mode': ('Mode', 'Modo', 'Modus'),
    'Mode du processeur': ('Processor mode', 'Modo del procesador', 'Prozessormodus'),
    'Mot de passe : {password}': ('Password: {password}', 'Contraseña: {password}', 'Kennwort: {password}'),
    'Nom du paquet Ubuntu, par exemple vlc': ('Ubuntu package name, for example vlc', 'Nombre del paquete de Ubuntu, por ejemplo vlc', 'Ubuntu-Paketname, zum Beispiel vlc'),
    'Nom d’utilisateur, adresses et chemins personnels sont remplacés dans le rapport.': ('User name, addresses and personal paths are replaced in the report.', 'El nombre de usuario, las direcciones y las rutas personales se sustituyen en el informe.', 'Benutzername, Adressen und persönliche Pfade werden im Bericht ersetzt.'),
    'Nom invalide : {name}': ('Invalid name: {name}', 'Nombre no válido: {name}', 'Ungültiger Name: {name}'),
    'Normale': ('Normal', 'Normal', 'Normal'),
    'Ouvert': ('Open', 'Abierta', 'Offen'),
    'Overlay de performances en jeu': ('In-game performance overlay', 'Superposición de rendimiento en juego', 'Leistungs-Overlay im Spiel'),
    'P-cores : {p} · E-cores : {e}': ('P-cores: {p} · E-cores: {e}', 'Núcleos P: {p} · Núcleos E: {e}', 'P-Kerne: {p} · E-Kerne: {e}'),
    'Page de test': ('Test page', 'Página de prueba', 'Testseite'),
    'Paquet Linux (APT)': ('Linux package (APT)', 'Paquete de Linux (APT)', 'Linux-Paket (APT)'),
    'Par défaut': ('Set as default', 'Predeterminada', 'Als Standard'),
    'Par le navigateur': ('Through the browser', 'Mediante el navegador', 'Über den Browser'),
    'Partage de fichiers': ('File sharing', 'Uso compartido de archivos', 'Dateifreigabe'),
    'Partager un dossier…': ('Share a folder…', 'Compartir una carpeta…', 'Ordner freigeben…'),
    'Pavé tactile': ('Touchpad', 'Panel táctil', 'Touchpad'),
    'Performances': ('Performance', 'Rendimiento', 'Leistung'),
    'Performances : vitesse maximale. Économie : moins de chaleur et de bruit.': ('Performance: top speed. Power saver: less heat and noise.', 'Rendimiento: velocidad máxima. Ahorro: menos calor y ruido.', 'Leistung: Höchstgeschwindigkeit. Energiesparen: weniger Wärme und Lärm.'),
    'Permet de prendre la main sur ce PC depuis un autre ordinateur du réseau.': ('Lets another computer on the network take control of this PC.', 'Permite controlar este equipo desde otro ordenador de la red.', 'Ermöglicht die Steuerung dieses PCs von einem anderen Computer im Netzwerk.'),
    'Pincer pour zoomer': ('Pinch to zoom', 'Pellizcar para ampliar', 'Zum Zoomen zusammenziehen'),
    'Placement des programmes Windows': ('Placement of Windows programs', 'Ubicación de los programas de Windows', 'Platzierung von Windows-Programmen'),
    'Position': ('Position', 'Posición', 'Position'),
    'Processeur': ('Processor', 'Procesador', 'Prozessor'),
    'Priorité et cœurs d’un programme précis : Applications › Programmes Windows.': ('Priority and cores for a specific program: Apps › Windows programs.', 'Prioridad y núcleos de un programa concreto: Aplicaciones › Programas de Windows.', 'Priorität und Kerne eines bestimmten Programms: Apps › Windows-Programme.'),
    'Programmes lancés à l’ouverture de session.': ('Programs started when you sign in.', 'Programas que se inician al iniciar sesión.', 'Programme, die bei der Anmeldung starten.'),
    'Programmes lourds sur les P-cores': ('Heavy programs on P-cores', 'Programas pesados en los núcleos P', 'Schwere Programme auf P-Kernen'),
    'Proposer un rapport après un plantage': ('Offer a report after a crash', 'Proponer un informe tras un bloqueo', 'Nach einem Absturz einen Bericht anbieten'),
    'Préférences': ('Preferences', 'Preferencias', 'Einstellungen'),
    'Préparez un rapport (journal, matériel, programme en cause) pour les développeurs de Winlux.': ('Prepare a report (log, hardware, program involved) for the Winlux developers.', 'Prepare un informe (registro, hardware, programa afectado) para los desarrolladores de Winlux.', 'Erstellen Sie einen Bericht (Protokoll, Hardware, betroffenes Programm) für die Winlux-Entwickler.'),
    'Raccourci clavier': ('Keyboard shortcut', 'Método abreviado de teclado', 'Tastenkombination'),
    'Rapide': ('Fast', 'Rápida', 'Schnell'),
    'Rapports enregistrés': ('Saved reports', 'Informes guardados', 'Gespeicherte Berichte'),
    'Rechercher': ('Search', 'Buscar', 'Suchen'),
    'Refroidissement maximal': ('Maximum cooling', 'Refrigeración máxima', 'Maximale Kühlung'),
    'Se connecter': ('Connect', 'Conectar', 'Verbinden'),
    'Secours et restauration': ('Recovery', 'Recuperación', 'Wiederherstellung'),
    'Réinitialiser ce PC et clé de secours': ('Reset this PC and recovery drive', 'Restablecer este PC y unidad de recuperación', 'Diesen PC zurücksetzen und Wiederherstellungslaufwerk'),
    'Service arrêté': ('Service stopped', 'Servicio detenido', 'Dienst angehalten'),
    'Si le bureau ne démarre plus': ('If the desktop no longer starts', 'Si el escritorio ya no se inicia', 'Wenn der Desktop nicht mehr startet'),
    'Signaler un problème': ('Report a problem', 'Informar de un problema', 'Problem melden'),
    'Silencieux': ('Quiet', 'Silencioso', 'Leise'),
    'Sinon, appui dans le coin inférieur droit.': ('Otherwise, press the bottom-right corner.', 'Si no, pulse la esquina inferior derecha.', 'Sonst unten rechts drücken.'),
    'Sinon, défilement sur le bord du pavé.': ('Otherwise, scroll along the edge of the pad.', 'Si no, desplazamiento por el borde del panel.', 'Sonst Scrollen am Rand des Touchpads.'),
    'Taper pour cliquer': ('Tap to click', 'Tocar para hacer clic', 'Tippen zum Klicken'),
    'Toujours les E-cores': ('Always E-cores', 'Siempre los núcleos E', 'Immer E-Kerne'),
    'Toujours les P-cores': ('Always P-cores', 'Siempre los núcleos P', 'Immer P-Kerne'),
    'Tout mettre à jour': ('Update all', 'Actualizar todo', 'Alle aktualisieren'),
    'Transférer mes réglages': ('Transfer my settings', 'Transferir mi configuración', 'Meine Einstellungen übertragen'),
    'Un fichier .wlxprofile emporte vos réglages vers un autre PC Winlux.': ('A .wlxprofile file carries your settings to another Winlux PC.', 'Un archivo .wlxprofile lleva su configuración a otro equipo Winlux.', 'Eine .wlxprofile-Datei überträgt Ihre Einstellungen auf einen anderen Winlux-PC.'),
    'Une touche légère vaut un clic.': ('A light tap counts as a click.', 'Un toque ligero equivale a un clic.', 'Leichtes Tippen gilt als Klick.'),
    'Ventilateurs': ('Fans', 'Ventiladores', 'Lüfter'),
    'Version précédente': ('Previous version', 'Versión anterior', 'Vorherige Version'),
    'Visibles des autres PC (Windows, Mac, Linux) du réseau privé.': ('Visible to other PCs (Windows, Mac, Linux) on the private network.', 'Visibles para los demás equipos (Windows, Mac, Linux) de la red privada.', 'Für andere PCs (Windows, Mac, Linux) im privaten Netzwerk sichtbar.'),
    'Vitesse du curseur': ('Cursor speed', 'Velocidad del cursor', 'Cursorgeschwindigkeit'),
    'Vitesse du pointeur': ('Pointer speed', 'Velocidad del puntero', 'Zeigergeschwindigkeit'),
    'Wi-Fi': ('Wi-Fi', 'Wi-Fi', 'WLAN'),
    'ce PC': ('this PC', 'este equipo', 'dieser PC'),
    'jusqu’à {mhz} MHz': ('up to {mhz} MHz', 'hasta {mhz} MHz', 'bis zu {mhz} MHz'),
    '{count} connexion(s) en cours': ('{count} active connection(s)', '{count} conexión(es) activa(s)', '{count} aktive Verbindung(en)'),
    '{count} processeurs logiques': ('{count} logical processors', '{count} procesadores lógicos', '{count} logische Prozessoren'),
    '{rpm} tr/min': ('{rpm} RPM', '{rpm} RPM', '{rpm} U/min'),
    'Économie d’énergie': ('Power saver', 'Ahorro de energía', 'Energiesparmodus'),
    'Équilibré': ('Balanced', 'Equilibrado', 'Ausbalanciert'),
    'Évite les clics involontaires avec la paume.': ('Avoids accidental clicks with your palm.', 'Evita clics involuntarios con la palma.', 'Verhindert versehentliche Klicks mit dem Handballen.'),
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
    # Presse-papiers
    'Historique du presse-papiers': ('Clipboard history', 'Historial del portapapeles', 'Zwischenablageverlauf'),
    'Gardez plusieurs éléments copiés et retrouvez-les avec Windows + V.': ('Keep several copied items and find them with Windows + V.', 'Guarde varios elementos copiados y encuéntrelos con Windows + V.', 'Behalten Sie mehrere kopierte Elemente und finden Sie sie mit Windows + V.'),
    'Copier le texte d’une image': ('Copy text from an image', 'Copiar el texto de una imagen', 'Text aus einem Bild kopieren'),
    'Windows + Maj + T encadre une zone de l’écran et copie son texte ; la visionneuse d’images et le clic droit sur une image le proposent aussi.': (
        'Windows + Shift + T lets you draw a box on the screen and copies its text; the image viewer and right-clicking an image offer it too.',
        'Windows + Mayús + T enmarca una zona de la pantalla y copia su texto; el visor de imágenes y el clic derecho en una imagen también lo ofrecen.',
        'Windows + Umschalt + T markiert einen Bildschirmbereich und kopiert seinen Text; die Bildanzeige und der Rechtsklick auf ein Bild bieten es ebenfalls an.'),
    'Reconnaissance de texte installée': ('Text recognition installed', 'Reconocimiento de texto instalado', 'Texterkennung installiert'),
    'Reconnaissance de texte': ('Text recognition', 'Reconocimiento de texto', 'Texterkennung'),
    'Téléchargée à la première utilisation (environ 15 Mo), en français, anglais, espagnol et allemand.': (
        'Downloaded on first use (about 15 MB), in French, English, Spanish and German.',
        'Se descarga en el primer uso (unos 15 MB), en francés, inglés, español y alemán.',
        'Wird bei der ersten Verwendung heruntergeladen (etwa 15 MB), auf Französisch, Englisch, Spanisch und Deutsch.'),
    'français': ('French', 'francés', 'Französisch'),
    'anglais': ('English', 'inglés', 'Englisch'),
    'espagnol': ('Spanish', 'español', 'Spanisch'),
    'allemand': ('German', 'alemán', 'Deutsch'),
    'Automatique : clair le jour, sombre le soir': ('Automatic: light by day, dark in the evening', 'Automático: claro de día, oscuro por la noche', 'Automatisch: tagsüber hell, abends dunkel'),
    'Lever et coucher du soleil calculés pour votre fuseau horaire.': ('Sunrise and sunset calculated for your time zone.', 'Amanecer y atardecer calculados para su zona horaria.', 'Sonnenauf- und -untergang für Ihre Zeitzone berechnet.'),
    'Personnaliser l’arrière-plan': ('Personalize your background', 'Personalizar el fondo', 'Hintergrund anpassen'),
    'Image': ('Picture', 'Imagen', 'Bild'),
    'Diaporama': ('Slideshow', 'Presentación', 'Diashow'),
    'Winlux Aube le jour, Winlux Nuit le soir': ('Winlux Dawn by day, Winlux Night in the evening', 'Winlux Amanecer de día, Winlux Noche por la noche', 'Winlux Morgen tagsüber, Winlux Nacht am Abend'),
    'Dossier des images': ('Picture folder', 'Carpeta de imágenes', 'Bildordner'),
    'Aucun dossier choisi.': ('No folder chosen.', 'No se eligió ninguna carpeta.', 'Kein Ordner ausgewählt.'),
    'Changer d’image toutes les': ('Change picture every', 'Cambiar de imagen cada', 'Bild wechseln alle'),
    '1 minute': ('1 minute', '1 minuto', '1 Minute'),
    '10 minutes': ('10 minutes', '10 minutos', '10 Minuten'),
    '30 minutes': ('30 minutes', '30 minutos', '30 Minuten'),
    '1 heure': ('1 hour', '1 hora', '1 Stunde'),
    '6 heures': ('6 hours', '6 horas', '6 Stunden'),
    '1 jour': ('1 day', '1 día', '1 Tag'),
    'Ordre aléatoire': ('Shuffle the picture order', 'Orden aleatorio', 'Zufällige Reihenfolge'),
    'Image suivante': ('Next picture', 'Imagen siguiente', 'Nächstes Bild'),
    'Thèmes': ('Themes', 'Temas', 'Designs'),
    'Enregistrez votre fond, votre couleur d’accentuation, la taille de la barre et les sons en un fichier à partager.': (
        'Save your background, accent color, taskbar size and sounds in one file to share.',
        'Guarde su fondo, su color de énfasis, el tamaño de la barra y los sonidos en un archivo para compartir.',
        'Speichern Sie Hintergrund, Akzentfarbe, Taskleistengröße und Sounds in einer Datei zum Teilen.'),
    'Thèmes Winlux': ('Winlux themes', 'Temas de Winlux', 'Winlux-Designs'),
    'Enregistrer le thème': ('Save theme', 'Guardar tema', 'Design speichern'),
    'Enregistrer le thème…': ('Save theme…', 'Guardar tema…', 'Design speichern…'),
    'Appliquer un thème': ('Apply a theme', 'Aplicar un tema', 'Design anwenden'),
    'Appliquer un thème…': ('Apply a theme…', 'Aplicar un tema…', 'Design anwenden…'),
    'Mon thème': ('My theme', 'Mi tema', 'Mein Design'),
    'Effacer les données du presse-papiers': ('Clear clipboard data', 'Borrar los datos del portapapeles', 'Zwischenablagedaten löschen'),
    'Efface tout l’historique, sauf les éléments épinglés.': ('Clears the whole history except pinned items.', 'Borra todo el historial excepto los elementos anclados.', 'Löscht den gesamten Verlauf außer angehefteten Elementen.'),
    'Effacer': ('Clear', 'Borrar', 'Löschen'),
    # Éclairage nocturne (Écran)
    'Éclairage nocturne': ('Night light', 'Luz nocturna', 'Nachtmodus'),
    'Des couleurs plus chaudes le soir reposent les yeux et aident à s’endormir.': ('Warmer colours in the evening rest your eyes and help you fall asleep.', 'Los colores más cálidos por la noche descansan la vista y ayudan a dormir.', 'Wärmere Farben am Abend schonen die Augen und helfen beim Einschlafen.'),
    'Activer maintenant': ('Turn on now', 'Activar ahora', 'Jetzt einschalten'),
    'Intensité': ('Strength', 'Intensidad', 'Stärke'),
    'Planifier l’éclairage nocturne': ('Schedule night light', 'Programar la luz nocturna', 'Nachtmodus planen'),
    'Jamais': ('Never', 'Nunca', 'Nie'),
    'Du coucher au lever du soleil ({sunset} – {sunrise})': ('Sunset to sunrise ({sunset} – {sunrise})', 'Del atardecer al amanecer ({sunset} – {sunrise})', 'Sonnenuntergang bis Sonnenaufgang ({sunset} – {sunrise})'),
    'Définir les heures': ('Set hours', 'Establecer horas', 'Uhrzeiten festlegen'),
    'Activer à': ('Turn on at', 'Activar a las', 'Einschalten um'),
    'Désactiver à': ('Turn off at', 'Desactivar a las', 'Ausschalten um'),
    'Heures calculées pour votre fuseau horaire, sans connexion à Internet.': ('Times calculated for your time zone, without an Internet connection.', 'Horas calculadas para su zona horaria, sin conexión a Internet.', 'Zeiten für Ihre Zeitzone berechnet, ohne Internetverbindung.'),
    'Fuseau horaire sans coordonnées : 20:00 – 07:00 par défaut.': ('Time zone without coordinates: 20:00 – 07:00 by default.', 'Zona horaria sin coordenadas: 20:00 – 07:00 por defecto.', 'Zeitzone ohne Koordinaten: standardmäßig 20:00 – 07:00.'),
    # Widgets (Windows + W)
    'Widgets': ('Widgets', 'Widgets', 'Widgets'),
    'Windows + W ouvre le volet : météo, agenda, état du PC, pense-bête, photos et minuteur.': ('Windows + W opens the panel: weather, agenda, PC status, sticky note, photos and timer.', 'Windows + W abre el panel: tiempo, agenda, estado del equipo, nota adhesiva, fotos y temporizador.', 'Windows + W öffnet den Bereich: Wetter, Termine, PC-Status, Notizzettel, Fotos und Timer.'),
    'Bouton Widgets dans la barre des tâches': ('Widgets button on the taskbar', 'Botón Widgets en la barra de tareas', 'Widgets-Schaltfläche in der Taskleiste'),
    'Affiche la météo ; un clic ouvre les widgets.': ('Shows the weather; a click opens the widgets.', 'Muestra el tiempo; un clic abre los widgets.', 'Zeigt das Wetter; ein Klick öffnet die Widgets.'),
    'Lieu de la météo': ('Weather location', 'Ubicación del tiempo', 'Wetterort'),
    'Vide : d’après la connexion Internet. Données de wttr.in.': ('Empty: based on the Internet connection. Data from wttr.in.', 'Vacío: según la conexión a Internet. Datos de wttr.in.', 'Leer: anhand der Internetverbindung. Daten von wttr.in.'),
    'Unité de température': ('Temperature unit', 'Unidad de temperatura', 'Temperatureinheit'),
    'Celsius (°C)': ('Celsius (°C)', 'Celsius (°C)', 'Celsius (°C)'),
    'Fahrenheit (°F)': ('Fahrenheit (°F)', 'Fahrenheit (°F)', 'Fahrenheit (°F)'),
    'Dossier des photos': ('Photos folder', 'Carpeta de fotos', 'Fotoordner'),
    'Choisir un dossier…': ('Choose a folder…', 'Elegir una carpeta…', 'Ordner auswählen…'),
    'Images du compte': ('Account pictures', 'Imágenes de la cuenta', 'Bilder des Kontos'),
    'Ouvrir les widgets': ('Open widgets', 'Abrir widgets', 'Widgets öffnen'),
    'Rétablir les widgets par défaut': ('Restore default widgets', 'Restablecer los widgets predeterminados', 'Standard-Widgets wiederherstellen'),
    # Assistant Claude (#286)
    'Assistant Claude dans la barre des tâches': ('Claude assistant on the taskbar', 'Asistente Claude en la barra de tareas', 'Claude-Assistent in der Taskleiste'),
    'Un bouton Claude dans la barre des tâches, et Windows+C, pour poser une question sur Winlux : pourquoi un programme .exe ne s’ouvre pas, où est un fichier, comment changer un réglage.': ('A Claude button on the taskbar, and Windows+C, to ask a question about Winlux: why an .exe program won’t open, where a file is, how to change a setting.', 'Un botón Claude en la barra de tareas, y Windows+C, para preguntar sobre Winlux: por qué no se abre un programa .exe, dónde está un archivo, cómo cambiar un ajuste.', 'Eine Claude-Schaltfläche in der Taskleiste und Windows+C, um Fragen zu Winlux zu stellen: warum ein .exe-Programm nicht startet, wo eine Datei liegt, wie man eine Einstellung ändert.'),
    'À l’activation, Claude Code est téléchargé s’il manque (environ 300 Mo), puis vous vous connectez à votre compte Claude (Google, e-mail…).': ('When you turn it on, Claude Code is downloaded if needed (about 300 MB), then you sign in to your Claude account (Google, email…).', 'Al activarlo, se descarga Claude Code si falta (unos 300 MB) y luego inicia sesión en su cuenta de Claude (Google, correo…).', 'Beim Aktivieren wird Claude Code bei Bedarf heruntergeladen (etwa 300 MB), dann melden Sie sich bei Ihrem Claude-Konto an (Google, E-Mail…).'),
    'Compte Claude': ('Claude account', 'Cuenta de Claude', 'Claude-Konto'),
    'Vérification de la connexion…': ('Checking the sign-in…', 'Comprobando el inicio de sesión…', 'Anmeldung wird geprüft…'),
    'Connecté': ('Signed in', 'Sesión iniciada', 'Angemeldet'),
    'Connecté : {account}': ('Signed in: {account}', 'Sesión iniciada: {account}', 'Angemeldet: {account}'),
    'Non connecté': ('Not signed in', 'Sin iniciar sesión', 'Nicht angemeldet'),
    'Claude Code n’est pas installé': ('Claude Code is not installed', 'Claude Code no está instalado', 'Claude Code ist nicht installiert'),
    'Ouvrir l’assistant': ('Open the assistant', 'Abrir el asistente', 'Assistent öffnen'),
    'Se connecter': ('Sign in', 'Iniciar sesión', 'Anmelden'),
    'Se déconnecter': ('Sign out', 'Cerrar sesión', 'Abmelden'),
    'Ce qui est envoyé': ('What is sent', 'Qué se envía', 'Was gesendet wird'),
    'Seuls votre question et la conversation en cours partent vers Claude (Anthropic). Rien n’est gardé sur ce PC une fois la fenêtre fermée.': ('Only your question and the current conversation go to Claude (Anthropic). Nothing is kept on this PC once the window is closed.', 'Solo su pregunta y la conversación actual se envían a Claude (Anthropic). No se guarda nada en este equipo al cerrar la ventana.', 'Nur Ihre Frage und das aktuelle Gespräch gehen an Claude (Anthropic). Nach dem Schließen des Fensters bleibt nichts auf diesem PC gespeichert.'),
    'Claude ne lit rien et ne change rien tout seul : chaque lecture (journal d’un programme, recherche de fichier) vous montre le texte exact avant l’envoi, et chaque action attend votre clic.': ('Claude reads and changes nothing on its own: each read (a program’s log, a file search) shows you the exact text before it is sent, and each action waits for your click.', 'Claude no lee ni cambia nada por sí solo: cada lectura (registro de un programa, búsqueda de archivos) le muestra el texto exacto antes del envío, y cada acción espera su clic.', 'Claude liest und ändert nichts von selbst: Jeder Lesezugriff (Protokoll eines Programms, Dateisuche) zeigt Ihnen vor dem Senden den genauen Text, und jede Aktion wartet auf Ihren Klick.'),
    'La connexion à votre compte est gardée par Claude Code dans votre dossier personnel, lisible par vous seul.': ('The sign-in to your account is kept by Claude Code in your home folder, readable only by you.', 'El inicio de sesión de su cuenta lo guarda Claude Code en su carpeta personal, legible solo por usted.', 'Die Anmeldung bei Ihrem Konto speichert Claude Code in Ihrem persönlichen Ordner, nur für Sie lesbar.'),
    'Désactivé, l’assistant n’apparaît nulle part et rien ne tourne.': ('When off, the assistant appears nowhere and nothing runs.', 'Desactivado, el asistente no aparece en ningún sitio y nada se ejecuta.', 'Ausgeschaltet erscheint der Assistent nirgends und nichts läuft.'),
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
PAGE_APPS, PAGE_INPUT, PAGE_REPORT, PAGE_PRINTERS, PAGE_REMOTE, PAGE_SHARING = 1, 4, 25, 26, 27, 28
PAGE_GAMES = 29
PAGE_ASSISTANT = 30
PAGE_RECOVERY = 31
PAGE_CLIPBOARD = 32
PAGE_HARDWARE = 33

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


# ── Contraste du texte (WCAG) : couleurs calculées plutôt que figées ─────────

def _rgb(color):
    color = color.lstrip("#")
    return tuple(int(color[i:i + 2], 16) for i in (0, 2, 4))


def luminance(color):
    def channel(value):
        value /= 255
        return value / 12.92 if value <= 0.03928 else ((value + 0.055) / 1.055) ** 2.4
    r, g, b = _rgb(color)
    return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)


def contrast(first, second):
    lighter, darker = sorted((luminance(first), luminance(second)), reverse=True)
    return (lighter + 0.05) / (darker + 0.05)


def mix(first, second, amount):
    """Mélange first → second (amount de 0 à 1)."""
    return "#%02x%02x%02x" % tuple(round(a + (b - a) * amount) for a, b in zip(_rgb(first), _rgb(second)))


def text_on(background):
    """Texte noir ou blanc, celui qui se lit le mieux sur ce fond."""
    return "#000000" if contrast("#000000", background) >= contrast("#ffffff", background) else "#ffffff"


def readable(color, background, minimum=4.5):
    """color assombrie ou éclaircie juste assez pour atteindre minimum:1 sur background."""
    target = text_on(background)
    step = 0
    while contrast(color, background) < minimum and step < 20:
        step += 1
        color = mix(color, target, 0.1)
    return color if contrast(color, background) >= minimum else target


def settings_palette(dark, accent="#8178b7"):
    """Couleurs de Paramètres (thème clair ou sombre), texte toujours à 4,5:1 au moins."""
    if dark:
        p = {"bg": "#181b24", "fg": "#ecedf5", "side": "#202430", "hover": "#30374a", "card": "#242a38",
             "card_hover": "#2c3344", "entry": "#171b25", "border": "#48536b", "button": "#30384b",
             "button_hover": "#414b64", "disabled_fg": "#858da0", "disabled_bg": "#252a36", "dim": "#adb6c9"}
    else:
        p = {"bg": "#f3f5fa", "fg": "#1f2330", "side": "#e9ecf4", "hover": "#dce1ec", "card": "#ffffff",
             "card_hover": "#eef1f7", "entry": "#ffffff", "border": "#b9c0d1", "button": "#ffffff",
             "button_hover": "#e6eaf3", "disabled_fg": "#7d8496", "disabled_bg": "#eceef3", "dim": "#4f566b"}
    p["accent"] = accent
    # Ligne sélectionnée teintée par l'accent, texte noir ou blanc selon le fond obtenu.
    p["selected"] = mix(p["side"], accent, 0.45)
    p["selected_fg"] = text_on(p["selected"])
    p["selected_dim"] = p["selected_fg"]
    for key in ("dim",):
        for surface in ("bg", "side", "card", "hover"):
            p[key] = readable(p[key], p[surface])
    return p
EXE_SCALES = (("auto", "Automatique (comme l’écran)"), ("1", "100 %"), ("1.25", "125 %"), ("1.5", "150 %"), ("2", "200 %"))
# Pavé tactile : (clé touchpad_*, titre, explication), comme la page PE.
TOUCHPAD = (("tap", "Taper pour cliquer", "Une touche légère vaut un clic."),
            ("twofinger", "Défiler avec deux doigts", "Sinon, défilement sur le bord du pavé."),
            ("clickfinger", "Clic droit à deux doigts", "Sinon, appui dans le coin inférieur droit."),
            ("natural", "Défilement naturel", "Le contenu suit les doigts, comme sur un téléphone."),
            ("dwt", "Désactiver pendant la frappe", "Évite les clics involontaires avec la paume."))
CPU_PROFILES = {"performance": "Performances", "balanced": "Équilibré", "powersave": "Économie d’énergie"}
CPU_HYBRID = (("auto", "Automatique (recommandé)"), ("heavy", "Programmes lourds sur les P-cores"),
              ("performance", "Toujours les P-cores"), ("efficiency", "Toujours les E-cores"))
CPU_FANS = (("auto", "Automatique"), ("quiet", "Silencieux"), ("balanced", "Équilibré"), ("performance", "Refroidissement maximal"))
OVERLAY_KEYS = (("shift-f12", "Maj + F12"), ("ctrl-shift-o", "Ctrl + Maj + O"), ("alt-f12", "Alt + F12"))
OVERLAY_POSITIONS = (("top-left", "En haut à gauche"), ("top-right", "En haut à droite"),
                     ("bottom-left", "En bas à gauche"), ("bottom-right", "En bas à droite"))
REPORT_MODES = (("auto", "Automatique"), ("browser", "Par le navigateur"), ("file", "Enregistrer un fichier"))
EXE_PRIORITIES = (("auto", "Automatique"), ("low", "Basse"), ("normal", "Normale"), ("high", "Haute"), ("realtime", "Temps réel"))
EXE_CORES = (("auto", "Automatique"), ("performance", "Cœurs performants (P)"), ("efficiency", "Cœurs économes (E)"))
EXE_GPUS = (("auto", "Automatique"), ("integrated", "GPU économie d’énergie"), ("dedicated", "GPU hautes performances"))


def read(path):
    try: return Path(path).read_text(errors="replace")
    except OSError: return ""


def keyvals(text):
    return dict(line.split("=", 1) for line in text.splitlines() if "=" in line)


def have(command):
    return shutil.which(command) is not None


def ambiance_tool():
    """winlux-ambiance installé, ou le script du dépôt à côté de celui-ci."""
    if have("winlux-ambiance"): return ["winlux-ambiance"]
    script = Path(__file__).resolve().with_name("winlux-ambiance.py")
    return ["/usr/bin/python3", str(script)] if script.is_file() else None


def clipboard_tool():
    if have("winlux-clipboard"): return ["winlux-clipboard"]
    script = Path(__file__).resolve().with_name("winlux-clipboard.py")
    return ["/usr/bin/python3", str(script)] if script.is_file() else None


def ocr_tool():
    if have("winlux-ocr"): return ["winlux-ocr"]
    script = Path(__file__).resolve().with_name("winlux-ocr.py")
    return ["/usr/bin/python3", str(script)] if script.is_file() else None


def ambiance_status():
    tool = ambiance_tool()
    result = run(tool + ["status"]) if tool else None
    return keyvals(result.stdout) if result else {}


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
    if page == PAGE_REPORT:
        return have("winlux-report")
    if page == PAGE_PRINTERS:
        return have("lpstat")
    if page == PAGE_REMOTE:
        return have("winlux-remote-config") or (RUN / "remote.txt").is_file()
    if page == PAGE_CLIPBOARD:
        return clipboard_tool() is not None
    if page == PAGE_SHARING:
        return have("winlux-share") or have("winlux-transfer") or (RUN / "shares.txt").is_file()
    if page == PAGE_GAMES:
        return games_module() is not None
    if page == PAGE_ASSISTANT:
        return assistant_module() is not None
    if page == PAGE_RECOVERY:
        module = reset_page()
        return module is not None and module.available()
    if page == PAGE_HARDWARE:
        module = hardware_page()
        return module is not None and module.available()
    return True


GAMES_APP = Path(os.environ.get("WINLUX_GAMES_APP", "/usr/share/winlux/native/jeux/winlux-games.py"))


def games_module():
    """Application Jeux (integration/native/jeux), installée à côté ou dans le dépôt."""
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    for candidate in (GAMES_APP, here.parent.parent / "integration/native/jeux/winlux-games.py"):
        if candidate.is_file():
            try:
                loader = SourceFileLoader("winlux_games", str(candidate))
                spec = importlib.util.spec_from_loader("winlux_games", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                module.APP_PATH = candidate
                return module
            except Exception:
                return None
    return None


_ASSISTANT = []


def assistant_module():
    """winlux-assistant (installé à côté, ou scripts/ du dépôt), chargé une fois."""
    if _ASSISTANT: return _ASSISTANT[0]
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    module = None
    for candidate in (here.with_name("winlux-assistant.py"), here.with_name("winlux-assistant")):
        if candidate.is_file():
            try:
                loader = SourceFileLoader("winlux_assistant", str(candidate))
                spec = importlib.util.spec_from_loader("winlux_assistant", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                break
            except Exception:
                module = None
    _ASSISTANT.append(module)
    return module
def reset_page():
    """Page Récupération (winlux-reset-page, installée à côté), None si absente."""
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    for candidate in (here.with_name("winlux-reset-page.py"), here.with_name("winlux-reset-page")):
        if candidate.is_file():
            try:
                loader = SourceFileLoader("winlux_reset_page", str(candidate))
                spec = importlib.util.spec_from_loader("winlux_reset_page", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module
            except Exception:
                return None
    return None


_HARDWARE = []


def hardware_page():
    """Matériel et pilotes (winlux-hardware-page, installé à côté), None si absent."""
    if _HARDWARE: return _HARDWARE[0]
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    module = None
    for candidate in (here.with_name("winlux-hardware-page.py"), here.with_name("winlux-hardware-page")):
        if candidate.is_file():
            try:
                loader = SourceFileLoader("winlux_hardware_page", str(candidate))
                spec = importlib.util.spec_from_loader("winlux_hardware_page", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                break
            except Exception:
                module = None
    _HARDWARE.append(module)
    return module


def default_apps_panel(window):
    """Applications par défaut (winlux-default-apps, installé à côté) dans la page."""
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    for candidate in (here.with_name("winlux-default-apps.py"), here.with_name("winlux-default-apps")):
        if candidate.is_file():
            try:
                loader = SourceFileLoader("winlux_default_apps", str(candidate))
                spec = importlib.util.spec_from_loader("winlux_default_apps", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module.DefaultAppsPanel(window)
            except Exception:
                return None
    return None


class ExtraPages:
    """Mélangé à la fenêtre Settings : self.body, self.execute, self.show_error."""

    EXTRA_PAGES = {PAGE_PROGRAMS: "page_programs", PAGE_ACCESSIBILITY: "page_accessibility",
                   PAGE_EFFECTS: "page_effects", PAGE_LOCALE: "page_locale", PAGE_FILES: "page_files",
                   PAGE_SOUND: "page_sound", PAGE_SYSUPDATE: "page_sysupdate", PAGE_NETDRIVES: "page_netdrives",
                   PAGE_DESKTOP: "page_desktop", PAGE_BLUETOOTH: "page_bluetooth", PAGE_APPS: "page_apps",
                   PAGE_INPUT: "page_input", PAGE_REPORT: "page_report", PAGE_PRINTERS: "page_printers",
                   PAGE_REMOTE: "page_remote", PAGE_SHARING: "page_sharing",
                   PAGE_GAMES: "page_games",
                   PAGE_ASSISTANT: "page_assistant", PAGE_RECOVERY: "page_recovery",
                   PAGE_CLIPBOARD: "page_clipboard", PAGE_HARDWARE: "page_hardware"}
    # Pages gardant leur cadre d’état, complétées par des réglages en dessous.
    EXTRA_SECTIONS = {8: "sections_display", 2: "sections_network", 7: "sections_recovery", 19: "sections_cpu", 24: "sections_graphics"}

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

    def tool(self, args, refresh=True, after=None):
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
            if after is not None: after()
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
        tool = ambiance_tool()
        moods = ambiance_status() if tool else {}

        def fixed_then(args):
            """Choix manuel : le mode automatique ou le diaporama s'arrête, puis la commande."""
            key = "THEME_AUTO" if args[1] == "theme" else "WALLPAPER"
            value = "off" if key == "THEME_AUTO" else "fixed"
            if tool and moods.get(key.lower()) not in (None, value):
                self.tool(tool + ["set", key, value], refresh=False, after=lambda: self.tool(args))
            else:
                self.tool(args)
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
            button.connect("clicked", lambda *_, p=path: fixed_then(["winlux-personalize", "wallpaper", str(p)]))
            flow.add(button)
        self.body.pack_start(flow, False, False, 0)
        images = Gtk.FileFilter()
        images.set_name(_t("Images"))
        images.add_pixbuf_formats()
        box = self.card()
        self.row(box, _t("Choisir une photo…"), self.browse_button(
            _t("Choisir une photo…"), images, lambda path: fixed_then(["winlux-personalize", "wallpaper", path])))
        modes = [("dark", _t("Sombre")), ("light", _t("Clair"))]
        if tool: modes.append(("auto", _t("Automatique : clair le jour, sombre le soir")))
        mode = "auto" if moods.get("theme_auto") == "on" else prefs.get("theme", "dark")

        def set_mode(value):
            if value == "auto": self.tool(tool + ["set", "THEME_AUTO", "on"])
            else: fixed_then(["winlux-personalize", "theme", value])
        self.combo(box, _t("Mode de couleur"), modes, mode, set_mode,
                   _t("Lever et coucher du soleil calculés pour votre fuseau horaire.") if mode == "auto" else None)
        if tool:
            self.wallpaper_modes(box, tool, moods)
        self.section(_t("Couleur d’accentuation"))
        palette = Gtk.Box(spacing=8)
        for color in ACCENTS:
            swatch = Gtk.Button(label="✓" if prefs.get("accent", "").lower() == color else " ")
            swatch.set_size_request(44, 36)
            provider = Gtk.CssProvider()
            # Coche noire ou blanche selon la teinte (blanc sur turquoise : 2,6:1).
            provider.load_from_data(f"button, button label {{ background: {color}; color: {text_on(color)}; "
                                    f"border-color: {color}; }}".encode())
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
        self.theme_files()
        if have("winlux-widgets"):
            self.widget_settings()
        self.section(_t("Icônes du bureau"))
        box = self.card()
        self.actions(box, [(_t("Corbeille"), lambda: self.execute("desktop-icon-recycle")),
                           (_t("Ce PC"), lambda: self.execute("desktop-icon-computer")),
                           (_t("Explorateur"), lambda: self.execute("desktop-icon-explorer")),
                           (_t("Documents"), lambda: self.execute("desktop-icon-documents"))])
        self.hint(_t("Ajoute l’icône sur le bureau ; un clic droit sur l’icône la retire."), box)

    def wallpaper_modes(self, box, tool, moods):
        """Diaporama ou fond selon l'heure (winlux-ambiance, #281)."""
        mode = moods.get("wallpaper", "fixed")
        reopen = lambda: self.select(self.nav, None)
        self.combo(box, _t("Personnaliser l’arrière-plan"),
                   [("fixed", _t("Image")), ("slideshow", _t("Diaporama")),
                    ("daynight", _t("Winlux Aube le jour, Winlux Nuit le soir"))], mode,
                   lambda value: self.tool(tool + ["set", "WALLPAPER", value], refresh=False, after=reopen))
        if mode != "slideshow": return
        folder = moods.get("slide_folder") or ""
        chooser = Gtk.Button(label=_t("Parcourir…"))

        def pick(*_):
            dialog = Gtk.FileChooserNative.new(_t("Choisir un dossier"), self, Gtk.FileChooserAction.SELECT_FOLDER, None, None)
            if folder: dialog.set_current_folder(folder)
            if dialog.run() == Gtk.ResponseType.ACCEPT and dialog.get_filename():
                self.tool(tool + ["set", "SLIDE_FOLDER", dialog.get_filename()], refresh=False, after=reopen)
            dialog.destroy()
        chooser.connect("clicked", pick)
        self.row(box, _t("Dossier des images"), chooser, folder or _t("Aucun dossier choisi."))
        self.combo(box, _t("Changer d’image toutes les"),
                   [("1", _t("1 minute")), ("10", _t("10 minutes")), ("30", _t("30 minutes")), ("60", _t("1 heure")),
                    ("360", _t("6 heures")), ("1440", _t("1 jour"))], moods.get("slide_minutes", "30"),
                   lambda value: self.tool(tool + ["set", "SLIDE_MINUTES", value], refresh=False))
        self.switch(box, _t("Ordre aléatoire"), moods.get("slide_shuffle") == "on",
                    lambda on: self.tool(tool + ["set", "SLIDE_SHUFFLE", "on" if on else "off"], refresh=False))
        self.actions(box, [(_t("Image suivante"), lambda: self.tool(tool + ["next"], refresh=False), bool(folder))])

    def theme_files(self):
        """Thème complet (fond, couleur, barre, sons) en un fichier .wlxtheme (#281)."""
        self.section(_t("Thèmes"), _t("Enregistrez votre fond, votre couleur d’accentuation, la taille de la barre et les sons en un fichier à partager."))
        box = self.card()
        themes = Gtk.FileFilter()
        themes.set_name(_t("Thèmes Winlux"))
        themes.add_pattern("*.wlxtheme")

        def save():
            dialog = Gtk.FileChooserNative.new(_t("Enregistrer le thème"), self, Gtk.FileChooserAction.SAVE, None, None)
            dialog.add_filter(themes)
            dialog.set_do_overwrite_confirmation(True)
            dialog.set_current_name(_t("Mon thème") + ".wlxtheme")
            if dialog.run() == Gtk.ResponseType.ACCEPT and dialog.get_filename():
                target = dialog.get_filename()
                if not target.endswith(".wlxtheme"): target += ".wlxtheme"
                self.tool(["winlux-personalize", "export", target], refresh=False)
            dialog.destroy()

        def load():
            dialog = Gtk.FileChooserNative.new(_t("Appliquer un thème"), self, Gtk.FileChooserAction.OPEN, None, None)
            dialog.add_filter(themes)
            if dialog.run() == Gtk.ResponseType.ACCEPT and dialog.get_filename():
                chosen = dialog.get_filename()
                tool = ambiance_tool()
                apply = lambda: self.tool(["winlux-personalize", "import", chosen])
                if tool:
                    self.tool(tool + ["set", "THEME_AUTO", "off"], refresh=False,
                              after=lambda: self.tool(tool + ["set", "WALLPAPER", "fixed"], refresh=False, after=apply))
                else:
                    apply()
            dialog.destroy()
        self.actions(box, [(_t("Enregistrer le thème…"), save), (_t("Appliquer un thème…"), load)])

    def widget_settings(self):
        """Volet Win+W (winlux-widgets) : bouton de la barre, météo, photos."""
        try:
            answer = run(["winlux-widgets", "status"])
            prefs = json.loads(answer.stdout if answer else "{}")
        except ValueError:
            prefs = {}
        self.section(_t("Widgets"), _t("Windows + W ouvre le volet : météo, agenda, état du PC, pense-bête, photos et minuteur."))
        box = self.card()
        self.switch(box, _t("Bouton Widgets dans la barre des tâches"), prefs.get("button"),
                    lambda on: self.tool(["winlux-widgets", "set", "button", "on" if on else "off"], refresh=False),
                    _t("Affiche la météo ; un clic ouvre les widgets."))
        place = Gtk.Box(spacing=6)
        entry = Gtk.Entry(text=prefs.get("location", ""), placeholder_text=_t("Automatique"), width_chars=20)
        entry.get_accessible().set_name(_t("Lieu de la météo"))
        apply = Gtk.Button(label=_t("Appliquer"))
        save = lambda *_: self.tool(["winlux-widgets", "set", "location", entry.get_text().strip()], refresh=False)
        entry.connect("activate", save)
        apply.connect("clicked", save)
        place.pack_start(entry, False, False, 0)
        place.pack_start(apply, False, False, 0)
        self.row(box, _t("Lieu de la météo"), place, _t("Vide : d’après la connexion Internet. Données de wttr.in."))
        self.combo(box, _t("Unité de température"), [("c", _t("Celsius (°C)")), ("f", _t("Fahrenheit (°F)"))],
                   prefs.get("units", "c"), lambda value: self.tool(["winlux-widgets", "set", "units", value], refresh=False))
        folder = Gtk.Button(label=_t("Choisir un dossier…"))

        def pick(*_):
            dialog = Gtk.FileChooserNative.new(_t("Dossier des photos"), self, Gtk.FileChooserAction.SELECT_FOLDER, None, None)
            if dialog.run() == Gtk.ResponseType.ACCEPT and dialog.get_filename():
                self.tool(["winlux-widgets", "set", "photos", dialog.get_filename()])
            dialog.destroy()
        folder.connect("clicked", pick)
        self.row(box, _t("Dossier des photos"), folder, prefs.get("photos") or _t("Images du compte"))
        self.actions(box, [(_t("Ouvrir les widgets"), lambda: self.spawn(["winlux-widgets"])),
                           (_t("Rétablir les widgets par défaut"), lambda: self.tool(["winlux-widgets", "reset"], refresh=False))])

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
        mode = appearance().get("theme", "dark")
        if theme is None and have("winlux-personalize"):
            # Retour au thème Winlux du compte (couleurs et accent compris).
            run(["winlux-personalize", "theme", mode if mode in ("dark", "light") else "dark"], timeout=30)
            return
        if theme is None: theme = "Adwaita-dark" if mode == "dark" else "Adwaita"
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
        self.combo(box, _t("Priorité du processeur"), [(k, _t(v)) for k, v in EXE_PRIORITIES], profile.get("cpu_priority", "auto"),
                   lambda value: self.save_program(exe, cpu_priority=None if value == "auto" else value))
        if keyvals(read(RUN / "cpu.txt")).get("cap_hybrid") == "1":
            self.combo(box, _t("Cœurs utilisés"), [(k, _t(v)) for k, v in EXE_CORES], profile.get("cpu_cores", "auto"),
                       lambda value: self.save_program(exe, cpu_cores=None if value == "auto" else value))
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

    # ── Clavier et souris (le pavé tactile n’apparaît que s’il existe) ──

    def page_input(self):
        status = self.status
        self.section(_t("Clavier"))
        box = self.card()
        self.combo(box, _t("Disposition du clavier"), [(k, _t(v)) for k, v in LAYOUTS], status.get("layout") or "fr",
                   lambda value: self.execute("keyboard-" + value), _t("Langue et fuseau horaire : Heure et langue."))
        self.section(_t("Souris"))
        box = self.card()
        speeds = [("25", _t("Lente")), ("50", _t("Normale")), ("75", _t("Rapide"))]
        self.combo(box, _t("Vitesse du pointeur"), speeds, status.get("mouse_speed") if status.get("mouse_speed") in ("25", "75") else "50",
                   lambda value: self.execute({"25": "mouse-slow", "75": "mouse-fast"}.get(value, "mouse-normal")))
        if status.get("touchpad_present") != "1": return
        self.section(_t("Pavé tactile"))
        box = self.card()
        for key, title, subtitle in TOUCHPAD:
            value = status.get("touchpad_" + key, "")
            if not value: continue
            self.switch(box, _t(title), value == "1",
                        lambda on, k=key: self.execute("touchpad-set", k, "1" if on else "0"), _t(subtitle))
        speed = status.get("touchpad_speed", "")
        if speed.isdigit():
            self.level_row(box, _t("Vitesse du curseur"), int(speed), lambda v: self.execute("touchpad-set", "speed", str(v)))
        if status.get("touchpad_gestures") == "1":
            self.row(box, _t("Pincer pour zoomer"), None, _t("Les applications zooment comme avec Ctrl + molette."))

    def level_row(self, parent, title, value, changed):
        """Curseur 0-100 appliqué quand on le relâche (une commande, pas cent)."""
        scale = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 100, 10)
        scale.set_value(min(100, max(0, value)))
        scale.set_size_request(260, -1)
        scale.connect("button-release-event", lambda w, _: changed(round(w.get_value())) and False)
        self.row(parent, title, scale)

    # ── Processeur : sections que le matériel propose vraiment ──

    def sections_cpu(self):
        report = keyvals(read(RUN / "cpu.txt"))
        if report.get("cpu_model") or report.get("cpu_count"):
            box = self.card()
            detail = _t("{count} processeurs logiques").format(count=report.get("cpu_count", "?"))
            if report.get("freq_max"): detail += " · " + _t("jusqu’à {mhz} MHz").format(mhz=report["freq_max"])
            self.row(box, report.get("cpu_model") or _t("Processeur"), None, detail)
        profiles = [p for p in report.get("profiles", "").split(",") if p in CPU_PROFILES]
        if report.get("cap_governor") == "1" and profiles:
            self.section(_t("Mode du processeur"), _t("Performances : vitesse maximale. Économie : moins de chaleur et de bruit."))
            box = self.card()
            self.combo(box, _t("Mode"), [(p, _t(CPU_PROFILES[p])) for p in profiles], report.get("profile"),
                       lambda value: self.execute("cpu-profile-" + value))
        if report.get("cap_hybrid") == "1":
            self.section(_t("Cœurs performants et cœurs économes"))
            box = self.card()
            self.combo(box, _t("Placement des programmes Windows"), [(k, _t(v)) for k, v in CPU_HYBRID],
                       report.get("hybrid_policy") or "auto", lambda value: self.execute("cpu-hybrid-" + value),
                       _t("P-cores : {p} · E-cores : {e}").format(p=report.get("pcores", "?"), e=report.get("ecores", "?")))
        if report.get("cap_fan_control") == "1":
            self.section(_t("Ventilateurs"))
            box = self.card()
            for entry in [f for f in report.get("fans", "").split(",") if ":" in f][:8]:
                label, rpm = entry.split(":", 1)
                self.row(box, label, None, _t("{rpm} tr/min").format(rpm=rpm))
            self.combo(box, _t("Courbe des ventilateurs"), [(k, _t(v)) for k, v in CPU_FANS],
                       report.get("fan_curve") if report.get("fan_curve") in dict(CPU_FANS) else "auto",
                       lambda value: self.execute("cpu-fan-" + value))
        hardware = hardware_page()
        if hardware is not None: hardware.cpu_section(self)
        self.hint(_t("Priorité et cœurs d’un programme précis : Applications › Programmes Windows."))

    # ── Presse-papiers (Windows + V) ──

    def page_clipboard(self):
        tool = clipboard_tool()
        result = run(tool + ["status"])
        state = keyvals(result.stdout) if result else {}
        box = self.card()
        self.switch(box, _t("Historique du presse-papiers"), state.get("history", "on") == "on",
                    lambda on: self.tool(tool + ["set", "HISTORY", "on" if on else "off"]),
                    _t("Gardez plusieurs éléments copiés et retrouvez-les avec Windows + V."))
        box = self.card()
        self.row(box, _t("Effacer les données du presse-papiers"), None,
                 _t("Efface tout l’historique, sauf les éléments épinglés."))
        self.actions(box, [(_t("Effacer"), lambda: self.tool(tool + ["clear"]),
                            state.get("items", "0") != state.get("pinned", "0"))])
        ocr = ocr_tool()
        if ocr is None: return
        result = run(ocr + ["status"])
        state = keyvals(result.stdout) if result else {}
        self.section(_t("Copier le texte d’une image"),
                     _t("Windows + Maj + T encadre une zone de l’écran et copie son texte ; la visionneuse d’images et le clic droit sur une image le proposent aussi."))
        box = self.card()
        if state.get("installed") == "1":
            names = {"fra": _t("français"), "eng": _t("anglais"), "spa": _t("espagnol"), "deu": _t("allemand")}
            languages = ", ".join(names[code] for code in state.get("languages", "").split(",") if code in names)
            self.row(box, _t("Reconnaissance de texte installée"), None, languages or None)
            self.actions(box, [(_t("Retirer"), lambda: self.tool(ocr + ["remove"]))])
        else:
            self.row(box, _t("Reconnaissance de texte"), None,
                     _t("Téléchargée à la première utilisation (environ 15 Mo), en français, anglais, espagnol et allemand."))
            self.actions(box, [(_t("Télécharger"), lambda: self.tool(ocr + ["install"]))])

    # ── Écran : éclairage nocturne ──

    def sections_display(self):
        state = ambiance_status()
        # Pilote sans rampe gamma (certaines VM) : section masquée.
        if state.get("supported") != "1": return
        tool = ambiance_tool()
        self.section(_t("Éclairage nocturne"), _t("Des couleurs plus chaudes le soir reposent les yeux et aident à s’endormir."))
        box = self.card()
        self.switch(box, _t("Activer maintenant"), state.get("nightlight") == "on",
                    lambda on: self.tool(tool + ["set", "NIGHTLIGHT", "on" if on else "off"], refresh=False))
        try: strength = int(state.get("strength", "50"))
        except ValueError: strength = 50
        scale = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 100, 5)
        scale.set_value(strength)
        scale.set_size_request(240, -1)
        scale.set_draw_value(False)
        pending = {}

        def moved(widget):
            # Valeur envoyée quand le curseur s’arrête, pas à chaque pixel.
            if pending.get("id"): GLib.source_remove(pending["id"])
            def send():
                pending["id"] = None
                self.tool(tool + ["set", "STRENGTH", str(int(widget.get_value()))], refresh=False)
                return False
            pending["id"] = GLib.timeout_add(250, send)
        scale.connect("value-changed", moved)
        self.row(box, _t("Intensité"), scale)
        sunset, sunrise = state.get("sunset", "20:00"), state.get("sunrise", "07:00")
        schedule = state.get("schedule") if state.get("schedule") in ("off", "sun", "manual") else "off"
        self.combo(box, _t("Planifier l’éclairage nocturne"),
                   [("off", _t("Jamais")),
                    ("sun", _t("Du coucher au lever du soleil ({sunset} – {sunrise})").format(sunset=sunset, sunrise=sunrise)),
                    ("manual", _t("Définir les heures"))], schedule,
                   lambda value: self.tool(tool + ["set", "SCHEDULE", value], refresh=False,
                                           after=lambda: self.select(self.nav, None)))
        if schedule == "sun":
            self.hint(_t("Heures calculées pour votre fuseau horaire, sans connexion à Internet.")
                      if state.get("sun_known") == "1" else _t("Fuseau horaire sans coordonnées : 20:00 – 07:00 par défaut."), box)
        if schedule == "manual":
            for title, key, value in ((_t("Activer à"), "FROM", state.get("from", "21:00")),
                                      (_t("Désactiver à"), "TO", state.get("to", "07:00"))):
                entry = Gtk.Entry(text=value, width_chars=6, max_length=5)
                entry.connect("activate", lambda widget, k=key: self.tool(tool + ["set", k, widget.get_text()], refresh=False))
                self.row(box, title, entry)

    # ── Jeux : bibliothèque, temps de jeu, dossiers de jeux ──

    def page_games(self):
        games = games_module()
        if games is None: return
        config = games.load_config()

        def change(edit):
            games.update_config(edit)
            GLib.idle_add(self.extra_refresh)

        def setter(key, on):
            def edit(c): c[key] = "1" if on else "0"
            change(edit)
        self.section(_t("Bibliothèque de jeux"), _t("L’application Jeux rassemble vos jeux Windows installés : GOG, itch.io, dossiers de jeux et programmes 3D."))
        box = self.card()
        self.actions(box, [(_t("Ouvrir Jeux"), lambda: subprocess.Popen(
            ["/usr/bin/python3", str(games.APP_PATH)], start_new_session=True,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))])
        self.switch(box, _t("Rechercher les nouveaux jeux à l’ouverture"), config["AUTOSCAN"] == "1",
                    lambda on: setter("AUTOSCAN", on))
        self.switch(box, _t("Compter le temps de jeu"), config["PLAYTIME"] == "1",
                    lambda on: setter("PLAYTIME", on), _t("Enregistré sur ce PC uniquement, pour chaque jeu lancé."))

        def forget():
            if self.confirm(_t("Effacer le temps de jeu de tous les jeux ?")):
                games.forget_playtime()
        self.actions(box, [(_t("Effacer le temps de jeu"), forget, games.data_dir().is_dir())])
        self.section(_t("Dossiers de jeux"), _t("Toujours parcourus : C:\\Games, C:\\Jeux, C:\\GOG Games, Program Files et les jeux itch.io. Ajoutez ici un disque ou un dossier où vous rangez d’autres jeux."))
        box = self.card()
        if not config["FOLDER"]: self.hint(_t("Aucun dossier ajouté."), box)
        for folder in config["FOLDER"]:
            remove = Gtk.Button(label=_t("Retirer"))
            remove.connect("clicked", lambda *_, f=folder: change(lambda c: c["FOLDER"].remove(f) if f in c["FOLDER"] else None))
            self.row(box, folder, remove)

        def pick():
            dialog = Gtk.FileChooserNative.new(_t("Choisir un dossier"), self, Gtk.FileChooserAction.SELECT_FOLDER, None, None)
            if dialog.run() == Gtk.ResponseType.ACCEPT and dialog.get_filename():
                chosen = dialog.get_filename()
                change(lambda c: chosen in c["FOLDER"] or c["FOLDER"].append(chosen))
            dialog.destroy()
        self.actions(box, [(_t("Ajouter un dossier…"), pick)])
        if config["HIDDEN"]:
            self.section(_t("Jeux retirés de la bibliothèque"))
            box = self.card()
            for exe in config["HIDDEN"][:50]:
                restore = Gtk.Button(label=_t("Remettre"))
                restore.connect("clicked", lambda *_, e=exe: change(lambda c: e in c["HIDDEN"] and c["HIDDEN"].remove(e)))
                self.row(box, Path(exe).name, restore, exe)
        self.section(_t("Performances en jeu"), _t("Le mode jeu, le choix de la carte graphique et l’overlay de performances s’appliquent tout seuls à chaque jeu."))
        box = self.card()
        self.actions(box, [(_t("Mode jeu (Processeur)"), lambda: GLib.idle_add(self.open_page, 19)),
                           (_t("Overlay et carte graphique (Graphismes)"), lambda: GLib.idle_add(self.open_page, 24))])

    # ── Graphismes : overlay de performances en jeu ──

    def sections_graphics(self):
        status = self.status
        # Pilote de la carte, NVIDIA propriétaire, VA-API/NVENC (#matériel réel).
        hardware = hardware_page()
        if hardware is not None: hardware.gpu_section(self)
        self.section(_t("Overlay de performances en jeu"))
        box = self.card()
        enabled = status.get("overlay_enabled") == "1"
        self.switch(box, _t("Afficher l’overlay dans les jeux"), enabled,
                    lambda on: self.execute("overlay-on" if on else "overlay-off"))
        if not enabled: return
        self.combo(box, _t("Raccourci clavier"), [(k, _t(v)) for k, v in OVERLAY_KEYS], status.get("overlay_hotkey") or "shift-f12",
                   lambda value: self.execute("overlay-key-" + value))
        self.combo(box, _t("Position"), [(k, _t(v)) for k, v in OVERLAY_POSITIONS], status.get("overlay_position") or "top-left",
                   lambda value: self.execute("overlay-pos-" + value))
        self.switch(box, _t("Afficher au lancement d’un jeu"), status.get("overlay_autoshow") == "1",
                    lambda on: self.execute("overlay-auto-on" if on else "overlay-auto-off"))

    # ── Réseau : Wi-Fi, seulement avec une carte Wi-Fi ──

    def sections_network(self):
        if not have("nmcli"): return
        devices = run(["nmcli", "-t", "-f", "DEVICE,TYPE,STATE", "device"], timeout=3)
        if not devices or ":wifi:" not in devices.stdout: return
        self.section(_t("Wi-Fi"))
        box = self.card()
        radio = run(["nmcli", "radio", "wifi"], timeout=3)
        self.switch(box, _t("Wi-Fi"), bool(radio) and radio.stdout.strip() == "enabled",
                    lambda on: self.tool(["nmcli", "radio", "wifi", "on" if on else "off"]))
        found = run(["nmcli", "-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list"], timeout=8)
        networks = []
        for line in (found.stdout.splitlines() if found else []):
            parts = re.split(r"(?<!\\):", line)
            if len(parts) == 4 and parts[1] and parts[1] not in [n[1] for n in networks]:
                networks.append((parts[0] == "*", parts[1].replace("\\:", ":"), parts[2], parts[3]))
        if not networks: self.hint(_t("Aucun réseau Wi-Fi visible pour l’instant."), box)
        for in_use, ssid, signal, security in networks[:12]:
            detail = f"{signal} % · " + (security or _t("Ouvert"))
            button = Gtk.Button(label=_t("Connecté") if in_use else _t("Se connecter"))
            button.set_sensitive(not in_use)
            button.connect("clicked", lambda *_, name=ssid: self.tool(["nmcli", "--ask", "--wait", "90", "device", "wifi", "connect", name]))
            self.row(box, ssid, button, detail)
        self.actions(box, [(_t("Actualiser"), lambda: self.tool(["nmcli", "device", "wifi", "rescan"]))])

    # ── Mises à jour : secours et restauration, dans la page ──

    def sections_recovery(self):
        self.section(_t("Secours et restauration"))
        box = self.card()
        if read("/usr/share/winlux/update-backend").strip() == "apt":
            saved = Path("/var/cache/winlux/previous.deb").is_file()
        else:
            name = read("/var/lib/winlux/rollback-current").strip()
            saved = bool(re.fullmatch(r"update-backup\.[A-Za-z0-9._+-]+", name)) and (Path("/var/lib/winlux") / name / "restore.list").is_file()
        self.row(box, _t("Version précédente"), None,
                 _t("Conservée : le bouton Restaurer ci-dessus y revient.") if saved else _t("Aucune version précédente conservée."))
        self.row(box, _t("Si le bureau ne démarre plus"), None,
                 _t("Choisissez « Winlux secours » au démarrage, ou démarrez sur la clé USB d’installation. Gardez une copie de vos documents sur un autre support."))
        if page_available(PAGE_RECOVERY):
            self.actions(box, [(_t("Réinitialiser ce PC et clé de secours"), lambda: self.open_page(PAGE_RECOVERY))])

    # ── Récupération : réinitialiser ce PC, clé de secours (#283) ──

    def page_recovery(self):
        module = reset_page()
        if module is not None:
            self.recovery_panel = module.RecoveryPanel(self)

    # ── Matériel et pilotes : diagnostic et pilotes recommandés ──

    def page_hardware(self):
        module = hardware_page()
        if module is not None: module.page(self)

    # ── Signaler un problème ──

    def page_report(self):
        status = self.status
        self.hint(_t("Préparez un rapport (journal, matériel, programme en cause) pour les développeurs de Winlux."))
        box = self.card()
        self.actions(box, [(_t("Signaler un problème"), lambda: self.spawn(["winlux-report"])),
                           (_t("Dernier plantage"), lambda: self.spawn(["winlux-report", "--crash", "--force"])),
                           (_t("Rapports enregistrés"), lambda: self.spawn(["winlux-report", "--open-saved"]))])
        self.section(_t("Préférences"))
        box = self.card()
        self.switch(box, _t("Proposer un rapport après un plantage"), status.get("report_offer") != "off",
                    lambda on: self.execute("report-offer-on" if on else "report-offer-off"))
        self.switch(box, _t("Masquer les informations personnelles"), status.get("report_redact") != "off",
                    lambda on: self.execute("report-redact-on" if on else "report-redact-off"),
                    _t("Nom d’utilisateur, adresses et chemins personnels sont remplacés dans le rapport."))
        mode = status.get("report_mode") if status.get("report_mode") in ("browser", "file") else "auto"
        self.combo(box, _t("Envoi du rapport"), [(k, _t(v)) for k, v in REPORT_MODES], mode,
                   lambda value: self.execute("report-mode-" + value))

    # ── Imprimantes et scanners (CUPS) ──

    def page_printers(self):
        default = run(["lpstat", "-d"], timeout=3)
        default = default.stdout.split(":", 1)[-1].strip() if default and ":" in default.stdout else ""
        listed = run(["lpstat", "-e"], timeout=3)
        printers = [name for name in (listed.stdout.split() if listed else [])]
        self.section(_t("Imprimantes installées"))
        box = self.card()
        if not printers: self.hint(_t("Aucune imprimante pour l’instant."), box)
        for name in printers:
            line = Gtk.Box(spacing=6)
            for title, args, show in ((_t("Par défaut"), ["lpoptions", "-d", name], name != default),
                                      (_t("Page de test"), ["lp", "-d", name, "/usr/share/cups/data/testprint"],
                                       Path("/usr/share/cups/data/testprint").is_file()),
                                      (_t("Annuler les impressions"), ["cancel", "-a", name], True),
                                      (_t("Supprimer"), ["lpadmin", "-x", name], True)):
                if not show: continue
                button = Gtk.Button(label=title)
                button.connect("clicked", lambda *_, a=args: self.tool(a))
                line.pack_start(button, False, False, 0)
            self.row(box, name, line, _t("Imprimante par défaut") if name == default else None)
        if not have("lpinfo"): return
        self.section(_t("Ajouter une imprimante"), _t("Imprimantes du réseau et USB trouvées par CUPS (pilote sans installation)."))
        box = self.card()
        self.printer_list = Gtk.ComboBoxText()
        self.row(box, _t("Appareil"), self.printer_list)
        self.actions(box, [(_t("Rechercher"), self.find_printers),
                           (_t("Ajouter"), self.add_printer)])

    def find_printers(self):
        self.printer_list.remove_all()
        found = run(["lpinfo", "-v"], timeout=30)
        for line in (found.stdout.splitlines() if found else []):
            uri = line.split(None, 1)[-1]
            if "://" in uri and not uri.startswith(("file:", "beh:", "cups-pdf:")):
                self.printer_list.append(uri, uri)
        if self.printer_list.get_model().iter_n_children(None): self.printer_list.set_active(0)
        else: self.show_error(_t("Ajouter une imprimante"), _t("Aucune imprimante trouvée. Vérifiez qu’elle est allumée et branchée."))

    def add_printer(self):
        uri = self.printer_list.get_active_id()
        if not uri: return
        name = re.sub(r"[^A-Za-z0-9_-]+", "_", uri.split("://", 1)[-1])[:40].strip("_") or "Imprimante"
        self.tool(["lpadmin", "-p", name, "-E", "-v", uri, "-m", "everywhere"])

    # ── Bureau à distance ──

    def page_remote(self):
        if have("winlux-remote-config"): run(["winlux-remote-config", "status"], timeout=5)
        info = keyvals(read(RUN / "remote.txt"))
        enabled = info.get("enabled") == "1"
        box = self.card()
        self.switch(box, _t("Bureau à distance"), enabled, lambda on: self.execute("remote-enable" if on else "remote-disable"),
                    _t("Permet de prendre la main sur ce PC depuis un autre ordinateur du réseau."))
        if not enabled: return
        clients = info.get("clients", "0")
        state = _t("Service arrêté") if info.get("running") != "1" else (
            _t("En attente de connexion") if clients in ("", "0") else _t("{count} connexion(s) en cours").format(count=clients))
        address = f"{info.get('address') or _t('ce PC')}:{info.get('port', '5900')}"
        self.row(box, _t("Adresse"), None, address + " · " + state)
        if info.get("fingerprint"): self.row(box, _t("Empreinte"), None, info["fingerprint"])
        self.switch(box, _t("Affichage seul (sans souris ni clavier)"), info.get("view_only") == "1",
                    lambda on: self.execute("remote-view-only" if on else "remote-full-control"))
        self.switch(box, _t("Compatibilité VNC classique"), info.get("legacy") == "1",
                    lambda on: self.execute("remote-legacy-on" if on else "remote-legacy-off"),
                    _t("Mot de passe : {password}").format(password=info["legacy_password"]) if info.get("legacy_password") else None)
        self.actions(box, [("Port −", lambda: self.execute("remote-port-down")), ("Port 5900", lambda: self.execute("remote-port-default")),
                           ("Port +", lambda: self.execute("remote-port-up"))])
        accounts = [line.split("=", 1)[1].split("|")[0] for line in read(RUN / "remote.txt").splitlines()
                    if line.startswith("account=") and line.endswith("|1")]
        if accounts:
            self.section(_t("Comptes autorisés"))
            box = self.card()
            allowed = info.get("allowed", "").split(",")
            for name in accounts:
                self.switch(box, name, name in allowed, lambda on, n=name: self.execute("remote-allow" if on else "remote-deny", n))

    # ── Assistant Claude (#286) : désactivé par défaut ──

    def page_assistant(self):
        import sys, threading
        module = assistant_module()
        enabled = module.is_enabled()
        module.protect_credentials()
        box = self.card()
        self.switch(box, _t("Assistant Claude dans la barre des tâches"), enabled, self.assistant_toggle,
                    _t("Un bouton Claude dans la barre des tâches, et Windows+C, pour poser une question sur Winlux : pourquoi un programme .exe ne s’ouvre pas, où est un fichier, comment changer un réglage."))
        if not enabled:
            self.hint(_t("À l’activation, Claude Code est téléchargé s’il manque (environ 300 Mo), puis vous vous connectez à votre compte Claude (Google, e-mail…)."), box)
        else:
            account = Gtk.Label(label=_t("Vérification de la connexion…"), xalign=0)
            account.get_style_context().add_class("subtitle")
            self.row(box, _t("Compte Claude"), account)
            buttons = self.actions(box, [(_t("Ouvrir l’assistant"), lambda: self.spawn([sys.executable, module.__file__])),
                                         (_t("Se déconnecter"), lambda: self.tool([sys.executable, module.__file__, "--deconnecter"]))])

            def checked(status):
                if not status.get("installed"): text = _t("Claude Code n’est pas installé")
                elif status.get("loggedIn"):
                    who = module.account_name(status)
                    text = _t("Connecté : {account}").format(account=who) if who else _t("Connecté")
                else: text = _t("Non connecté")
                if account.get_toplevel() is not None: account.set_text(text)
                if not status.get("loggedIn"):
                    for child in buttons.get_children(): child.destroy()
                    login = Gtk.Button(label=_t("Se connecter"))
                    login.connect("clicked", lambda *_: self.assistant_toggle(True))
                    buttons.pack_start(login, False, False, 0)
                    login.show()
                return False
            threading.Thread(target=lambda: GLib.idle_add(checked, module.auth_status()), daemon=True).start()
        self.section(_t("Ce qui est envoyé"))
        box = self.card()
        for text in ("Seuls votre question et la conversation en cours partent vers Claude (Anthropic). Rien n’est gardé sur ce PC une fois la fenêtre fermée.",
                     "Claude ne lit rien et ne change rien tout seul : chaque lecture (journal d’un programme, recherche de fichier) vous montre le texte exact avant l’envoi, et chaque action attend votre clic.",
                     "La connexion à votre compte est gardée par Claude Code dans votre dossier personnel, lisible par vous seul.",
                     "Désactivé, l’assistant n’apparaît nulle part et rien ne tourne."):
            self.hint(_t(text), box)

    def assistant_toggle(self, on):
        """Activer ouvre la fenêtre de configuration (téléchargement, connexion) ;
        l’assistant n’est activé qu’une fois tout prêt. Désactiver est immédiat."""
        import sys
        module = assistant_module()
        if not on:
            self.tool([sys.executable, module.__file__, "--desactiver"])
            return
        if getattr(self, "assistant_setup", None) is not None and self.assistant_setup.poll() is None: return
        try:
            self.assistant_setup = subprocess.Popen([sys.executable, module.__file__, "--configurer"], start_new_session=True)
        except OSError as error:
            self.show_error(_t("L’opération a échoué"), str(error))
            return

        def wait():
            if self.assistant_setup.poll() is None: return True
            self.extra_refresh()
            return False
        GLib.timeout_add(500, wait)

    # ── Partage et transfert ──

    def page_sharing(self):
        if have("winlux-control"): run(["winlux-control", "share-status"], timeout=5)
        shares = read(RUN / "shares.txt")
        info = keyvals(shares)
        if info.get("available") == "1":
            self.section(_t("Dossiers partagés"), _t("Visibles des autres PC (Windows, Mac, Linux) du réseau privé."))
            box = self.card()
            self.switch(box, _t("Partage de fichiers"), info.get("enabled") == "1",
                        lambda on: self.execute("share-enable" if on else "share-disable"))
            for line in shares.splitlines():
                if not line.startswith("share="): continue
                parts = line[6:].split("|")
                if len(parts) < 3: continue
                remove = Gtk.Button(label=_t("Arrêter le partage"))
                remove.connect("clicked", lambda *_, i=parts[0]: self.execute("share-remove", i))
                self.row(box, parts[1], remove, f"\\\\{info.get('host', '')}\\{parts[1]} · {parts[2]}")

            def pick():
                folders = Gtk.FileChooserNative.new(_t("Choisir un dossier"), self, Gtk.FileChooserAction.SELECT_FOLDER, None, None)
                if folders.run() == Gtk.ResponseType.ACCEPT and folders.get_filename():
                    self.execute("share-add", folders.get_filename())
                folders.destroy()
            self.actions(box, [(_t("Partager un dossier…"), pick)])
            self.hint(_t("Droits par compte : clic droit sur le dossier › « Partager ce dossier… »."), box)
        if not have("winlux-transfer") and not (RUN / "transfer.txt").is_file(): return
        if have("winlux-control"): run(["winlux-control", "transfer-status"], timeout=5)
        transfer = read(RUN / "transfer.txt")
        self.section(_t("Transférer mes réglages"), _t("Un fichier .wlxprofile emporte vos réglages vers un autre PC Winlux."))
        box = self.card()
        targets = [line[7:] for line in transfer.splitlines() if line.startswith("target=")]
        if targets:
            where = Gtk.ComboBoxText()
            for index, target in enumerate(targets): where.append(str(index), target.split("|", 1)[-1])
            where.set_active(0)
            self.row(box, _t("Exporter vers"), where)
            self.actions(box, [(_t("Exporter mes réglages"), lambda: self.execute("transfer-export", where.get_active_id() or "0"))])
        found = [line[6:].split("|") for line in transfer.splitlines() if line.startswith("found=")]
        for index, parts in enumerate(found):
            button = Gtk.Button(label=_t("Importer"))
            button.connect("clicked", lambda *_, i=index: self.confirm(_t("Importer ces réglages ?")) and self.execute("transfer-import", str(i)))
            self.row(box, parts[0], button, " · ".join(p for p in parts[1:] if p))
        if keyvals(transfer).get("backup") == "1":
            self.actions(box, [(_t("Annuler le dernier import"), lambda: self.execute("transfer-undo"))])
        if not targets and not found and keyvals(transfer).get("backup") != "1":
            self.hint(_t("Aucun emplacement d’export pour l’instant : branchez une clé USB ou ouvrez Documents."), box)

    # ── Applications : installer, par défaut, démarrage (dans la page) ──

    def page_apps(self):
        panel = default_apps_panel(self)
        if panel is not None:
            self.section(_t("Applications par défaut"))
            self.body.pack_start(panel, False, False, 0)
        if have("flatpak") or Path("/usr/share/winlux/apt-base").is_file():
            self.section(_t("Installer des applications Linux"), _t("Les programmes Windows s’installent en ouvrant leur installateur .exe."))
            box = self.card()
            if Path("/usr/share/winlux/apt-base").is_file():
                package = Gtk.Entry(placeholder_text=_t("Nom du paquet Ubuntu, par exemple vlc"))
                self.row(box, _t("Paquet Linux (APT)"), package)
                self.actions(box, [(_t("Installer"), lambda: self.package("apt", "install", package.get_text())),
                                   (_t("Désinstaller"), lambda: self.package("apt", "remove", package.get_text()))])
            if have("flatpak"):
                ident = Gtk.Entry(placeholder_text=_t("Identifiant Flatpak, par exemple org.videolan.VLC"))
                self.row(box, "Flatpak", ident)
                self.actions(box, [(_t("Installer"), lambda: self.package("flatpak", "install", ident.get_text())),
                                   (_t("Désinstaller"), lambda: self.package("flatpak", "uninstall", ident.get_text())),
                                   (_t("Tout mettre à jour"), lambda: self.tool(["flatpak", "update", "--user", "--noninteractive"]))])
        self.section(_t("Démarrage"), _t("Programmes lancés à l’ouverture de session."))
        box = self.card()
        autostart = HOME / ".config/autostart"
        entries = sorted(autostart.glob("*.desktop")) if autostart.is_dir() else []
        shown = 0
        for path in entries:
            key = GLib.KeyFile()
            try: key.load_from_file(str(path), GLib.KeyFileFlags.NONE)
            except GLib.Error: continue
            try: name = key.get_locale_string("Desktop Entry", "Name", None)
            except GLib.Error: name = path.stem
            try: hidden = key.get_boolean("Desktop Entry", "Hidden")
            except GLib.Error: hidden = False
            self.switch(box, name, not hidden, lambda on, p=path: self.set_autostart(p, on))
            shown += 1
        if not shown: self.hint(_t("Aucun programme ne démarre avec la session."), box)
        apps = sorted((app for app in Gio.AppInfo.get_all() if app.should_show() and app.get_id() and app.get_filename()),
                      key=lambda app: app.get_display_name().casefold())
        if apps:
            choice = Gtk.ComboBoxText()
            for app in apps: choice.append(app.get_id(), app.get_display_name())
            choice.set_active(0)
            line = Gtk.Box(spacing=8)
            line.pack_start(choice, False, False, 0)
            add = Gtk.Button(label=_t("Ajouter"))
            add.connect("clicked", lambda *_: self.add_autostart({a.get_id(): a for a in apps}.get(choice.get_active_id())))
            line.pack_start(add, False, False, 0)
            self.row(box, _t("Lancer aussi au démarrage"), line)

    def package(self, kind, action, name):
        name = name.strip()
        if kind == "apt" and not re.fullmatch(r"[a-z0-9][a-z0-9+.-]+", name) or \
           kind == "flatpak" and not re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]*(?:\.[A-Za-z0-9_-]+){2,}", name):
            self.show_error(_t("L’opération a échoué"), _t("Nom invalide : {name}").format(name=name or "—"))
            return
        if not self.confirm(_t("Installer {name} ?" if action == "install" else "Désinstaller {name} ?").format(name=name)): return
        if kind == "apt":
            self.tool(["env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "-y", "-o", "Dpkg::Options::=--force-confold", action, name])
        else:
            self.tool(["flatpak", action, "--user", "--noninteractive"] + (["flathub", name] if action == "install" else [name]))

    def add_autostart(self, app):
        if app is None: return
        target = HOME / ".config/autostart" / Path(app.get_filename()).name
        try:
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(app.get_filename(), target)
        except OSError as error:
            self.show_error(_t("L’opération a échoué"), str(error))
            return
        self.set_autostart(target, True)
        self.extra_refresh()

    def set_autostart(self, path, enabled):
        key = GLib.KeyFile()
        try:
            key.load_from_file(str(path), GLib.KeyFileFlags.KEEP_TRANSLATIONS)
            key.set_boolean("Desktop Entry", "Hidden", not enabled)
            path.write_text(key.to_data()[0])
        except (GLib.Error, OSError) as error:
            self.show_error(_t("L’opération a échoué"), str(error))

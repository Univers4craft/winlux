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
import time
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, Gdk, Gio, GLib, GdkPixbuf


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
    'Réseaux Wi-Fi…': ('Wi-Fi networks…', 'Redes Wi-Fi…', 'WLAN-Netzwerke…'),
    'Paramètres réseau…': ('Network settings…', 'Configuración de red…', 'Netzwerkeinstellungen…'),
    'Bluetooth et appareils…': ('Bluetooth & devices…', 'Bluetooth y dispositivos…', 'Bluetooth und Geräte…'),
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
    'Pour restaurer, ouvrez Mise à jour OS et choisissez Restaurer. Une confirmation est demandée avant toute modification.': ("To restore, open OS update and choose Restore. You'll be asked to confirm before anything changes.", 'Para restaurar, abra Actualización del SO y elija Restaurar. Se pide confirmación antes de cualquier cambio.', 'Öffnen Sie zum Wiederherstellen OS-Update und wählen Sie Wiederherstellen. Vor jeder Änderung wird eine Bestätigung angefordert.'),
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
    'Éclairage nocturne : activer': ('Night light: turn on', 'Luz nocturna: activar', 'Nachtmodus: einschalten'),
    'Éclairage nocturne : désactiver': ('Night light: turn off', 'Luz nocturna: desactivar', 'Nachtmodus: ausschalten'),
}


# Textes de la refonte des outils (cartes, tuiles, lignes façon Paramètres).
_TR.update({
    'Accessibilité': ('Accessibility', 'Accesibilidad', 'Barrierefreiheit'),
    'Aide supplémentaire': ('More help', 'Más ayuda', 'Weitere Hilfe'),
    'Ajouter un dossier': ('Add a folder', 'Agregar una carpeta', 'Ordner hinzufügen'),
    'Alimentation et batterie': ('Power & battery', 'Energía y batería', 'Energie und Akku'),
    'Application': ('App', 'Aplicación', 'App'),
    'Arrière-plan': ('Background', 'Fondo', 'Hintergrund'),
    'Aucun': ('None', 'Ninguno', 'Keiner'),
    'Aucun dossier épinglé': ('No pinned folders', 'Ninguna carpeta anclada', 'Keine angehefteten Ordner'),
    'Aucun lecteur détecté': ('No drive detected', 'No se detectó ninguna unidad', 'Kein Laufwerk erkannt'),
    'Aucune nouvelle notification': ('No new notifications', 'No hay notificaciones nuevas', 'Keine neuen Benachrichtigungen'),
    'Aucune sauvegarde': ('No backup', 'Sin copia de seguridad', 'Keine Sicherung'),
    'Aucune sortie audio détectée.': ('No audio output detected.', 'No se detectó ninguna salida de audio.', 'Keine Audioausgabe erkannt.'),
    'Batterie : {n} %': ('Battery: {n}%', 'Batería: {n} %', 'Akku: {n} %'),
    'Bluetooth': ('Bluetooth', 'Bluetooth', 'Bluetooth'),
    'Branchez une clé USB ou un disque : il apparaîtra ici.': ('Plug in a USB drive or disk: it will show up here.', 'Conecte una memoria USB o un disco: aparecerá aquí.', 'Schließen Sie einen USB-Stick oder Datenträger an: Er erscheint hier.'),
    'Branché': ('Plugged in', 'Enchufado', 'Angeschlossen'),
    'Bureau': ('Desktop', 'Escritorio', 'Desktop'),
    'Bureau et barre des tâches': ('Desktop & taskbar', 'Escritorio y barra de tareas', 'Desktop und Taskleiste'),
    'Choisir un thème': ('Choose a theme', 'Elegir un tema', 'Design auswählen'),
    'Clair': ('Light', 'Claro', 'Hell'),
    'Clé de secours': ('Recovery drive', 'Unidad de recuperación', 'Wiederherstellungslaufwerk'),
    'Copier vos documents sur un autre disque, régulièrement.': ('Copy your documents to another drive, regularly.', 'Copiar sus documentos en otro disco con regularidad.', 'Ihre Dokumente regelmäßig auf ein anderes Laufwerk kopieren.'),
    'Couleur d’accentuation': ('Accent color', 'Color de énfasis', 'Akzentfarbe'),
    'Couleur personnalisée': ('Custom color', 'Color personalizado', 'Benutzerdefinierte Farbe'),
    'Créer une clé': ('Create a drive', 'Crear una unidad', 'Laufwerk erstellen'),
    'Disponible': ('Available', 'Disponible', 'Verfügbar'),
    'Disque système': ('System drive', 'Disco del sistema', 'Systemlaufwerk'),
    'Dossiers épinglés des fenêtres Ouvrir et Enregistrer': ('Folders pinned in Open and Save windows', 'Carpetas ancladas en las ventanas Abrir y Guardar', 'In „Öffnen“ und „Speichern“ angeheftete Ordner'),
    'Démarrage avancé': ('Advanced startup', 'Inicio avanzado', 'Erweiterter Start'),
    'Effets visuels, écran de veille, transparence…': ('Visual effects, screen saver, transparency…', 'Efectos visuales, protector de pantalla, transparencia…', 'Visuelle Effekte, Bildschirmschoner, Transparenz…'),
    'En charge': ('Charging', 'Cargando', 'Wird geladen'),
    'Envoyer un rapport aux développeurs de Winlux.': ('Send a report to the Winlux developers.', 'Enviar un informe a los desarrolladores de Winlux.', 'Einen Bericht an die Winlux-Entwickler senden.'),
    'Ethernet': ('Ethernet', 'Ethernet', 'Ethernet'),
    'Explorateur introuvable.': ('Explorer not found.', 'No se encontró el Explorador.', 'Explorer nicht gefunden.'),
    'Gestion des disques': ('Disk management', 'Administración de discos', 'Datenträgerverwaltung'),
    'Grande': ('Large', 'Grande', 'Groß'),
    'Guide d’accueil': ('Welcome guide', 'Guía de bienvenida', 'Willkommensleitfaden'),
    'Hauteur de la barre et de ses icônes.': ('Height of the taskbar and its icons.', 'Altura de la barra y de sus iconos.', 'Höhe der Taskleiste und ihrer Symbole.'),
    'Icône Winlux d’origine': ('Original Winlux icon', 'Icono original de Winlux', 'Ursprüngliches Winlux-Symbol'),
    'Image personnalisée': ('Custom image', 'Imagen personalizada', 'Eigenes Bild'),
    'Images': ('Pictures', 'Imágenes', 'Bilder'),
    'Le thème change les fenêtres, le menu Démarrer et la barre des tâches.': ('The theme changes windows, the Start menu and the taskbar.', 'El tema cambia las ventanas, el menú Inicio y la barra de tareas.', 'Das Design ändert Fenster, Startmenü und Taskleiste.'),
    'Lecteurs et périphériques': ('Drives & devices', 'Unidades y dispositivos', 'Laufwerke und Geräte'),
    'Lecteurs réseau': ('Network drives', 'Unidades de red', 'Netzlaufwerke'),
    'Les messages des applications apparaîtront ici.': ('Messages from your apps will show up here.', 'Los mensajes de las aplicaciones aparecerán aquí.', 'Nachrichten Ihrer Apps erscheinen hier.'),
    'Les notifications restent dans l’historique, sans bannière à l’écran.': ('Notifications stay in the history, without a banner on screen.', 'Las notificaciones quedan en el historial, sin aviso en pantalla.', 'Benachrichtigungen bleiben im Verlauf, ohne Banner auf dem Bildschirm.'),
    'Luminosité': ('Brightness', 'Brillo', 'Helligkeit'),
    'Messages récents des applications et de Winlux.': ('Recent messages from your apps and Winlux.', 'Mensajes recientes de las aplicaciones y de Winlux.', 'Aktuelle Nachrichten Ihrer Apps und von Winlux.'),
    'Mode d’alimentation, mise en veille et écran.': ('Power mode, sleep and screen.', 'Modo de energía, suspensión y pantalla.', 'Energiemodus, Energiesparen und Bildschirm.'),
    'Moyenne': ('Medium', 'Mediana', 'Mittel'),
    'Musique': ('Music', 'Música', 'Musik'),
    'Ne pas déranger': ('Do not disturb', 'No molestar', 'Nicht stören'),
    'Options de démarrage': ('Startup options', 'Opciones de inicio', 'Startoptionen'),
    'Options de récupération': ('Recovery options', 'Opciones de recuperación', 'Wiederherstellungsoptionen'),
    'Ouvrir': ('Open', 'Abrir', 'Öffnen'),
    'Ouvrir l’Explorateur': ('Open Explorer', 'Abrir el Explorador', 'Explorer öffnen'),
    'Ouvrir l’application Paramètres.': ('Open the Settings app.', 'Abrir la aplicación Configuración.', 'Die App „Einstellungen“ öffnen.'),
    'Par défaut': ('Default', 'Predeterminado', 'Standard'),
    'Paramètres du son': ('Sound settings', 'Configuración de sonido', 'Soundeinstellungen'),
    'Parcourir': ('Browse', 'Examinar', 'Durchsuchen'),
    'Parcourir les images': ('Browse photos', 'Examinar las imágenes', 'Bilder durchsuchen'),
    'Partages Windows, FTP et SFTP dans l’Explorateur.': ('Windows shares, FTP and SFTP in Explorer.', 'Recursos compartidos de Windows, FTP y SFTP en el Explorador.', 'Windows-Freigaben, FTP und SFTP im Explorer.'),
    'Partitions, formatage et santé des disques.': ('Partitions, formatting and disk health.', 'Particiones, formato y estado de los discos.', 'Partitionen, Formatierung und Zustand der Datenträger.'),
    'Petite': ('Small', 'Pequeña', 'Klein'),
    'Plus de personnalisation': ('More personalization', 'Más personalización', 'Weitere Personalisierung'),
    'Plus de réglages': ('More settings', 'Más opciones', 'Weitere Einstellungen'),
    'Plus d’options': ('More options', 'Más opciones', 'Weitere Optionen'),
    'Préparer une clé USB qui démarre Winlux et répare ce PC.': ('Prepare a USB drive that starts Winlux and repairs this PC.', 'Preparar una memoria USB que inicie Winlux y repare este equipo.', 'Einen USB-Stick vorbereiten, der Winlux startet und diesen PC repariert.'),
    'Redémarrer dans le menu de secours pour réparer le démarrage, même si le bureau ne s’ouvre plus.': ('Restart into the recovery menu to repair startup, even if the desktop no longer opens.', 'Reiniciar en el menú de recuperación para reparar el inicio, aunque el escritorio ya no se abra.', 'Im Wiederherstellungsmenü neu starten, um den Start zu reparieren, auch wenn der Desktop nicht mehr öffnet.'),
    'Revenir en arrière ou réparer Winlux si quelque chose ne va pas.': ('Go back or repair Winlux if something goes wrong.', 'Volver atrás o reparar Winlux si algo va mal.', 'Zurückgehen oder Winlux reparieren, wenn etwas nicht stimmt.'),
    'Revenir à la version précédente': ('Go back to the previous version', 'Volver a la versión anterior', 'Zur vorherigen Version zurückkehren'),
    'Réinitialiser ce PC': ('Reset this PC', 'Restablecer este equipo', 'Diesen PC zurücksetzen'),
    'Réinitialiser le PC': ('Reset PC', 'Restablecer el equipo', 'PC zurücksetzen'),
    'Réinstaller Winlux en gardant ou en supprimant vos fichiers personnels.': ('Reinstall Winlux, keeping or removing your personal files.', 'Reinstalar Winlux conservando o quitando sus archivos personales.', 'Winlux neu installieren und persönliche Dateien behalten oder entfernen.'),
    'Réseau, son, luminosité et modes en un coup d’œil.': ('Network, sound, brightness and modes at a glance.', 'Red, sonido, brillo y modos de un vistazo.', 'Netzwerk, Ton, Helligkeit und Modi auf einen Blick.'),
    'Rétablir': ('Restore', 'Restablecer', 'Wiederherstellen'),
    'Sauvegarde de vos fichiers': ('Back up your files', 'Copia de seguridad de sus archivos', 'Sicherung Ihrer Dateien'),
    'Si le bureau ne démarre plus : entrée « Winlux secours » au démarrage, ou une clé d’installation Winlux.': ('If the desktop no longer starts: the “Winlux recovery” entry at startup, or a Winlux installation drive.', 'Si el escritorio ya no se inicia: entrada «Winlux secours» al arrancar, o una memoria de instalación de Winlux.', 'Wenn der Desktop nicht mehr startet: der Eintrag „Winlux secours“ beim Start oder ein Winlux-Installationsstick.'),
    'Si votre PC ne fonctionne pas correctement, ces options peuvent vous aider. Vos documents ne sont pas une sauvegarde système : gardez-en aussi une copie sur un autre support.': ("If your PC isn't running well, these options might help. Your documents are not a system backup: keep a copy of them on another drive too.", 'Si su equipo no funciona bien, estas opciones pueden ayudar. Sus documentos no son una copia del sistema: guarde también una copia en otro soporte.', 'Wenn Ihr PC nicht richtig läuft, können diese Optionen helfen. Ihre Dokumente sind keine Systemsicherung: Bewahren Sie auch eine Kopie auf einem anderen Datenträger auf.'),
    'Signaler un problème': ('Report a problem', 'Informar de un problema', 'Problem melden'),
    'Sombre': ('Dark', 'Oscuro', 'Dunkel'),
    'Supprimer cette notification': ('Dismiss this notification', 'Descartar esta notificación', 'Diese Benachrichtigung schließen'),
    'Sur batterie': ('On battery', 'Con batería', 'Akkubetrieb'),
    'Taille de la barre des tâches': ('Taskbar size', 'Tamaño de la barra de tareas', 'Taskleistengröße'),
    'Thème sombre, violet Winlux, fond Aurore et barre moyenne.': ('Dark theme, Winlux violet, Aurore background and medium taskbar.', 'Tema oscuro, violeta Winlux, fondo Aurore y barra mediana.', 'Dunkles Design, Winlux-Violett, Hintergrund Aurore und mittlere Taskleiste.'),
    'Thème, couleur d’accentuation, arrière-plan et barre des tâches.': ('Theme, accent color, background and taskbar.', 'Tema, color de énfasis, fondo y barra de tareas.', 'Design, Akzentfarbe, Hintergrund und Taskleiste.'),
    'Tous les paramètres': ('All settings', 'Toda la configuración', 'Alle Einstellungen'),
    'Tout effacer': ('Clear all', 'Borrar todo', 'Alle löschen'),
    'Toute couleur : les textes restent lisibles, Winlux ajuste les contrastes.': ('Any color: text stays readable, Winlux adjusts the contrast.', 'Cualquier color: los textos siguen legibles, Winlux ajusta los contrastes.', 'Jede Farbe: Texte bleiben lesbar, Winlux passt die Kontraste an.'),
    'Très grande': ('Extra large', 'Muy grande', 'Sehr groß'),
    'Très petite': ('Extra small', 'Muy pequeña', 'Sehr klein'),
    'Vidéos': ('Videos', 'Vídeos', 'Videos'),
    'Voir une image ou un texte sans l’ouvrir.': ('View a picture or text without opening it.', 'Ver una imagen o un texto sin abrirlo.', 'Ein Bild oder einen Text ansehen, ohne ihn zu öffnen.'),
    'Volume': ('Volume', 'Volumen', 'Lautstärke'),
    'Vos dossiers, lecteurs et périphériques amovibles.': ('Your folders, drives and removable devices.', 'Sus carpetas, unidades y dispositivos extraíbles.', 'Ihre Ordner, Laufwerke und Wechselmedien.'),
    'Wi-Fi': ('Wi-Fi', 'Wi-Fi', 'WLAN'),
    'Winlux (par défaut)': ('Winlux (default)', 'Winlux (predeterminado)', 'Winlux (Standard)'),
    '{free} libres sur {size}': ('{free} free of {size}', '{free} libres de {size}', '{free} frei von {size}'),
    'Éclairage nocturne': ('Night light', 'Luz nocturna', 'Nachtmodus'),
    'Éjecter': ('Eject', 'Expulsar', 'Auswerfen'),
    'Éléments, comportement et position de la barre.': ('Items, behavior and position of the taskbar.', 'Elementos, comportamiento y posición de la barra.', 'Elemente, Verhalten und Position der Taskleiste.'),
    '{n} o': ('{n} B', '{n} B', '{n} B'),
    '{n} Ko': ('{n} KB', '{n} KB', '{n} KB'),
    '{n} Mo': ('{n} MB', '{n} MB', '{n} MB'),
    '{n} Go': ('{n} GB', '{n} GB', '{n} GB'),
    '{n} To': ('{n} TB', '{n} TB', '{n} TB'),
})


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



# ── Apparence commune : mêmes couleurs que Paramètres (fond, cartes, accent) ──

APPEARANCE = HOME / ".config/winlux/appearance.json"
DEFAULT_ACCENT = "#6a3df0"
# Couleurs d'accentuation proposées (la première est celle de Winlux).
ACCENTS = ("#6a3df0", "#0063b1", "#0f7b6c", "#107c10", "#c75100", "#c42b1c", "#b4009e", "#5d5a58")
WALLPAPER = Path("/usr/share/winlux/wallpapers/winlux-prisme-aurore.png")


def appearance_state():
    try:
        state = json.loads(APPEARANCE.read_text())
        return state if isinstance(state, dict) else {}
    except (OSError, ValueError):
        return {}


def dark_theme():
    """Thème sombre demandé : GTK_THEME, préférence GTK ou thème « …-dark »."""
    forced = os.environ.get("GTK_THEME")
    if forced:
        return forced.lower().endswith((":dark", "-dark", "inverse"))
    settings = Gtk.Settings.get_default()
    name = (settings.get_property("gtk-theme-name") or "").lower()
    if name == "highcontrast":
        return False
    if name.startswith("winlux-"):
        return "-dark" in name
    return bool(settings.get_property("gtk-application-prefer-dark-theme")) or "dark" in name or "inverse" in name


def _rgb(color):
    return tuple(int(color[i:i + 2], 16) for i in (1, 3, 5))


def _hex(rgb):
    return "#%02x%02x%02x" % tuple(max(0, min(255, round(v))) for v in rgb)


def _mix(a, b, amount):
    return _hex(tuple(x + (y - x) * amount for x, y in zip(_rgb(a), _rgb(b))))


def _luminance(color):
    def channel(v):
        v /= 255
        return v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4
    r, g, b = (channel(v) for v in _rgb(color))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def _contrast(a, b):
    la, lb = sorted((_luminance(a), _luminance(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def _text_on(color):
    return "#ffffff" if _contrast("#ffffff", color) >= _contrast("#1a1a1a", color) else "#1a1a1a"


def palette(dark, accent):
    """Couleurs de Paramètres ; l'accent sert de fond (tuiles, bouton principal)
    et, éclairci en thème sombre, de couleur de lien (#B69CFF pour Winlux)."""
    if not re.fullmatch(r"#[0-9a-fA-F]{6}", accent or ""):
        accent = DEFAULT_ACCENT
    accent = accent.lower()
    if dark:
        p = {"bg": "#181b24", "fg": "#ecedf5", "card": "#242a38", "card_hover": "#2c3344", "border": "#48536b",
             "button": "#30384b", "button_hover": "#414b64", "dim": "#adb6c9", "entry": "#171b25",
             "disabled_fg": "#858da0", "disabled_bg": "#252a36"}
        link = "#b69cff" if accent == DEFAULT_ACCENT else _mix(accent, "#ffffff", 0.45)
    else:
        p = {"bg": "#f3f5fa", "fg": "#1f2330", "card": "#ffffff", "card_hover": "#eef1f7", "border": "#b9c0d1",
             "button": "#ffffff", "button_hover": "#e6eaf3", "dim": "#4f566b", "entry": "#ffffff",
             "disabled_fg": "#6f7689", "disabled_bg": "#eceef3"}
        link = accent
        while _contrast(link, p["card"]) < 4.5:
            link = _mix(link, "#000000", 0.15)
    while _contrast(link, p["card"]) < 4.5:
        link = _mix(link, "#ffffff", 0.15)
    p["accent"] = accent
    p["on_accent"] = _text_on(accent)
    p["accent_hover"] = _mix(accent, p["fg"], 0.12)
    p["link"] = link
    p["card_border"] = _mix(p["card"], p["border"], 0.5)
    p["badge"] = _mix(p["card"], accent, 0.22 if dark else 0.14)
    # Le glyphe des pastilles se lit sur le fond teinté, pas sur la carte.
    badge_fg = link
    while _contrast(badge_fg, p["badge"]) < 4.5:
        badge_fg = _mix(badge_fg, "#ffffff" if dark else "#000000", 0.15)
    p["badge_fg"] = badge_fg
    p["ok"] = "#6ccb5f" if dark else "#0f7b0f"
    p["warn"] = "#fce100" if dark else "#9d5d00"
    p["ok_bg"] = _mix(p["card"], "#6ccb5f" if dark else "#0f7b0f", 0.16)
    p["warn_bg"] = _mix(p["card"], "#fce100" if dark else "#9d5d00", 0.14)
    p["trough"] = p["border"]
    return p


TOOLS_CSS = """
  window, window box, window viewport, window label, window grid, window flowbox, window flowboxchild {{ background-color: transparent; }}
  window {{ background: {bg}; color: {fg}; }}
  label {{ color: {fg}; }}
  #page {{ background: {bg}; }}
  .page-icon {{ background: {badge}; border-radius: 12px; padding: 12px; color: {badge_fg}; }}
  .page-icon image {{ color: {badge_fg}; }}
  .page-title {{ font-size: 24px; font-weight: 600; }}
  .dim, label.dim {{ color: {dim}; }}
  .small {{ font-size: 12px; }}
  .section {{ font-size: 15px; font-weight: 600; margin-top: 8px; }}
  .card {{ background: {card}; border-radius: 8px; border: 1px solid {card_border}; }}
  .row {{ padding: 14px 16px; }}
  .row-title {{ font-weight: 500; }}
  .row-icon {{ color: {fg}; }}
  .sep {{ background: {card_border}; min-height: 1px; }}
  button {{ background: {button}; color: {fg}; border: 1px solid {border}; border-radius: 6px; padding: 6px 14px;
           box-shadow: none; background-image: none; text-shadow: none; min-height: 20px; }}
  button label, button image {{ color: {fg}; }}
  button:hover {{ background: {button_hover}; }}
  button.accent {{ background: {accent}; border-color: {accent}; }}
  button.accent label, button.accent image {{ color: {on_accent}; }}
  button.accent:hover {{ background: {accent_hover}; border-color: {accent_hover}; }}
  button.flat {{ background: transparent; border-color: transparent; }}
  button.flat:hover {{ background: {card_hover}; }}
  button.link label {{ color: {link}; }}
  button.row-button {{ background: transparent; border: none; border-radius: 0; padding: 0; }}
  button.row-button:hover {{ background: {card_hover}; }}
  button:disabled, button:disabled label {{ color: {disabled_fg}; background: {disabled_bg}; }}
  button.icon-button {{ padding: 0; min-width: 20px; min-height: 20px; }}
  .tile button {{ min-height: 44px; padding: 0 12px; }}
  .tile button.tile-main {{ border-top-right-radius: 6px; border-bottom-right-radius: 6px; }}
  .tile.split button.tile-main {{ border-top-right-radius: 0; border-bottom-right-radius: 0; border-right-width: 0; }}
  .tile button.tile-more {{ border-top-left-radius: 0; border-bottom-left-radius: 0; padding: 0 6px; }}
  .tile button:checked, .tile button.on {{ background: {accent}; border-color: {accent}; }}
  .tile button:checked image, .tile button.on image {{ color: {on_accent}; }}
  .tile button:checked:hover, .tile button.on:hover {{ background: {accent_hover}; }}
  .tile-label {{ font-size: 12px; }}
  .swatch {{ border-radius: 6px; min-width: 36px; min-height: 36px; padding: 0; border: 2px solid transparent; }}
  .swatch.chosen {{ border: 2px solid {fg}; }}
  .theme-card {{ padding: 8px; border-radius: 8px; }}
  .theme-card.chosen {{ border: 2px solid {link}; }}
  .badge {{ background: {badge}; color: {badge_fg}; border-radius: 10px; padding: 2px 10px; font-size: 12px; font-weight: 600; }}
  .ok-badge {{ background: {ok_bg}; color: {ok}; border-radius: 10px; padding: 2px 10px; font-size: 12px; font-weight: 600; }}
  .warn-badge {{ background: {warn_bg}; color: {fg}; border-radius: 10px; padding: 2px 10px; font-size: 12px; font-weight: 600; }}
  .ok-icon {{ color: {ok}; }}
  .banner {{ background: {badge}; border-radius: 8px; padding: 14px 16px; }}
  .banner image {{ color: {badge_fg}; }}
  .notice {{ color: {dim}; }}
  entry {{ background: {entry}; color: {fg}; border: 1px solid {border}; border-radius: 6px; padding: 6px 8px; }}
  switch {{ border: 1px solid {border}; background: {button}; }}
  switch:checked {{ background: {accent}; border-color: {accent}; }}
  switch slider {{ background: #ffffff; border: 1px solid {border}; }}
  scale trough {{ background: {trough}; border: none; min-height: 4px; border-radius: 2px; }}
  scale highlight {{ background: {accent}; border: none; border-radius: 2px; }}
  scale slider {{ background: #ffffff; border: 4px solid {accent}; min-width: 18px; min-height: 18px; }}
  levelbar trough {{ background: {trough}; border: none; min-height: 6px; border-radius: 3px; padding: 0; }}
  levelbar block.filled {{ background: {accent}; border: none; border-radius: 3px; }}
  levelbar block.empty {{ background: transparent; border: none; }}
  combobox button, combobox button label, menu, menuitem, menuitem label {{ background: {button}; color: {fg}; }}
  menuitem:hover, menuitem:hover label {{ background: {accent}; color: {on_accent}; }}
  treeview, treeview.view {{ background: {card}; color: {fg}; }}
  treeview header button {{ background: {card}; border-radius: 0; }}
  textview, textview text {{ background: {card}; color: {fg}; }}
"""

# Icône de fenêtre, titre et phrase d'accroche de chaque outil.
PAGES = {
    "quick": ("preferences-system", "Réglages rapides", "Réseau, son, luminosité et modes en un coup d’œil."),
    "notifications": ("preferences-system-notifications", "Notifications", "Messages récents des applications et de Winlux."),
    "tasks": ("utilities-system-monitor", "Gestionnaire de tâches", ""),
    "appearance": ("preferences-desktop-theme", "Personnalisation", "Thème, couleur d’accentuation, arrière-plan et barre des tâches."),
    "apps": ("system-software-install", "Applications", ""),
    "files": ("system-file-manager", "Fichiers et périphériques", "Vos dossiers, lecteurs et périphériques amovibles."),
    "recovery": ("preferences-system", "Secours et restauration", "Revenir en arrière ou réparer Winlux si quelque chose ne va pas."),
}
# Glyphe monochrome affiché en tête de page (thème d'icônes symbolique).
PAGE_GLYPHS = {"quick": "emblem-system-symbolic", "notifications": "preferences-system-notifications-symbolic",
               "appearance": "applications-graphics-symbolic", "files": "folder-symbolic",
               "recovery": "view-refresh-symbolic", "tasks": ("utilities-system-monitor-symbolic", "view-list-symbolic"),
               "apps": "view-app-grid-symbolic"}


def icon(name, size=Gtk.IconSize.LARGE_TOOLBAR, fallback="image-missing"):
    theme = Gtk.IconTheme.get_default()
    names = [name] if isinstance(name, str) else list(name)
    for candidate in names:
        if theme.has_icon(candidate):
            return Gtk.Image.new_from_icon_name(candidate, size)
    return Gtk.Image.new_from_icon_name(fallback, size)


def size_text(value):
    value = float(value)
    for unit in ("o", "Ko", "Mo", "Go", "To"):
        if value < 1000 or unit == "To":
            text = f"{value:.0f}" if value >= 100 or unit == "o" else f"{value:.1f}"
            return _t("{n} " + unit).format(n=text.replace(".", "," if _LANG in ("fr", "es", "de") else "."))
        value /= 1000
    return str(value)


def popup_module():
    """Fonctions de lecture des réglages rapides du panneau volant (même
    matériel détecté, mêmes commandes) : winlux-popup installé ou script voisin."""
    import importlib.machinery
    import importlib.util
    for candidate in (Path(__file__).resolve().with_name("winlux-popup.py"), Path(shutil.which("winlux-popup") or "/nonexistent")):
        if candidate.is_file():
            try:
                loader = importlib.machinery.SourceFileLoader("winlux_popup_tools", str(candidate))
                spec = importlib.util.spec_from_loader(loader.name, loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module
            except Exception:
                continue
    return None


class Tools(Gtk.Window):
    def __init__(self, page):
        super().__init__(title=_t("Outils Winlux"))
        # Même icône que l'entrée du menu Démarrer qui ouvre cette page.
        self.set_icon_name({"quick": "preferences-system", "notifications": "preferences-system-notifications",
            "tasks": "utilities-system-monitor", "appearance": "preferences-desktop-theme", "apps": "system-software-install",
            "files": "system-file-manager", "recovery": "preferences-system"}.get(page, "preferences-system"))
        self.set_default_size(880, {"quick": 600, "notifications": 640, "recovery": 760}.get(page, 720))
        self.connect("destroy", Gtk.main_quit)
        self.page = page
        self.process = None
        self.previous = {}
        self.previous_time = time.monotonic()
        self.css = Gtk.CssProvider()
        Gtk.StyleContext.add_provider_for_screen(self.get_screen(), self.css, Gtk.STYLE_PROVIDER_PRIORITY_USER + 1)
        self.apply_theme()
        settings = Gtk.Settings.get_default()
        for name in ("gtk-theme-name", "gtk-application-prefer-dark-theme"):
            settings.connect(f"notify::{name}", lambda *_: self.apply_theme())
        _, title, subtitle = PAGES.get(page, ("", "Winlux", ""))
        self.set_title(_t(title))
        scroll = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER)
        frame = Gtk.Box()
        frame.set_name("page")
        scroll.add(frame)
        self.add(scroll)
        # Colonne centrée, 760 px au plus, comme une page de Paramètres.
        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16, margin_start=32, margin_end=32,
                           margin_top=28, margin_bottom=28, halign=Gtk.Align.FILL, hexpand=True)
        self.box.set_size_request(min(760, 600), -1)
        clamp = Gtk.Box(halign=Gtk.Align.CENTER, hexpand=True)
        clamp.pack_start(self.box, True, True, 0)
        frame.pack_start(clamp, True, True, 0)
        self.connect("size-allocate", self.clamp_width)
        header = Gtk.Box(spacing=16)
        badge = Gtk.Box(valign=Gtk.Align.CENTER)
        badge.get_style_context().add_class("page-icon")
        glyph = icon(PAGE_GLYPHS.get(page, "emblem-system-symbolic"), Gtk.IconSize.DND)
        glyph.set_pixel_size(28)
        badge.pack_start(glyph, False, False, 0)
        header.pack_start(badge, False, False, 0)
        titles = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, valign=Gtk.Align.CENTER)
        heading = Gtk.Label(label=_t(title), xalign=0)
        heading.get_style_context().add_class("page-title")
        titles.pack_start(heading, False, False, 0)
        if subtitle:
            line = Gtk.Label(label=_t(subtitle), xalign=0, wrap=True)
            line.get_style_context().add_class("dim")
            titles.pack_start(line, False, False, 0)
        header.pack_start(titles, True, True, 0)
        self.header_end = Gtk.Box(spacing=8, valign=Gtk.Align.CENTER)
        header.pack_end(self.header_end, False, False, 0)
        self.box.pack_start(header, False, False, 0)
        self.content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        self.box.pack_start(self.content, True, True, 0)
        self.notice = Gtk.Label(label="", xalign=0, wrap=True, selectable=True)
        self.notice.get_style_context().add_class("notice")
        self.box.pack_end(self.notice, False, False, 0)
        getattr(self, page)()
        GLib.timeout_add(1000, self.poll)
        self.show_all()

    def clamp_width(self, _window, allocation):
        width = max(480, min(760, allocation.width - 64))
        if self.box.get_size_request()[0] != width:
            self.box.set_size_request(width, -1)

    def apply_theme(self):
        p = palette(dark_theme(), appearance_state().get("accent", DEFAULT_ACCENT))
        self.colors = p
        self.css.load_from_data(TOOLS_CSS.format(**p).encode())

    # ── Briques de mise en page ──────────────────────────────────────────

    def section(self, text, parent=None, end=None):
        line = Gtk.Box(spacing=8)
        label = Gtk.Label(label=text, xalign=0)
        label.get_style_context().add_class("section")
        line.pack_start(label, True, True, 0)
        if end is not None:
            line.pack_end(end, False, False, 0)
        (parent or self.content).pack_start(line, False, False, 4)
        return line

    def card(self, parent=None):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        box.get_style_context().add_class("card")
        (parent or self.content).pack_start(box, False, False, 0)
        return box

    def row(self, card, icon_name, title, subtitle="", end=None, activate=None):
        """Ligne de carte façon Paramètres : icône, titre, explication, réglage à droite.
        Une ligne entière cliquable (activate) se termine par un chevron."""
        if card.get_children():
            sep = Gtk.Box()
            sep.get_style_context().add_class("sep")
            card.pack_start(sep, False, False, 0)
        line = Gtk.Box(spacing=16)
        line.get_style_context().add_class("row")
        if icon_name:
            image = icon(icon_name, Gtk.IconSize.LARGE_TOOLBAR) if isinstance(icon_name, (str, tuple, list)) else icon_name
            if isinstance(icon_name, (str, tuple, list)):
                image.set_pixel_size(20)
            image.get_style_context().add_class("row-icon")
            image.set_valign(Gtk.Align.CENTER)
            line.pack_start(image, False, False, 0)
        texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, valign=Gtk.Align.CENTER)
        head = Gtk.Label(label=title, xalign=0, wrap=True)
        head.get_style_context().add_class("row-title")
        texts.pack_start(head, False, False, 0)
        sub = Gtk.Label(label=subtitle, xalign=0, wrap=True)
        sub.get_style_context().add_class("dim")
        sub.get_style_context().add_class("small")
        sub.set_no_show_all(not subtitle)
        texts.pack_start(sub, False, False, 0)
        line.pack_start(texts, True, True, 0)
        ends = list(end) if isinstance(end, (list, tuple)) else ([end] if end is not None else [])
        if activate is not None:
            chevron = icon("go-next-symbolic", Gtk.IconSize.MENU)
            chevron.set_valign(Gtk.Align.CENTER)
            line.pack_end(chevron, False, False, 0)
        for widget in reversed(ends):
            widget.set_valign(Gtk.Align.CENTER)
            line.pack_end(widget, False, False, 0)
        if activate is not None:
            button = Gtk.Button()
            button.get_style_context().add_class("row-button")
            button.add(line)
            button.connect("clicked", lambda *_: activate())
            card.pack_start(button, False, False, 0)
        else:
            card.pack_start(line, False, False, 0)
        line.subtitle = sub
        return line

    def action(self, text, callback, style=None, available=True):
        widget = Gtk.Button(label=text)
        widget.set_sensitive(available)
        if style:
            widget.get_style_context().add_class(style)
        widget.connect("clicked", lambda *_: callback())
        return widget

    def label(self, text):
        widget = Gtk.Label(label=text, xalign=0, wrap=True, selectable=True)
        self.content.pack_start(widget, False, False, 0)
        return widget

    def button(self, text, callback, available=True):
        widget = self.action(text, callback, available=available)
        widget.set_halign(Gtk.Align.START)
        self.content.pack_start(widget, False, False, 0)
        return widget

    def entry(self, placeholder):
        entry = Gtk.Entry(placeholder_text=placeholder)
        self.content.pack_start(entry, False, False, 0)
        return entry

    def open_settings(self, page):
        """Page dédiée de Paramètres ; cette fenêtre se ferme derrière elle."""
        try:
            subprocess.Popen(["winlux-shell-app", "settings", page], stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL, start_new_session=True)
        except OSError as error:
            self.notice.set_text(str(error))
            return
        self.destroy()

    def launch(self, args):
        """Programme graphique lancé à part (l'Explorateur, un dossier…)."""
        try:
            subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        except OSError as error:
            self.notice.set_text(str(error))

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
                if self.page == "appearance": GLib.timeout_add(300, self.reload_appearance)
        if self.page == "tasks": self.refresh_tasks()
        if self.page == "notifications":
            stamp = read(DATA / "notifications.json")
            if stamp != self.notification_snapshot:
                self.rebuild(self.notifications)
        return True

    def rebuild(self, builder):
        for widget in self.content.get_children(): widget.destroy()
        for widget in self.header_end.get_children(): widget.destroy()
        builder()
        self.content.show_all()
        self.header_end.show_all()
        return False

    # ── Réglages rapides ─────────────────────────────────────────────────

    def quick(self):
        # Sections sans matériel masquées (règle Winlux) : pas de Wi-Fi,
        # Bluetooth ni luminosité proposés dans une VM ou sur un PC fixe.
        def present(directory, pattern="*"):
            try:
                return any(Path(directory).glob(pattern))
            except OSError:
                return False
        self.popup = popup_module()
        if self.popup:
            self.hardware = self.popup.quick_hardware()
            self.in_vm = self.popup.virtual_machine()
        else:
            self.hardware = {"wifi": present("/sys/class/ieee80211") or present("/sys/class/net", "*/wireless"),
                             "bluetooth": present("/sys/class/bluetooth"), "backlight": present("/sys/class/backlight"),
                             "battery": present("/sys/class/power_supply", "BAT*/capacity")}
            self.in_vm = False
        self.quick_values = {}
        self.updating = False
        tiles_card = self.card()
        self.tile_grid = Gtk.Grid(column_spacing=12, row_spacing=14, column_homogeneous=True, margin=16)
        tiles_card.pack_start(self.tile_grid, False, False, 0)
        self.show_tiles({})
        sliders = self.card()
        self.light = None
        if self.hardware.get("backlight"):
            self.light = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 100, 1)
            self.light.set_draw_value(False)
            self.light.set_hexpand(True)
            self.light.set_size_request(260, -1)
            self.light_value = Gtk.Label(label="", width_chars=4, xalign=1)
            self.light.connect("value-changed", self.light_changed)
            self.light_timer = 0
            self.row(sliders, "display-brightness-symbolic", _t("Luminosité"), "", [self.light, self.light_value])
        self.audio_backend = None
        self.volume = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 100, 1)
        self.volume.set_draw_value(False)
        self.volume.set_hexpand(True)
        self.volume.set_size_request(260, -1)
        self.volume_value = Gtk.Label(label="", width_chars=4, xalign=1)
        self.volume.connect("value-changed", self.volume_changed)
        self.volume_timer = 0
        self.mute = Gtk.Button()
        self.mute.get_style_context().add_class("flat")
        self.mute.get_style_context().add_class("icon-button")
        self.mute.set_tooltip_text(_t("Muet / rétablir"))
        self.mute.add(icon("audio-volume-high-symbolic", Gtk.IconSize.LARGE_TOOLBAR))
        self.mute.connect("clicked", lambda *_: self.toggle_mute())
        self.sound_settings = self.action(_t("Paramètres du son"), lambda: self.open_settings("16"))
        self.sound_settings.set_no_show_all(True)
        self.sound_row = self.row(sliders, None, _t("Volume"), "", [self.volume, self.volume_value, self.sound_settings])
        self.sound_row.pack_start(self.mute, False, False, 0)
        self.sound_row.reorder_child(self.mute, 0)
        footer = self.card()
        battery = self.popup.battery_level() if self.popup and self.hardware.get("battery") else None
        if battery:
            level, status = battery
            state = _t("En charge") if status == "Charging" else _t("Sur batterie") if status == "Discharging" else _t("Branché")
            self.row(footer, self.popup.battery_icon(level, status), _t("Batterie : {n} %").format(n=level), state,
                     activate=lambda: self.open_settings("23"))
        self.row(footer, "system-shutdown-symbolic", _t("Alimentation et batterie") if self.hardware.get("battery") else _t("Alimentation"),
                 _t("Mode d’alimentation, mise en veille et écran."), activate=lambda: self.open_settings("23"))
        self.row(footer, "preferences-system-symbolic", _t("Tous les paramètres"), _t("Ouvrir l’application Paramètres."),
                 activate=lambda: self.open_settings("8"))
        self.read_quick()

    def read_quick(self):
        """État lu hors de la boucle GTK (nmcli, bluetoothctl… peuvent être lents)."""
        import threading
        def worker():
            state = {}
            if self.popup:
                try: state = self.popup.quick_state(self.hardware)
                except Exception: state = {}
                try: state["audio"] = self.popup.audio()
                except Exception: state["audio"] = (None, 0, False, "")
            tool = shutil.which("winlux-ambiance")
            if tool and "night" not in state:
                try:
                    values = dict(line.split("=", 1) for line in subprocess.run([tool, "status"], capture_output=True,
                                  text=True, timeout=5).stdout.splitlines() if "=" in line)
                    if values.get("supported") == "1": state["night"] = values.get("nightlight") == "on"
                except (OSError, subprocess.SubprocessError):
                    pass
            GLib.idle_add(self.apply_quick, state)
        threading.Thread(target=worker, daemon=True).start()

    def apply_quick(self, state):
        if not self.get_realized() and not self.get_visible():
            return False
        self.quick_values = state
        self.show_tiles(state)
        backend, level, muted, name = state.get("audio", (None, 0, False, ""))
        self.audio_backend = backend
        self.updating = True
        if backend:
            self.volume.set_value(level)
            self.volume_value.set_text(str(level))
            self.sound_row.subtitle.set_text(name)
            self.sound_row.subtitle.show()
        else:
            self.sound_row.subtitle.set_text(_t("Aucune sortie audio détectée."))
            self.sound_row.subtitle.show()
        # Sans sortie audio, pas de curseur grisé : le lien vers la page Son.
        for widget in (self.volume, self.volume_value):
            widget.set_visible(bool(backend))
        self.sound_settings.set_visible(not backend)
        self.mute.set_sensitive(bool(backend))
        image = self.mute.get_child()
        if self.popup:
            image.set_from_icon_name(self.popup.volume_icon(level, muted), Gtk.IconSize.LARGE_TOOLBAR)
        if self.light is not None and state.get("brightness") is not None:
            self.light.set_value(state["brightness"])
            self.light_value.set_text(str(state["brightness"]))
        self.updating = False
        return False

    def show_tiles(self, state):
        for child in self.tile_grid.get_children(): child.destroy()
        if self.popup:
            tiles = self.popup.quick_tiles(self.hardware, state, self.in_vm)
        else:
            tiles = []
            if self.hardware["wifi"]: tiles.append(("wifi", _t("Wi-Fi"), "network-wireless-symbolic", False, True))
            else: tiles.append(("ethernet", _t("Ethernet"), "network-wired-symbolic", False, False))
            if self.hardware["bluetooth"]: tiles.append(("bluetooth", _t("Bluetooth"), "bluetooth-active-symbolic", False, True))
            if "night" in state: tiles.append(("night", _t("Éclairage nocturne"), "night-light-symbolic", state["night"], False))
            tiles.append(("accessibility", _t("Accessibilité"), "preferences-desktop-accessibility-symbolic", False, False))
        for index, (key, title, icon_name, active, detail) in enumerate(tiles):
            self.tile_grid.attach(self.tile(key, title, icon_name, active, detail), index % 3, index // 3, 1, 1)
        # Toujours trois colonnes : deux tuiles ne s'étirent pas sur toute la largeur.
        for column in range(len(tiles), 3):
            self.tile_grid.attach(Gtk.Box(), column, 0, 1, 1)
        self.tile_grid.show_all()

    def tile(self, key, title, icon_name, active, detail):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        buttons = Gtk.Box()
        buttons.get_style_context().add_class("tile")
        main = Gtk.ToggleButton()
        main.get_style_context().add_class("tile-main")
        main.add(icon(icon_name, Gtk.IconSize.LARGE_TOOLBAR))
        main.set_active(active)
        main.set_hexpand(True)
        tips = {"night": _t("Éclairage nocturne : désactiver") if active else _t("Éclairage nocturne : activer")}
        main.set_tooltip_text(tips.get(key, title))
        main.connect("toggled", self.tile_toggled, key)
        buttons.pack_start(main, True, True, 0)
        if detail:
            buttons.get_style_context().add_class("split")
            more = Gtk.Button()
            more.get_style_context().add_class("tile-more")
            if active: more.get_style_context().add_class("on")
            more.add(icon("go-next-symbolic", Gtk.IconSize.MENU))
            more.set_tooltip_text(_t("Plus de réglages"))
            page = {"wifi": "35", "bluetooth": "22", "vpn": "40", "hotspot": "41"}.get(key, "2")
            if key == "wifi":
                more.connect("clicked", lambda *_: self.open_settings("35"))
            elif key == "bluetooth":
                more.connect("clicked", lambda *_: self.open_settings("22"))
            else:
                more.connect("clicked", lambda *_, page=page: self.open_settings(page))
            buttons.pack_start(more, False, False, 0)
        box.pack_start(buttons, False, False, 0)
        label = Gtk.Label(label=title, ellipsize=3, max_width_chars=18)
        label.get_style_context().add_class("tile-label")
        box.pack_start(label, False, False, 0)
        return box

    def tile_toggled(self, button, key):
        if self.updating: return
        on = button.get_active()
        pages = {"ethernet": "2", "accessibility": "12", "vpn": "40", "hotspot": "41", "saver": "23"}
        if key in pages:
            self.updating = True; button.set_active(not on); self.updating = False
            self.open_settings(pages[key]); return
        airplane = getattr(self.popup, "AIRPLANE", None)
        commands = []
        if key == "wifi":
            commands = [["winlux-wifi", "radio", "on" if on else "off"]] if shutil.which("winlux-wifi") else \
                [["nmcli", "radio", "wifi", "on" if on else "off"]]
            if on and airplane: airplane.unlink(missing_ok=True)
        elif key == "bluetooth":
            commands = [["bluetoothctl", "power", "on" if on else "off"]]
            if on and airplane: airplane.unlink(missing_ok=True)
        elif key == "airplane" and airplane:
            try:
                if on: airplane.parent.mkdir(parents=True, exist_ok=True); airplane.touch(mode=0o600)
                else: airplane.unlink(missing_ok=True)
            except OSError:
                pass
            if shutil.which("winlux-wifi") and self.hardware["wifi"]: commands.append(["winlux-wifi", "airplane", "on" if on else "off"])
            if self.hardware["bluetooth"]: commands.append(["bluetoothctl", "power", "off" if on else "on"])
        elif key == "night":
            commands = [["winlux-ambiance", "toggle"]]
        import threading
        def run_all():
            for args in commands:
                try: subprocess.run(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
                except (OSError, subprocess.SubprocessError): pass
            self.read_quick()
        threading.Thread(target=run_all, daemon=True).start()

    def light_changed(self, scale):
        self.light_value.set_text(str(round(scale.get_value())))
        if self.updating: return
        if self.light_timer: GLib.source_remove(self.light_timer)
        def apply():
            self.light_timer = 0
            tool = self.popup.brightness_tool() if self.popup else (["winlux-brightness"] if shutil.which("winlux-brightness") else None)
            if tool: self.launch(tool + ["set", str(round(self.light.get_value()))])
            return False
        self.light_timer = GLib.timeout_add(120, apply)

    def volume_changed(self, scale):
        self.volume_value.set_text(str(round(scale.get_value())))
        if self.updating or not self.audio_backend: return
        if self.volume_timer: GLib.source_remove(self.volume_timer)
        def apply():
            self.volume_timer = 0
            value = round(self.volume.get_value())
            self.launch({"pactl": ["pactl", "set-sink-volume", "@DEFAULT_SINK@", f"{value}%"],
                         "wpctl": ["wpctl", "set-volume", "-l", "1", "@DEFAULT_AUDIO_SINK@", f"{value}%"],
                         "amixer": ["amixer", "set", "Master", f"{value}%"]}[self.audio_backend])
            return False
        self.volume_timer = GLib.timeout_add(120, apply)

    def toggle_mute(self):
        args = {"pactl": ["pactl", "set-sink-mute", "@DEFAULT_SINK@", "toggle"],
                "wpctl": ["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"],
                "amixer": ["amixer", "set", "Master", "toggle"]}.get(self.audio_backend)
        if args:
            self.launch(args)
            GLib.timeout_add(300, lambda: (self.read_quick(), False)[1])

    # ── Notifications ────────────────────────────────────────────────────

    def notification_call(self, method, parameters=None):
        try:
            Gio.bus_get_sync(Gio.BusType.SESSION, None).call_sync("org.freedesktop.Notifications",
                "/org/freedesktop/Notifications", "org.winlux.Notifications", method,
                parameters, None, Gio.DBusCallFlags.NONE, 3000, None)
        except GLib.Error as error: self.notice.set_text(str(error))

    def notifications(self):
        preference = Path(os.environ.get("XDG_CONFIG_HOME",HOME/".config")) / "winlux/do-not-disturb"
        switch = Gtk.Switch(active=preference.exists())
        def toggle(widget, _):
            preference.parent.mkdir(parents=True, exist_ok=True)
            if widget.get_active(): preference.touch(mode=0o600)
            else: preference.unlink(missing_ok=True)
        switch.connect("notify::active", toggle)
        quiet = self.card()
        self.row(quiet, "notifications-disabled-symbolic", _t("Ne pas déranger"),
                 _t("Les notifications restent dans l’historique, sans bannière à l’écran."), switch)
        self.notification_snapshot = read(DATA / "notifications.json")
        try: history = json.loads(self.notification_snapshot or "[]")
        except ValueError: history = []
        if not isinstance(history, list): history = []
        history = [item for item in history if isinstance(item, dict)][-100:]
        clear = self.action(_t("Tout effacer"), lambda: self.notification_call("Clear"), "flat")
        clear.get_style_context().add_class("link")
        self.section(_t("Notifications"), end=clear if history else None)
        if not history:
            empty = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin_top=36, margin_bottom=36)
            bell = icon("preferences-system-notifications-symbolic", Gtk.IconSize.DIALOG)
            bell.set_pixel_size(48)
            bell.get_style_context().add_class("dim")
            empty.pack_start(bell, False, False, 0)
            line = Gtk.Label(label=_t("Aucune nouvelle notification"))
            line.get_style_context().add_class("row-title")
            empty.pack_start(line, False, False, 0)
            hint = Gtk.Label(label=_t("Les messages des applications apparaîtront ici."))
            hint.get_style_context().add_class("dim")
            empty.pack_start(hint, False, False, 0)
            self.content.pack_start(empty, False, False, 0)
            return
        # Regroupées par application, la plus récente en premier (Windows 11).
        groups = []
        for item in reversed(history):
            app = str(item.get("app") or _t("Application"))
            group = next((g for g in groups if g[0] == app), None)
            if group is None:
                group = (app, [])
                groups.append(group)
            group[1].append(item)
        today = time.localtime(time.time())
        for app, items in groups:
            card = self.card()
            head = Gtk.Box(spacing=10, margin_start=16, margin_end=8, margin_top=10)
            app_icon = icon((str(items[0].get("icon") or ""), app.lower().replace(" ", "-"), "preferences-system-notifications-symbolic"),
                            Gtk.IconSize.MENU)
            app_icon.set_pixel_size(16)
            head.pack_start(app_icon, False, False, 0)
            name = Gtk.Label(label=app, xalign=0, ellipsize=3)
            name.get_style_context().add_class("small")
            name.get_style_context().add_class("dim")
            head.pack_start(name, True, True, 0)
            card.pack_start(head, False, False, 0)
            for item in items:
                body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4, margin_start=16, margin_end=8,
                               margin_top=6, margin_bottom=12)
                top = Gtk.Box(spacing=8)
                title = Gtk.Label(label=str(item.get("title", "")), xalign=0, wrap=True)
                title.get_style_context().add_class("row-title")
                top.pack_start(title, True, True, 0)
                stamp = time.localtime(float(item.get("time", 0) or 0))
                when = time.strftime("%H:%M", stamp) if stamp[:3] == today[:3] else time.strftime("%d/%m %H:%M", stamp)
                moment = Gtk.Label(label=when)
                moment.get_style_context().add_class("small")
                moment.get_style_context().add_class("dim")
                top.pack_start(moment, False, False, 0)
                close = Gtk.Button()
                close.get_style_context().add_class("flat")
                close.add(icon("window-close-symbolic", Gtk.IconSize.MENU))
                close.set_tooltip_text(_t("Supprimer cette notification"))
                close.connect("clicked", lambda *_, identity=item.get("id", 0):
                              self.notification_call("Dismiss", GLib.Variant("(u)", (int(identity),))))
                top.pack_start(close, False, False, 0)
                body.pack_start(top, False, False, 0)
                if item.get("body"):
                    text = Gtk.Label(label=str(item["body"]), xalign=0, wrap=True, selectable=True)
                    text.get_style_context().add_class("dim")
                    body.pack_start(text, False, False, 0)
                actions = item.get("actions", []) or []
                if len(actions) >= 2:
                    line = Gtk.Box(spacing=8, margin_top=6)
                    for index in range(0, len(actions)-1, 2):
                        line.pack_start(self.action(str(actions[index+1]), lambda identity=item.get("id", 0), action=actions[index]:
                            self.notification_call("Activate", GLib.Variant("(us)", (int(identity), str(action)))),
                            "accent" if index == 0 else None), False, False, 0)
                    body.pack_start(line, False, False, 0)
                card.pack_start(body, False, False, 0)

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


    # ── Personnalisation ─────────────────────────────────────────────────

    def current_wallpaper(self):
        """Fond actuel : xfconf (bureau), sinon la préférence, sinon Aurore."""
        if shutil.which("xfconf-query"):
            try:
                props = subprocess.run(["xfconf-query", "-c", "xfce4-desktop", "-l"], capture_output=True,
                                       text=True, timeout=3).stdout.split()
                for prop in props:
                    if prop.endswith("/last-image"):
                        value = subprocess.run(["xfconf-query", "-c", "xfce4-desktop", "-p", prop], capture_output=True,
                                               text=True, timeout=3).stdout.strip()
                        if value and Path(value).is_file(): return Path(value)
            except (OSError, subprocess.SubprocessError):
                pass
        for candidate in (appearance_state().get("wallpaper"), WALLPAPER,
                          Path(__file__).resolve().parent.parent / "assets/wallpapers/winlux-prisme-aurore.png"):
            if candidate and Path(candidate).is_file(): return Path(candidate)
        return None

    def current_start_icon(self):
        folder = HOME / ".local/share/winlux/start"
        for path in sorted(folder.glob("icon.*")) if folder.is_dir() else []:
            return path, False
        for candidate in (Path("/usr/share/winlux/native/start.svg"),
                          Path(__file__).resolve().parent.parent / "integration/native/start.svg"):
            if candidate.is_file(): return candidate, True
        return None, True

    def picture(self, path, width, height):
        image = Gtk.Image()
        try:
            pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(path), width, height, False)
            image.set_from_pixbuf(pixbuf)
        except (GLib.Error, TypeError):
            image.set_from_icon_name("image-x-generic-symbolic", Gtk.IconSize.DIALOG)
        return image

    def desktop_preview(self, wallpaper, dark, accent, width=320, height=180):
        """Aperçu du bureau : fond, fenêtre au thème choisi et barre des tâches."""
        area = Gtk.DrawingArea()
        area.set_size_request(width, height)
        pixbuf = None
        if wallpaper:
            try: pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(wallpaper), width, height, False)
            except GLib.Error: pixbuf = None
        def rgb(color): return tuple(v / 255 for v in _rgb(color))
        def rounded(cr, x, y, w, h, r):
            import math
            cr.new_sub_path()
            cr.arc(x + w - r, y + r, r, -math.pi / 2, 0); cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
            cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi); cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
            cr.close_path()
        def draw(_area, cr):
            rounded(cr, 0, 0, width, height, 8); cr.clip()
            if pixbuf is not None:
                Gdk.cairo_set_source_pixbuf(cr, pixbuf, 0, 0); cr.paint()
            else:
                cr.set_source_rgb(*rgb("#1c1640")); cr.paint()
            surface, text, line = ("#202020", "#ffffff", "#3a3a3a") if dark else ("#f9f9f9", "#1a1a1a", "#dddddd")
            # Fenêtre.
            rounded(cr, width * 0.18, height * 0.16, width * 0.52, height * 0.52, 5)
            cr.set_source_rgb(*rgb(surface)); cr.fill()
            cr.set_source_rgb(*rgb(accent)); rounded(cr, width * 0.22, height * 0.42, width * 0.14, height * 0.08, 3); cr.fill()
            cr.set_source_rgb(*rgb(line))
            for k in range(3):
                rounded(cr, width * 0.22, height * (0.24 + k * 0.05), width * (0.38 - k * 0.08), height * 0.025, 1); cr.fill()
            cr.set_source_rgb(*rgb(text)); rounded(cr, width * 0.22, height * 0.20, width * 0.12, height * 0.025, 1); cr.fill()
            # Barre des tâches centrée.
            cr.set_source_rgba(*rgb(surface), 0.92); cr.rectangle(0, height - 18, width, 18); cr.fill()
            for k in range(5):
                cx = width / 2 - 40 + k * 18
                cr.set_source_rgb(*rgb(accent if k == 0 else line)); rounded(cr, cx, height - 14, 10, 10, 2); cr.fill()
        area.connect("draw", draw)
        return area

    def appearance(self):
        state = appearance_state()
        theme = state.get("theme") if state.get("theme") in ("dark", "light") else ("dark" if dark_theme() else "light")
        accent = state.get("accent") if re.fullmatch(r"#[0-9a-fA-F]{6}", str(state.get("accent", ""))) else DEFAULT_ACCENT
        accent = accent.lower()
        wallpaper = self.current_wallpaper()
        hero = self.card()
        top = Gtk.Box(spacing=24, margin=16)
        hero.pack_start(top, False, False, 0)
        top.pack_start(self.desktop_preview(wallpaper, theme == "dark", accent), False, False, 0)
        side = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, valign=Gtk.Align.CENTER)
        top.pack_start(side, True, True, 0)
        head = Gtk.Label(label=_t("Choisir un thème"), xalign=0)
        head.get_style_context().add_class("row-title")
        side.pack_start(head, False, False, 0)
        hint = Gtk.Label(label=_t("Le thème change les fenêtres, le menu Démarrer et la barre des tâches."), xalign=0, wrap=True)
        hint.get_style_context().add_class("dim"); hint.get_style_context().add_class("small")
        side.pack_start(hint, False, False, 0)
        choices = Gtk.Box(spacing=12)
        for value, title in (("light", _t("Clair")), ("dark", _t("Sombre"))):
            button = Gtk.Button()
            button.get_style_context().add_class("theme-card")
            if value == theme: button.get_style_context().add_class("chosen")
            inner = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
            inner.pack_start(self.desktop_preview(None if False else wallpaper, value == "dark", accent, 120, 68), False, False, 0)
            inner.pack_start(Gtk.Label(label=title), False, False, 0)
            button.add(inner)
            button.connect("clicked", lambda *_, v=value: self.command(["winlux-personalize", "theme", v]))
            choices.pack_start(button, False, False, 0)
        side.pack_start(choices, False, False, 0)
        self.section(_t("Couleur d’accentuation"))
        colors = self.card()
        swatches = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=10, column_spacing=8,
                               row_spacing=8, margin=16, homogeneous=True)
        for color in ACCENTS:
            swatch = Gtk.Button()
            swatch.get_style_context().add_class("swatch")
            if color == accent: swatch.get_style_context().add_class("chosen")
            provider = Gtk.CssProvider()
            provider.load_from_data(f"button {{ background: {color}; }} button:hover {{ background: {_mix(color, '#ffffff', 0.15)}; }}".encode())
            swatch.get_style_context().add_provider(provider, Gtk.STYLE_PROVIDER_PRIORITY_USER + 2)
            swatch.set_tooltip_text(_t("Winlux (par défaut)") if color == DEFAULT_ACCENT else color.upper())
            if color == accent:
                mark = icon("object-select-symbolic", Gtk.IconSize.MENU)
                mark_css = Gtk.CssProvider()
                mark_css.load_from_data(f"image {{ color: {_text_on(color)}; }}".encode())
                mark.get_style_context().add_provider(mark_css, Gtk.STYLE_PROVIDER_PRIORITY_USER + 2)
                swatch.add(mark)
            swatch.connect("clicked", lambda *_, c=color: self.command(["winlux-personalize", "accent", c]))
            swatches.add(swatch)
        colors.pack_start(swatches, False, False, 0)
        custom = Gtk.ColorButton(title=_t("Couleur d’accent"))
        rgba = Gdk.RGBA(); rgba.parse(accent); custom.set_rgba(rgba)
        custom.connect("color-set", lambda widget: self.command(["winlux-personalize","accent",
            "#%02x%02x%02x" % tuple(round(v*255) for v in (widget.get_rgba().red,widget.get_rgba().green,widget.get_rgba().blue))]))
        self.row(colors, "applications-graphics-symbolic", _t("Couleur personnalisée"),
                 _t("Toute couleur : les textes restent lisibles, Winlux ajuste les contrastes."), custom)
        self.section(_t("Bureau et barre des tâches"))
        rows = self.card()
        name = wallpaper.name if wallpaper else _t("Aucun")
        thumb = self.picture(wallpaper, 64, 36) if wallpaper else None
        self.row(rows, "preferences-desktop-wallpaper-symbolic", _t("Arrière-plan"), name,
                 [w for w in (thumb, self.action(_t("Parcourir les images"), self.choose_wallpaper)) if w])
        start, default_icon = self.current_start_icon()
        start_image = self.picture(start, 24, 24) if start else None
        buttons = [w for w in (start_image, self.action(_t("Parcourir"), self.choose_start_icon)) if w]
        if not default_icon:
            buttons.append(self.action(_t("Par défaut"), lambda: self.command(["winlux-personalize", "start-icon", "default"]), "flat"))
        self.row(rows, ("view-app-grid-symbolic", "view-grid-symbolic"), _t("Icône du menu Démarrer"),
                 _t("Icône Winlux d’origine") if default_icon else _t("Image personnalisée"), buttons)
        sizes = Gtk.ComboBoxText()
        for size, label in ((32, _t("Très petite")), (40, _t("Petite")), (48, _t("Moyenne")), (56, _t("Grande")), (64, _t("Très grande"))):
            sizes.append(str(size), f"{label} · {size} px")
        sizes.set_active_id(str(state.get("panel-size", "48")) if str(state.get("panel-size", "48")) in ("32","40","48","56","64") else "48")
        sizes.connect("changed", lambda widget: self.command(["winlux-personalize", "panel-size", widget.get_active_id()]))
        self.row(rows, "view-continuous-symbolic", _t("Taille de la barre des tâches"),
                 _t("Hauteur de la barre et de ses icônes."), sizes)
        if shutil.which("xfce4-panel"):
            self.row(rows, "preferences-other-symbolic", _t("Options avancées de la barre des tâches"),
                     _t("Éléments, comportement et position de la barre."),
                     activate=lambda: self.command(["xfce4-panel", "--preferences"]))
        self.row(rows, ("preferences-desktop-appearance-symbolic", "emblem-system-symbolic"), _t("Plus de personnalisation"),
                 _t("Effets visuels, écran de veille, transparence…"), activate=lambda: self.open_settings("21"))
        reset = self.card()
        def reset_appearance():
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
                                       text=_t("Rétablir l’apparence Winlux par défaut ?"),
                                       secondary_text=_t("Le thème, l’accent, le fond, l’icône Démarrer et la taille de la barre seront réinitialisés."))
            result = dialog.run(); dialog.destroy()
            if result == Gtk.ResponseType.OK: self.command(["winlux-personalize", "reset", "default"])
        self.row(reset, "edit-undo-symbolic", _t("Rétablir l’apparence par défaut"),
                 _t("Thème sombre, violet Winlux, fond Aurore et barre moyenne."),
                 self.action(_t("Rétablir"), reset_appearance))

    def reload_appearance(self):
        self.apply_theme()
        self.rebuild(self.appearance)
        return False

    def choose_image(self, title):
        dialog = Gtk.FileChooserDialog(title=title, transient_for=self, action=Gtk.FileChooserAction.OPEN)
        dialog.add_buttons(_t("Annuler"), Gtk.ResponseType.CANCEL, _t("Choisir"), Gtk.ResponseType.OK)
        images = Gtk.FileFilter(); images.set_name(_t("Images")); images.add_pixbuf_formats()
        dialog.add_filter(images)
        pictures = GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_PICTURES)
        if pictures and Path(pictures).is_dir(): dialog.set_current_folder(pictures)
        answer = dialog.run(); path = dialog.get_filename() if answer == Gtk.ResponseType.OK else None; dialog.destroy()
        return path

    def choose_wallpaper(self):
        path = self.choose_image(_t("Choisir un fond d’écran"))
        if path: self.command(["winlux-personalize", "wallpaper", path])

    def choose_start_icon(self):
        path = self.choose_image(_t("Choisir l’icône du menu Démarrer"))
        if path: self.command(["winlux-personalize", "start-icon", path])

    # ── Fichiers et périphériques ────────────────────────────────────────

    def explorer(self, target):
        """Dossier ouvert dans l'Explorateur Winlux (le seul des deux sessions)."""
        if shutil.which("winlux-shell-app"):
            self.launch(["winlux-shell-app", "explorer", str(target)])
        elif shutil.which("Thunar"):
            self.launch(["Thunar", str(target)])
        else:
            self.notice.set_text(_t("Explorateur introuvable."))

    def drives(self):
        """Systèmes de fichiers montés visibles (disque système, données, clés USB)."""
        try:
            data = json.loads(subprocess.run(["lsblk", "-J", "-b", "-o", "NAME,PATH,LABEL,MODEL,SIZE,FSSIZE,FSAVAIL,MOUNTPOINT,RM,HOTPLUG,TRAN,TYPE"],
                                             capture_output=True, text=True, timeout=5).stdout or "{}")
        except (OSError, subprocess.SubprocessError, ValueError):
            return []
        found = []
        def walk(devices, parent=None):
            for device in devices or []:
                mount = device.get("mountpoint")
                visible = mount == "/" or str(mount or "").startswith(("/home", "/media/", "/mnt/", "/run/media/"))
                # Plus de 100 Mo : pas les petits montages techniques (images, overlays).
                if mount and visible and int(device.get("fssize") or 0) >= 100_000_000:
                    removable = bool(device.get("rm") or device.get("hotplug") or (parent or {}).get("rm")
                                     or (parent or {}).get("hotplug")) or device.get("tran") == "usb" or (parent or {}).get("tran") == "usb"
                    found.append({"mount": mount, "label": device.get("label") or (parent or {}).get("model") or device.get("model") or "",
                                  "size": int(device.get("fssize") or 0), "free": int(device.get("fsavail") or 0),
                                  "removable": removable, "path": device.get("path")})
                walk(device.get("children"), device)
        walk(data.get("blockdevices"))
        return found

    def files(self):
        def xdg(kind, fallback):
            special = GLib.get_user_special_dir(kind)
            return Path(special) if special else HOME / fallback
        places = [(_t("Dossier personnel"), HOME, "user-home"),
                  (_t("Bureau"), xdg(GLib.UserDirectory.DIRECTORY_DESKTOP, "Bureau"), "user-desktop"),
                  (_t("Documents"), xdg(GLib.UserDirectory.DIRECTORY_DOCUMENTS, "Documents"), "folder-documents"),
                  (_t("Téléchargements"), xdg(GLib.UserDirectory.DIRECTORY_DOWNLOAD, "Téléchargements"), "folder-download"),
                  (_t("Images"), xdg(GLib.UserDirectory.DIRECTORY_PICTURES, "Images"), "folder-pictures"),
                  (_t("Musique"), xdg(GLib.UserDirectory.DIRECTORY_MUSIC, "Musique"), "folder-music"),
                  (_t("Vidéos"), xdg(GLib.UserDirectory.DIRECTORY_VIDEOS, "Vidéos"), "folder-videos"),
                  (_t("Corbeille"), "C:\\.local\\share\\Trash\\files", "user-trash")]
        open_explorer = self.action(_t("Ouvrir l’Explorateur"), lambda: self.explorer(HOME), "accent")
        self.header_end.pack_start(open_explorer, False, False, 0)
        self.section(_t("Dossiers"))
        grid = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=4, min_children_per_line=2,
                           column_spacing=10, row_spacing=10, homogeneous=True)
        for label, path, icon_name in places:
            if isinstance(path, Path) and path != HOME and not path.is_dir() and icon_name != "user-trash":
                continue
            button = Gtk.Button()
            button.get_style_context().add_class("card")
            inner = Gtk.Box(spacing=12, margin=10)
            image = icon((icon_name, "folder"), Gtk.IconSize.DND)
            image.set_pixel_size(32)
            inner.pack_start(image, False, False, 0)
            name = Gtk.Label(label=label, xalign=0, ellipsize=3)
            inner.pack_start(name, True, True, 0)
            button.add(inner)
            button.set_tooltip_text(str(path) if isinstance(path, Path) else label)
            button.connect("clicked", lambda *_, target=path: self.explorer(target))
            grid.add(button)
        self.content.pack_start(grid, False, False, 0)
        self.section(_t("Lecteurs et périphériques"))
        drives = self.card()
        found = self.drives()
        for drive in found:
            title = _t("Disque système") if drive["mount"] == "/" else drive["label"] or Path(drive["mount"]).name
            used = max(0, drive["size"] - drive["free"])
            bar = Gtk.LevelBar(min_value=0, max_value=max(1, drive["size"]), value=used)
            bar.set_size_request(180, -1)
            subtitle = _t("{free} libres sur {size}").format(free=size_text(drive["free"]), size=size_text(drive["size"]))
            end = [bar, self.action(_t("Ouvrir"), lambda target=drive["mount"]: self.explorer(target))]
            if drive["removable"] and shutil.which("udisksctl") and drive["path"]:
                end.append(self.action(_t("Éjecter"), lambda device=drive["path"]: self.command(
                    ["sh", "-c", 'udisksctl unmount -b "$1" && udisksctl power-off -b "$1" 2>/dev/null; true', "sh", device])))
            self.row(drives, "drive-removable-media-symbolic" if drive["removable"] else "drive-harddisk-symbolic",
                     title, subtitle, end)
        if not found:
            self.row(drives, "drive-harddisk-symbolic", _t("Aucun lecteur détecté"), _t("Branchez une clé USB ou un disque : il apparaîtra ici."))
        self.section(_t("Plus d’options"))
        more = self.card()
        self.row(more, "folder-remote-symbolic", _t("Lecteurs réseau"), _t("Partages Windows, FTP et SFTP dans l’Explorateur."),
                 activate=lambda: self.open_settings("20"))
        if shutil.which("gnome-disks"):
            self.row(more, "drive-multidisk-symbolic", _t("Gestion des disques"), _t("Partitions, formatage et santé des disques."),
                     activate=lambda: self.command(["gnome-disks"]))
        bookmarks = [line.split(" ", 1) for line in read(HOME / ".config/gtk-3.0/bookmarks").splitlines() if line.startswith("file://")]
        names = ", ".join((parts[1] if len(parts) > 1 else Path(GLib.filename_from_uri(parts[0])[0]).name) for parts in bookmarks[:3]) \
            if bookmarks else _t("Aucun dossier épinglé")
        self.row(more, "user-bookmarks-symbolic", _t("Dossiers épinglés des fenêtres Ouvrir et Enregistrer"),
                 names, self.action(_t("Ajouter un dossier"), self.add_bookmark))
        self.row(more, "document-open-symbolic", _t("Aperçu d’un fichier"), _t("Voir une image ou un texte sans l’ouvrir."),
                 self.action(_t("Choisir un fichier"), self.show_preview))

    # ── Secours et restauration ──────────────────────────────────────────

    def recovery(self):
        banner = Gtk.Box(spacing=14)
        banner.get_style_context().add_class("banner")
        info = icon("dialog-information-symbolic", Gtk.IconSize.LARGE_TOOLBAR)
        info.set_valign(Gtk.Align.START)
        banner.pack_start(info, False, False, 0)
        text = Gtk.Label(label=_t("Si votre PC ne fonctionne pas correctement, ces options peuvent vous aider. Vos documents ne sont pas une sauvegarde système : gardez-en aussi une copie sur un autre support."),
                         xalign=0, wrap=True)
        banner.pack_start(text, True, True, 0)
        self.content.pack_start(banner, False, False, 0)
        if Path("/usr/share/winlux/update-backend").is_file() and read("/usr/share/winlux/update-backend").strip() == "apt":
            available = Path("/var/cache/winlux/previous.deb").is_file()
        else:
            name = read("/var/lib/winlux/rollback-current").strip()
            available = bool(re.fullmatch(r"update-backup\.[A-Za-z0-9._+-]+", name)) and (Path("/var/lib/winlux") / name / "restore.list").is_file()
        self.section(_t("Options de récupération"))
        options = self.card()
        state = Gtk.Label(label=_t("Disponible") if available else _t("Aucune sauvegarde"))
        state.get_style_context().add_class("ok-badge" if available else "warn-badge")
        self.row(options, "edit-undo-symbolic", _t("Revenir à la version précédente"),
                 _t("Sauvegarde précédente détectée.") if available else _t("Aucune sauvegarde précédente détectée."),
                 [state, self.action(_t("Ouvrir les mises à jour"), lambda: self.open_settings("7"))])
        self.row(options, "view-refresh-symbolic", _t("Réinitialiser ce PC"),
                 _t("Réinstaller Winlux en gardant ou en supprimant vos fichiers personnels."),
                 self.action(_t("Réinitialiser le PC"), lambda: self.open_settings("31"), "accent"))
        self.row(options, "system-reboot-symbolic", _t("Démarrage avancé"),
                 _t("Redémarrer dans le menu de secours pour réparer le démarrage, même si le bureau ne s’ouvre plus."),
                 self.action(_t("Options de démarrage"), lambda: self.open_settings("31")))
        self.row(options, "drive-removable-media-symbolic", _t("Clé de secours"),
                 _t("Préparer une clé USB qui démarre Winlux et répare ce PC."),
                 self.action(_t("Créer une clé"), lambda: self.open_settings("31")))
        self.section(_t("Aide supplémentaire"))
        more = self.card()
        self.row(more, "document-save-symbolic", _t("Sauvegarde de vos fichiers"),
                 _t("Copier vos documents sur un autre disque, régulièrement."), activate=lambda: self.open_settings("38"))
        self.row(more, "dialog-warning-symbolic", _t("Signaler un problème"),
                 _t("Envoyer un rapport aux développeurs de Winlux."), activate=lambda: self.open_settings("25"))
        self.row(more, "help-browser-symbolic", _t("Guide d’accueil"),
                 _t("Si le bureau ne démarre plus : entrée « Winlux secours » au démarrage, ou une clé d’installation Winlux."),
                 activate=lambda: self.launch(["winlux-welcome"]))

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

    def show_preview(self):
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
    # Notifications : panneau volant façon Windows 11 (avec le calendrier),
    # aussi pour les raccourcis Win+N déjà enregistrés vers cette page.
    if page == "notifications" and shutil.which("winlux-popup") and not os.environ.get("WINLUX_TOOLS_WINDOW"):
        os.execvp("winlux-popup", ["winlux-popup", "notifications"])
    # Réglages rapides (Win+A) : panneau à tuiles et curseurs façon Windows 11 ;
    # la liste des réseaux Wi-Fi ci-dessous s'ouvre depuis la flèche de la tuile.
    if page == "quick" and shutil.which("winlux-popup") and not os.environ.get("WINLUX_TOOLS_WINDOW"):
        os.execvp("winlux-popup", ["winlux-popup", "quick"])
    Tools(page)
    Gtk.main()

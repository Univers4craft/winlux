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
from gi.repository import Gdk, GdkPixbuf, Gio, GLib, Gtk


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
    # Appareils mobiles (Téléphone connecté)
    'Téléphone connecté': ('Connected phone', 'Teléfono conectado', 'Verbundenes Telefon'),
    'Notifications, SMS et fichiers de votre téléphone Android sur ce PC, avec l’application KDE Connect.': ('Your Android phone’s notifications, texts and files on this PC, with the KDE Connect app.', 'Notificaciones, SMS y archivos de su teléfono Android en este PC, con la aplicación KDE Connect.', 'Benachrichtigungen, SMS und Dateien Ihres Android-Telefons auf diesem PC, mit der App KDE Connect.'),
    'Autoriser la connexion de mes téléphones': ('Allow my phones to connect', 'Permitir que mis teléfonos se conecten', 'Verbindung meiner Telefone zulassen'),
    'Ports ouverts sur les réseaux privés seulement.': ('Ports opened on private networks only.', 'Puertos abiertos solo en redes privadas.', 'Ports nur in privaten Netzwerken geöffnet.'),
    'Ouvrir Téléphone connecté': ('Open Connected phone', 'Abrir Teléfono conectado', 'Verbundenes Telefon öffnen'),
    'Hors de portée': ('Not reachable', 'Fuera de alcance', 'Nicht erreichbar'),
    'Batterie {level} %': ('Battery {level}%', 'Batería {level} %', 'Akku {level} %'),
    'Notifications du téléphone': ('Phone notifications', 'Notificaciones del teléfono', 'Telefonbenachrichtigungen'),
    'Affichées sur ce PC, avec réponse rapide.': ('Shown on this PC, with quick reply.', 'Se muestran en este PC, con respuesta rápida.', 'Auf diesem PC angezeigt, mit Schnellantwort.'),
    'Messages (SMS) et contacts': ('Text messages (SMS) and contacts', 'Mensajes (SMS) y contactos', 'Nachrichten (SMS) und Kontakte'),
    'Lire et envoyer des SMS depuis ce PC.': ('Read and send texts from this PC.', 'Leer y enviar SMS desde este PC.', 'SMS auf diesem PC lesen und senden.'),
    'Recevoir des fichiers': ('Receive files', 'Recibir archivos', 'Dateien empfangen'),
    'Enregistrés dans Téléchargements.': ('Saved to Downloads.', 'Se guardan en Descargas.', 'In Downloads gespeichert.'),
    'Faire sonner': ('Ring', 'Hacer sonar', 'Klingeln lassen'),
    'Dissocier': ('Unpair', 'Desvincular', 'Entkoppeln'),
    "Luminosité et couleur": ("Brightness & color", "Brillo y color", "Helligkeit und Farbe"),
    "Luminosité": ("Brightness", "Brillo", "Helligkeit"),
    "Luminosité de l’écran, en pourcentage": ("Screen brightness, as a percentage", "Brillo de la pantalla, en porcentaje",
                                              "Bildschirmhelligkeit in Prozent"),
    "Écran réglé par son câble (DDC/CI).": ("Monitor adjusted through its cable (DDC/CI).",
                                            "Pantalla ajustada por su cable (DDC/CI).", "Bildschirm über das Kabel geregelt (DDC/CI)."),
    "Cet écran n’a pas de rétroéclairage pilotable : Winlux assombrit l’image.": (
        "This screen has no adjustable backlight: Winlux dims the picture instead.",
        "Esta pantalla no tiene retroiluminación ajustable: Winlux oscurece la imagen.",
        "Dieser Bildschirm hat keine regelbare Hintergrundbeleuchtung: Winlux dunkelt das Bild ab."),
    "Disposition": ("Arrangement", "Disposición", "Anordnung"),
    "Écran": ("Display", "Pantalla", "Bildschirm"),
    "Plusieurs écrans": ("Multiple displays", "Varias pantallas", "Mehrere Bildschirme"),
    "Étendre ces écrans": ("Extend these displays", "Extender estas pantallas", "Diese Anzeigen erweitern"),
    "Dupliquer ces écrans": ("Duplicate these displays", "Duplicar estas pantallas", "Diese Anzeigen duplizieren"),
    "Afficher seulement sur {name}": ("Show only on {name}", "Mostrar solo en {name}", "Nur auf {name} anzeigen"),
    "Écran principal": ("Main display", "Pantalla principal", "Hauptanzeige"),
    "écran intégré": ("built-in display", "pantalla integrada", "integrierte Anzeige"),
    "Définition de l’écran": ("Display resolution", "Resolución de pantalla", "Bildschirmauflösung"),
    "Fréquence d’actualisation": ("Refresh rate", "Frecuencia de actualización", "Bildwiederholfrequenz"),
    "Plus elle est élevée, plus les mouvements sont fluides (jeux, défilement).": ("The higher it is, the smoother motion looks (games, scrolling).", "Cuanto más alta, más fluidos son los movimientos (juegos, desplazamiento).", "Je höher, desto flüssiger wirken Bewegungen (Spiele, Bildlauf)."),
    "La nouvelle échelle s’applique complètement à la prochaine ouverture de session.": ("The new scale fully applies at the next sign-in.", "La nueva escala se aplica por completo en el próximo inicio de sesión.", "Die neue Skalierung gilt vollständig ab der nächsten Anmeldung."),
    # Clavier tactile et souris avancée
    "Icône du clavier tactile": ("Touch keyboard icon", "Icono del teclado táctil", "Symbol der Bildschirmtastatur"),
    "Dans la barre des tâches. « Sans clavier physique » la montre quand aucun clavier n’est branché.": ("On the taskbar. “Without a physical keyboard” shows it when no keyboard is connected.", "En la barra de tareas. «Sin teclado físico» lo muestra cuando no hay ningún teclado conectado.", "In der Taskleiste. „Ohne physische Tastatur“ zeigt es, wenn keine Tastatur angeschlossen ist."),
    "Sans clavier physique": ("Without a physical keyboard", "Sin teclado físico", "Ohne physische Tastatur"),
    "Toujours": ("Always", "Siempre", "Immer"),
    "Ouvrir le clavier tactile": ("Open the touch keyboard", "Abrir el teclado táctil", "Bildschirmtastatur öffnen"),
    "Bouton principal de la souris": ("Primary mouse button", "Botón principal del mouse", "Primäre Maustaste"),
    "Gauche": ("Left", "Izquierdo", "Links"),
    "Droit": ("Right", "Derecho", "Rechts"),
    "Améliorer la précision du pointeur": ("Enhance pointer precision", "Mejorar la precisión del puntero", "Zeigerbeschleunigung verbessern"),
    "Le pointeur va plus loin quand la souris bouge vite.": ("The pointer travels further when the mouse moves fast.", "El puntero llega más lejos cuando el mouse se mueve rápido.", "Der Zeiger bewegt sich weiter, wenn die Maus schnell bewegt wird."),
    "Défilement": ("Scrolling", "Desplazamiento", "Bildlauf"),
    "Lignes à faire défiler à chaque cran de la molette": ("Lines to scroll at a time", "Líneas que se desplazan cada vez", "Zeilen pro Bildlauf"),
    "Programmes Windows ; 3 par défaut.": ("Windows programs; 3 by default.", "Programas de Windows; 3 de forma predeterminada.", "Windows-Programme; standardmäßig 3."),
    "Sens de défilement inversé": ("Reverse scrolling direction", "Invertir la dirección de desplazamiento", "Bildlaufrichtung umkehren"),
    "Tourner la molette vers le bas fait monter le contenu.": ("Turning the wheel down moves the content up.", "Girar la rueda hacia abajo sube el contenido.", "Das Mausrad nach unten drehen bewegt den Inhalt nach oben."),
    "Double-clic": ("Double-click", "Doble clic", "Doppelklick"),
    "Vitesse du double-clic": ("Double-click speed", "Velocidad del doble clic", "Doppelklickgeschwindigkeit"),
    "Plus lente à gauche, plus rapide à droite.": ("Slower to the left, faster to the right.", "Más lenta a la izquierda, más rápida a la derecha.", "Links langsamer, rechts schneller."),
    "Double-cliquez sur le dossier pour tester.": ("Double-click the folder to test.", "Haga doble clic en la carpeta para probar.", "Doppelklicken Sie zum Testen auf den Ordner."),
    "Zone de test": ("Test area", "Área de prueba", "Testbereich"),
    "Double-cliquez sur le dossier : il s’ouvre ou se ferme si la vitesse vous convient.": ("Double-click the folder: it opens or closes if the speed suits you.", "Haga doble clic en la carpeta: se abre o se cierra si la velocidad le conviene.", "Doppelklicken Sie auf den Ordner: Er öffnet oder schließt sich, wenn die Geschwindigkeit passt."),
    "Pointeur": ("Pointer", "Puntero", "Zeiger"),
    "Couleur du pointeur": ("Pointer colour", "Color del puntero", "Zeigerfarbe"),
    "Thème du système": ("System theme", "Tema del sistema", "Systemdesign"),
    "Blanc": ("White", "Blanco", "Weiß"),
    "Noir": ("Black", "Negro", "Schwarz"),
    "Vert vif": ("Lime", "Verde lima", "Hellgrün"),
    "Traînée du pointeur": ("Pointer trails", "Rastro del puntero", "Mausspur"),
    "Des copies du pointeur le suivent quand il bouge : il se repère mieux sur un grand écran.": ("Copies of the pointer follow it as it moves, so it is easier to find on a large screen.", "Copias del puntero lo siguen al moverse: se localiza mejor en una pantalla grande.", "Kopien des Zeigers folgen ihm beim Bewegen, so findet man ihn auf großen Bildschirmen leichter."),
    "Aucune": ("None", "Ninguno", "Keine"),
    "Courte": ("Short", "Corto", "Kurz"),
    "Longue": ("Long", "Largo", "Lang"),
    'Comptes': ('Accounts', 'Cuentas', 'Konten'),
    'Votre compte': ('Your account', 'Su cuenta', 'Ihr Konto'),
    'Autres utilisateurs': ('Other users', 'Otros usuarios', 'Andere Benutzer'),
    'Administrateur': ('Administrator', 'Administrador', 'Administrator'),
    'Standard': ('Standard user', 'Estándar', 'Standardbenutzer'),
    'Comptes locaux sur ce PC. Chacun a son bureau, ses fichiers et ses réglages ; « Changer d’utilisateur » dans le menu Démarrer passe de l’un à l’autre sans rien fermer.': ('Local accounts on this PC. Each one has its own desktop, files and settings; “Switch user” in the Start menu moves between them without closing anything.', 'Cuentas locales en este equipo. Cada una tiene su escritorio, sus archivos y su configuración; «Cambiar de usuario» en el menú Inicio pasa de una a otra sin cerrar nada.', 'Lokale Konten auf diesem PC. Jedes hat seinen eigenen Desktop, seine Dateien und Einstellungen; „Benutzer wechseln“ im Startmenü wechselt zwischen ihnen, ohne etwas zu schließen.'),
    'Ajouter un compte': ('Add account', 'Agregar cuenta', 'Konto hinzufügen'),
    'Ajouter un autre utilisateur à ce PC': ('Add someone else to this PC', 'Agregar otro usuario a este equipo', 'Andere Person zu diesem PC hinzufügen'),
    'Changer le mot de passe': ('Change password', 'Cambiar la contraseña', 'Kennwort ändern'),
    'Changer le type de compte': ('Change account type', 'Cambiar el tipo de cuenta', 'Kontotyp ändern'),
    'Nom du compte': ('User name', 'Nombre de usuario', 'Benutzername'),
    'Nom complet (facultatif)': ('Full name (optional)', 'Nombre completo (opcional)', 'Vollständiger Name (optional)'),
    'Confirmer le mot de passe': ('Confirm password', 'Confirmar la contraseña', 'Kennwort bestätigen'),
    'Administrateur de ce PC': ('Administrator of this PC', 'Administrador de este equipo', 'Administrator dieses PCs'),
    'Un administrateur installe des logiciels et change les réglages du système ; un compte standard en a besoin pour ces actions.': ('An administrator installs software and changes system settings; a standard account needs one for those actions.', 'Un administrador instala software y cambia la configuración del sistema; una cuenta estándar lo necesita para esas acciones.', 'Ein Administrator installiert Software und ändert Systemeinstellungen; ein Standardkonto braucht ihn für diese Aktionen.'),
    'Laissez vide pour un compte sans mot de passe.': ('Leave blank for an account without a password.', 'Déjelo en blanco para una cuenta sin contraseña.', 'Leer lassen für ein Konto ohne Kennwort.'),
    'Lettres minuscules, chiffres, - et _ (32 au plus), en commençant par une lettre.': ('Lowercase letters, digits, - and _ (32 at most), starting with a letter.', 'Letras minúsculas, cifras, - y _ (32 como máximo), empezando por una letra.', 'Kleinbuchstaben, Ziffern, - und _ (höchstens 32), beginnend mit einem Buchstaben.'),
    'Les deux mots de passe sont différents.': ('The two passwords don’t match.', 'Las dos contraseñas no coinciden.', 'Die beiden Kennwörter stimmen nicht überein.'),
    'Ce nom de compte existe déjà.': ('This user name already exists.', 'Este nombre de usuario ya existe.', 'Dieser Benutzername existiert bereits.'),
    'Supprimer le compte {name} ?': ('Remove the account {name}?', '¿Quitar la cuenta {name}?', 'Konto {name} entfernen?'),
    'Effacer aussi ses fichiers (Bureau, Documents, Images…)': ('Also delete their files (Desktop, Documents, Pictures…)', 'Borrar también sus archivos (Escritorio, Documentos, Imágenes…)', 'Auch die Dateien löschen (Desktop, Dokumente, Bilder…)'),
    'Il faut garder au moins un compte administrateur.': ('At least one administrator account must remain.', 'Debe quedar al menos una cuenta de administrador.', 'Mindestens ein Administratorkonto muss bleiben.'),
    'Session ouverte': ('Signed in', 'Sesión iniciada', 'Angemeldet'),
    'Nouveau mot de passe pour {name}': ('New password for {name}', 'Nueva contraseña para {name}', 'Neues Kennwort für {name}'),
    'Annuler': ('Cancel', 'Cancelar', 'Abbrechen'),
    'Valider': ('OK', 'Aceptar', 'OK'),
    'Aucun autre compte sur ce PC.': ('No other account on this PC.', 'No hay otras cuentas en este equipo.', 'Keine anderen Konten auf diesem PC.'),
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
    'Réseaux Wi-Fi…': ('Wi-Fi networks…', 'Redes Wi-Fi…', 'WLAN-Netzwerke…'),
    'Choisir un réseau, entrer sa clé, WPS, réseaux cachés et mode avion.': ('Choose a network, enter its key, WPS, hidden networks and airplane mode.', 'Elegir una red, escribir su clave, WPS, redes ocultas y modo avión.', 'Netzwerk wählen, Schlüssel eingeben, WPS, verborgene Netzwerke und Flugzeugmodus.'),
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
    "Icônes du bureau": ("Desktop icons", "Iconos del escritorio", "Desktopsymbole"),
    "Corbeille": ("Recycle Bin", "Papelera de reciclaje", "Papierkorb"),
    "Ce PC": ("This PC", "Este equipo", "Dieser PC"),
    "Explorateur": ("File Explorer", "Explorador de archivos", "Explorer"),
    "Documents": ("Documents", "Documentos", "Dokumente"),
    "Barre des tâches": ("Taskbar", "Barra de tareas", "Taskleiste"),
    "Ce PC (système de fichiers)": ("This PC (file system)", "Este equipo (sistema de archivos)", "Dieser PC (Dateisystem)"),
    "Dossier de l’utilisateur": ("User's files", "Archivos del usuario", "Benutzerdateien"),
    "Lecteurs amovibles et réseau": ("Removable and network drives", "Unidades extraíbles y de red", "Wechsel- und Netzlaufwerke"),
    "Réorganiser les icônes": ("Arrange icons", "Organizar iconos", "Symbole anordnen"),
    "Les raccourcis ajoutés se retirent d’un clic droit. Un clic droit sur une application du menu Démarrer crée aussi son raccourci.": (
        "Added shortcuts are removed with a right-click. Right-clicking an app in the Start menu also creates its shortcut.",
        "Los accesos directos añadidos se quitan con un clic derecho. Un clic derecho en una aplicación del menú Inicio también crea su acceso directo.",
        "Hinzugefügte Verknüpfungen werden per Rechtsklick entfernt. Ein Rechtsklick auf eine App im Startmenü erstellt ebenfalls ihre Verknüpfung."),
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
    "Matériel et pilotes": ("Hardware and drivers", "Hardware y controladores", "Hardware und Treiber"),
    "Aucun haut-parleur, casque ni micro n’est reconnu sur ce PC. Si l’ordinateur en a, son pilote ou son firmware audio manque : la page Matériel et pilotes l’installe.": (
        "No speakers, headphones or microphone are recognized on this PC. If the computer has them, its audio driver or firmware is missing: the Hardware and drivers page installs it.",
        "No se reconoce ningún altavoz, auricular ni micrófono en este PC. Si el equipo los tiene, falta su controlador o firmware de audio: la página Hardware y controladores lo instala.",
        "Auf diesem PC werden keine Lautsprecher, Kopfhörer oder Mikrofone erkannt. Wenn der Computer welche hat, fehlt der Audiotreiber oder die Firmware: Die Seite Hardware und Treiber installiert sie."),
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
    'Météo en ligne': ('Online weather', 'Tiempo en línea', 'Onlinewetter'),
    'Désactivée par défaut. Activée, le service wttr.in reçoit votre adresse IP et le lieu.': ('Off by default. When on, the wttr.in service receives your IP address and the place.', 'Desactivado de forma predeterminada. Activado, el servicio wttr.in recibe su dirección IP y el lugar.', 'Standardmäßig aus. Eingeschaltet erhält der Dienst wttr.in Ihre IP-Adresse und den Ort.'),
    'Unité de température': ('Temperature unit', 'Unidad de temperatura', 'Temperatureinheit'),
    'Celsius (°C)': ('Celsius (°C)', 'Celsius (°C)', 'Celsius (°C)'),
    'Fahrenheit (°F)': ('Fahrenheit (°F)', 'Fahrenheit (°F)', 'Fahrenheit (°F)'),
    'Dossier des photos': ('Photos folder', 'Carpeta de fotos', 'Fotoordner'),
    'Choisir un dossier…': ('Choose a folder…', 'Elegir una carpeta…', 'Ordner auswählen…'),
    'Images du compte': ('Account pictures', 'Imágenes de la cuenta', 'Bilder des Kontos'),
    'Ouvrir les widgets': ('Open widgets', 'Abrir widgets', 'Widgets öffnen'),
    'Rétablir les widgets par défaut': ('Restore default widgets', 'Restablecer los widgets predeterminados', 'Standard-Widgets wiederherstellen'),
    # Assistant IA (#286) : Claude, ChatGPT, IA locale
    'Un bouton dans la barre des tâches, et Windows+C, pour poser une question sur Winlux. Activez un ou plusieurs assistants ; s’il y en a plusieurs, on choisit dans le panneau.': ('A button on the taskbar, and Windows+C, to ask a question about Winlux. Turn on one or more assistants; with several, you choose in the panel.', 'Un botón en la barra de tareas, y Windows+C, para preguntar sobre Winlux. Active uno o varios asistentes; si hay varios, se elige en el panel.', 'Eine Schaltfläche in der Taskleiste und Windows+C, um Fragen zu Winlux zu stellen. Aktivieren Sie einen oder mehrere Assistenten; bei mehreren wählen Sie im Fenster.'),
    'Claude': ('Claude', 'Claude', 'Claude'),
    'ChatGPT': ('ChatGPT', 'ChatGPT', 'ChatGPT'),
    'IA locale': ('Local AI', 'IA local', 'Lokale KI'),
    'Le modèle d’Anthropic, avec votre compte Claude. Le plus fin pour expliquer pourquoi un programme .exe ne s’ouvre pas.': ('Anthropic’s model, with your Claude account. The sharpest at explaining why an .exe program won’t open.', 'El modelo de Anthropic, con su cuenta de Claude. El más preciso para explicar por qué no se abre un programa .exe.', 'Das Modell von Anthropic mit Ihrem Claude-Konto. Erklärt am genauesten, warum ein .exe-Programm nicht startet.'),
    'Le modèle d’OpenAI, avec votre compte ChatGPT.': ('OpenAI’s model, with your ChatGPT account.', 'El modelo de OpenAI, con su cuenta de ChatGPT.', 'Das Modell von OpenAI mit Ihrem ChatGPT-Konto.'),
    'Une petite IA qui tourne sur ce PC, sans compte ni Internet. Trois agents : retrouver un fichier et l’ouvrir dans l’Explorateur, changer un réglage, aider sur un programme.': ('A small AI that runs on this PC, with no account or Internet. Three agents: find a file and open it in File Explorer, change a setting, help with a program.', 'Una pequeña IA que funciona en este equipo, sin cuenta ni Internet. Tres agentes: encontrar un archivo y abrirlo en el Explorador, cambiar un ajuste, ayudar con un programa.', 'Eine kleine KI, die auf diesem PC läuft, ohne Konto und ohne Internet. Drei Agenten: eine Datei finden und im Explorer öffnen, eine Einstellung ändern, bei einem Programm helfen.'),
    'À l’activation, Claude Code est téléchargé s’il manque (environ 300 Mo), puis vous vous connectez à votre compte Claude (Google, e-mail…).': ('When you turn it on, Claude Code is downloaded if needed (about 300 MB), then you sign in to your Claude account (Google, email…).', 'Al activarlo, se descarga Claude Code si falta (unos 300 MB) y luego inicia sesión en su cuenta de Claude (Google, correo…).', 'Beim Aktivieren wird Claude Code bei Bedarf heruntergeladen (etwa 300 MB), dann melden Sie sich bei Ihrem Claude-Konto an (Google, E-Mail…).'),
    'À l’activation, Codex, l’outil officiel d’OpenAI, est téléchargé (environ 170 Mo), puis vous vous connectez à votre compte ChatGPT (Google, e-mail…).': ('When you turn it on, Codex, OpenAI’s official tool, is downloaded (about 170 MB), then you sign in to your ChatGPT account (Google, email…).', 'Al activarlo, se descarga Codex, la herramienta oficial de OpenAI (unos 170 MB), y luego inicia sesión en su cuenta de ChatGPT (Google, correo…).', 'Beim Aktivieren wird Codex, das offizielle Werkzeug von OpenAI, heruntergeladen (etwa 170 MB), dann melden Sie sich bei Ihrem ChatGPT-Konto an (Google, E-Mail…).'),
    'À l’activation, le moteur llama.cpp (environ 30 Mo) et le modèle Qwen2.5 1,5B (environ 1,1 Go) sont téléchargés. Elle ne tourne que lorsque son panneau est ouvert.': ('When you turn it on, the llama.cpp engine (about 30 MB) and the Qwen2.5 1.5B model (about 1.1 GB) are downloaded. It only runs while its panel is open.', 'Al activarla, se descargan el motor llama.cpp (unos 30 MB) y el modelo Qwen2.5 1,5B (unos 1,1 GB). Solo funciona mientras su panel está abierto.', 'Beim Aktivieren werden die llama.cpp-Engine (etwa 30 MB) und das Modell Qwen2.5 1,5B (etwa 1,1 GB) heruntergeladen. Sie läuft nur, solange ihr Fenster geöffnet ist.'),
    'Compte {name}': ('{name} account', 'Cuenta de {name}', '{name}-Konto'),
    'Modèle': ('Model', 'Modelo', 'Modell'),
    'Supprimer l’IA locale': ('Remove the local AI', 'Eliminar la IA local', 'Lokale KI entfernen'),
    'Pas encore téléchargé': ('Not downloaded yet', 'Aún no descargado', 'Noch nicht heruntergeladen'),
    'Qwen2.5 1,5B, sur ce PC': ('Qwen2.5 1.5B, on this PC', 'Qwen2.5 1,5B, en este equipo', 'Qwen2.5 1,5B, auf diesem PC'),
    'Terminer l’installation': ('Finish installing', 'Terminar la instalación', 'Installation abschließen'),
    'Vérification de la connexion…': ('Checking the sign-in…', 'Comprobando el inicio de sesión…', 'Anmeldung wird geprüft…'),
    'Connecté au compte': ('Signed in', 'Sesión iniciada', 'Angemeldet'),
    'Connecté : {account}': ('Signed in: {account}', 'Sesión iniciada: {account}', 'Angemeldet: {account}'),
    'Non connecté': ('Not signed in', 'Sin iniciar sesión', 'Nicht angemeldet'),
    'Ouvrir l’assistant': ('Open the assistant', 'Abrir el asistente', 'Assistent öffnen'),
    'Se connecter au compte': ('Sign in', 'Iniciar sesión', 'Anmelden'),
    'Se déconnecter': ('Sign out', 'Cerrar sesión', 'Abmelden'),
    'Ce qui est envoyé': ('What is sent', 'Qué se envía', 'Was gesendet wird'),
    'Claude et ChatGPT : seuls votre question et la conversation en cours partent vers Anthropic ou OpenAI. Rien n’est gardé sur ce PC une fois la fenêtre fermée.': ('Claude and ChatGPT: only your question and the current conversation go to Anthropic or OpenAI. Nothing is kept on this PC once the window is closed.', 'Claude y ChatGPT: solo su pregunta y la conversación actual se envían a Anthropic u OpenAI. No se guarda nada en este equipo al cerrar la ventana.', 'Claude und ChatGPT: Nur Ihre Frage und das aktuelle Gespräch gehen an Anthropic oder OpenAI. Nach dem Schließen des Fensters bleibt nichts auf diesem PC gespeichert.'),
    'Ils ne lisent rien et ne changent rien tout seuls : chaque lecture (journal d’un programme, recherche de fichier) vous montre le texte exact avant l’envoi.': ('They read and change nothing on their own: each read (a program’s log, a file search) shows you the exact text before it is sent.', 'No leen ni cambian nada por sí solos: cada lectura (registro de un programa, búsqueda de archivos) le muestra el texto exacto antes del envío.', 'Sie lesen und ändern nichts von selbst: Jeder Lesezugriff (Protokoll eines Programms, Dateisuche) zeigt Ihnen vor dem Senden den genauen Text.'),
    'IA locale : rien ne quitte ce PC. Ce qu’elle lit (recherche de fichier, journal) vous est montré.': ('Local AI: nothing leaves this PC. What it reads (file search, log) is shown to you.', 'IA local: nada sale de este equipo. Lo que lee (búsqueda de archivos, registro) se le muestra.', 'Lokale KI: Nichts verlässt diesen PC. Was sie liest (Dateisuche, Protokoll), wird Ihnen angezeigt.'),
    'Pour les trois, chaque action (réglage, dossier, page des Paramètres) attend votre clic sur « Faire ».': ('For all three, each action (setting, folder, Settings page) waits for your click on “Do it”.', 'Para los tres, cada acción (ajuste, carpeta, página de Configuración) espera su clic en «Hacerlo».', 'Bei allen dreien wartet jede Aktion (Einstellung, Ordner, Einstellungsseite) auf Ihren Klick auf „Ausführen“.'),
    'Les connexions à vos comptes sont gardées dans votre dossier personnel, lisibles par vous seul.': ('The sign-ins to your accounts are kept in your home folder, readable only by you.', 'Los inicios de sesión de sus cuentas se guardan en su carpeta personal, legibles solo por usted.', 'Die Anmeldungen bei Ihren Konten werden in Ihrem persönlichen Ordner gespeichert, nur für Sie lesbar.'),
    'Boutiques de logiciels': ('Software stores', 'Tiendas de programas', 'Software-Stores'),
    'Winlux ne fournit ni n’impose aucune boutique. Celles que vous activez s’ajoutent au Store : le nécessaire est téléchargé chez l’éditeur et vérifié, puis la boutique apparaît dans le menu Démarrer. Sans connexion, elle est ajoutée au démarrage suivant.': ('Winlux neither ships nor imposes any store. The ones you turn on are added to the Store: what they need is downloaded from the publisher and checked, then the store appears in the Start menu. Without a connection, it is added at the next startup.', 'Winlux no incluye ni impone ninguna tienda. Las que active se añaden a la Tienda: lo necesario se descarga del editor y se comprueba, y la tienda aparece en el menú Inicio. Sin conexión, se añade en el siguiente arranque.', 'Winlux liefert und erzwingt keinen Store. Die von Ihnen aktivierten werden dem Store hinzugefügt: Das Nötige wird beim Herausgeber heruntergeladen und geprüft, dann erscheint der Store im Startmenü. Ohne Verbindung wird er beim nächsten Start hinzugefügt.'),
    'Logiciels Windows': ('Windows software', 'Programas de Windows', 'Windows-Programme'),
    'Logiciels Linux': ('Linux software', 'Programas de Linux', 'Linux-Programme'),
    'En attente du réseau : ajoutée au prochain démarrage.': ('Waiting for the network: added at the next startup.', 'Esperando la red: se añadirá en el próximo arranque.', 'Wartet auf das Netzwerk: wird beim nächsten Start hinzugefügt.'),
    'Ouvrir le Store': ('Open the Store', 'Abrir la Tienda', 'Store öffnen'),
    'Désactivé, un assistant n’apparaît nulle part et rien ne tourne.': ('When off, an assistant appears nowhere and nothing runs.', 'Desactivado, un asistente no aparece en ningún sitio y nada se ejecuta.', 'Ausgeschaltet erscheint ein Assistent nirgends und nichts läuft.'),
    # Sauvegarde (page 38)
    '1 an': ('1 year', '1 año', '1 Jahr'),
    '1 mois': ('1 month', '1 mes', '1 Monat'),
    '1 semaine': ('1 week', '1 semana', '1 Woche'),
    '2 ans': ('2 years', '2 años', '2 Jahre'),
    '3 mois': ('3 months', '3 meses', '3 Monate'),
    '6 mois': ('6 months', '6 meses', '6 Monate'),
    'Adresse attendue : smb://serveur/partage ou serveur:/chemin.': ('Expected address: smb://server/share or server:/path.', 'Dirección esperada: smb://servidor/recurso o servidor:/ruta.', 'Erwartete Adresse: smb://server/freigabe oder server:/pfad.'),
    'Adresse du partage': ('Share address', 'Dirección del recurso compartido', 'Adresse der Freigabe'),
    'Arrêter de sauvegarder sur cette destination ? Les sauvegardes déjà faites y restent.': ('Stop backing up to this destination? The backups already made stay there.', '¿Dejar de hacer copias en este destino? Las copias ya hechas se quedan allí.', 'Nicht mehr auf diesem Ziel sichern? Die bereits erstellten Sicherungen bleiben dort.'),
    'Attention : cette destination est sur le même disque que vos fichiers ; elle ne les protège pas d’une panne de ce disque.': ('Warning: this destination is on the same disk as your files; it does not protect them if that disk fails.', 'Atención: este destino está en el mismo disco que sus archivos; no los protege de una avería de ese disco.', 'Achtung: Dieses Ziel liegt auf demselben Datenträger wie Ihre Dateien; es schützt sie nicht bei einem Ausfall dieses Datenträgers.'),
    'Aucun disque externe détecté. Branchez un disque ou une clé USB : il apparaîtra ici.': ('No external drive detected. Plug in a drive or a USB stick: it will appear here.', 'No se ha detectado ningún disco externo. Conecte un disco o una memoria USB: aparecerá aquí.', 'Kein externes Laufwerk erkannt. Schließen Sie ein Laufwerk oder einen USB-Stick an: Es erscheint hier.'),
    'Aucune sauvegarde pour l’instant': ('No backup yet', 'Todavía no hay ninguna copia de seguridad', 'Noch keine Sicherung'),
    'Aucune sauvegarde sur cette destination pour l’instant.': ('No backup on this destination yet.', 'Todavía no hay ninguna copia de seguridad en este destino.', 'Noch keine Sicherung auf diesem Ziel.'),
    'Compte (facultatif)': ('Account (optional)', 'Cuenta (opcional)', 'Konto (optional)'),
    'Conserver les deux (recommandé)': ('Keep both (recommended)', 'Conservar ambos (recomendado)', 'Beide behalten (empfohlen)'),
    'Conserver les sauvegardes': ('Keep backups', 'Conservar las copias de seguridad', 'Sicherungen aufbewahren'),
    'Copie vos dossiers sur un disque externe ou un partage réseau. Chaque sauvegarde est complète, mais seuls les fichiers modifiés prennent de la place.': ('Copies your folders to an external drive or a network share. Each backup is complete, but only changed files take up space.', 'Copia sus carpetas en un disco externo o un recurso compartido de red. Cada copia está completa, pero solo los archivos modificados ocupan espacio.', 'Kopiert Ihre Ordner auf ein externes Laufwerk oder eine Netzwerkfreigabe. Jede Sicherung ist vollständig, aber nur geänderte Dateien belegen Platz.'),
    'Dernière sauvegarde': ('Last backup', 'Última copia de seguridad', 'Letzte Sicherung'),
    'Destination': ('Destination', 'Destino', 'Ziel'),
    'Destination indisponible : branchez le disque ou vérifiez le partage.': ('Destination unavailable: plug in the drive or check the share.', 'Destino no disponible: conecte el disco o compruebe el recurso compartido.', 'Ziel nicht verfügbar: Schließen Sie das Laufwerk an oder prüfen Sie die Freigabe.'),
    'Dossiers sauvegardés': ('Backed-up folders', 'Carpetas incluidas en la copia', 'Gesicherte Ordner'),
    'Fichiers déjà présents :': ('Files already there:', 'Archivos ya presentes:', 'Bereits vorhandene Dateien:'),
    'Fréquence': ('Frequency', 'Frecuencia', 'Häufigkeit'),
    'Historique et restauration': ('History and restore', 'Historial y restauración', 'Verlauf und Wiederherstellung'),
    'Jamais pendant une mise à jour ni pendant une partie (mode jeu).': ('Never during an update or a game (game mode).', 'Nunca durante una actualización ni durante una partida (modo juego).', 'Nie während eines Updates oder eines Spiels (Spielmodus).'),
    'La sauvegarde la plus récente est toujours gardée.': ('The most recent backup is always kept.', 'La copia más reciente siempre se conserva.', 'Die neueste Sicherung wird immer behalten.'),
    'Les points de restauration du système (Récupération) protègent Winlux lui-même ; la sauvegarde protège vos fichiers.': ('System restore points (Recovery) protect Winlux itself; backup protects your files.', 'Los puntos de restauración del sistema (Recuperación) protegen Winlux; la copia de seguridad protege sus archivos.', 'Systemwiederherstellungspunkte (Wiederherstellung) schützen Winlux selbst; die Sicherung schützt Ihre Dateien.'),
    'Libre sur la destination : {size}': ('Free on the destination: {size}', 'Libre en el destino: {size}', 'Frei auf dem Ziel: {size}'),
    'Ne plus utiliser cette destination': ('Stop using this destination', 'Dejar de usar este destino', 'Dieses Ziel nicht mehr verwenden'),
    'Nom de compte invalide.': ('Invalid account name.', 'Nombre de cuenta no válido.', 'Ungültiger Kontoname.'),
    'Partage Windows ou NAS : smb://serveur/partage. Partage NFS : serveur:/chemin. Le mot de passe est gardé par le système, lisible par root seul.': ('Windows share or NAS: smb://server/share. NFS share: server:/path. The password is kept by the system, readable by root only.', 'Recurso de Windows o NAS: smb://servidor/recurso. Recurso NFS: servidor:/ruta. La contraseña la guarda el sistema, legible solo por root.', 'Windows-Freigabe oder NAS: smb://server/freigabe. NFS-Freigabe: server:/pfad. Das Kennwort wird vom System gespeichert und ist nur für root lesbar.'),
    'Partage réseau': ('Network share', 'Recurso compartido de red', 'Netzwerkfreigabe'),
    'Partage réseau…': ('Network share…', 'Recurso compartido de red…', 'Netzwerkfreigabe…'),
    'Place utilisée : {size}': ('Space used: {size}', 'Espacio usado: {size}', 'Belegter Platz: {size}'),
    'Planification': ('Schedule', 'Programación', 'Zeitplan'),
    'Que restaurer ?': ('What to restore?', '¿Qué restaurar?', 'Was wiederherstellen?'),
    'Remplacer par la version sauvegardée': ('Replace with the backed-up version', 'Reemplazar por la versión guardada', 'Durch die gesicherte Version ersetzen'),
    'Remplacer vos fichiers actuels par ceux de la sauvegarde ? Les fichiers modifiés depuis seront écrasés.': ('Replace your current files with those from the backup? Files changed since will be overwritten.', '¿Reemplazar sus archivos actuales por los de la copia? Los archivos modificados desde entonces se sobrescribirán.', 'Ihre aktuellen Dateien durch die der Sicherung ersetzen? Seitdem geänderte Dateien werden überschrieben.'),
    'Restauration lancée': ('Restore started', 'Restauración iniciada', 'Wiederherstellung gestartet'),
    'Restaurer': ('Restore', 'Restaurar', 'Wiederherstellen'),
    'Restaurer dans un autre dossier…': ('Restore to another folder…', 'Restaurar en otra carpeta…', 'In einem anderen Ordner wiederherstellen…'),
    'Restaurer la sauvegarde du {date}': ('Restore the backup of {date}', 'Restaurar la copia del {date}', 'Sicherung vom {date} wiederherstellen'),
    'Restaurer ne supprime aucun fichier récent. Par défaut, un fichier modifié depuis garde sa version actuelle et la version sauvegardée est ajoutée à côté.': ('Restoring never deletes a recent file. By default, a file changed since keeps its current version and the backed-up version is added next to it.', 'Restaurar no elimina ningún archivo reciente. Por defecto, un archivo modificado desde entonces conserva su versión actual y la versión guardada se añade al lado.', 'Beim Wiederherstellen wird keine neuere Datei gelöscht. Standardmäßig behält eine seitdem geänderte Datei ihre aktuelle Version, und die gesicherte Version wird daneben abgelegt.'),
    'Restaurer…': ('Restore…', 'Restaurar…', 'Wiederherstellen…'),
    'Sauvegarde de mes fichiers': ('Back up my files', 'Copia de seguridad de mis archivos', 'Meine Dateien sichern'),
    'Sauvegarde en cours…': ('Backup in progress…', 'Copia de seguridad en curso…', 'Sicherung läuft…'),
    'Sauvegarder automatiquement': ('Back up automatically', 'Hacer copias automáticamente', 'Automatisch sichern'),
    'Sauvegarder maintenant': ('Back up now', 'Hacer una copia ahora', 'Jetzt sichern'),
    'Sauvegarder sur': ('Back up to', 'Guardar en', 'Sichern auf'),
    'Tous les dossiers sauvegardés': ('All backed-up folders', 'Todas las carpetas incluidas', 'Alle gesicherten Ordner'),
    'Toutes les 10 minutes': ('Every 10 minutes', 'Cada 10 minutos', 'Alle 10 Minuten'),
    'Toutes les 12 heures': ('Every 12 hours', 'Cada 12 horas', 'Alle 12 Stunden'),
    'Toutes les 15 minutes': ('Every 15 minutes', 'Cada 15 minutos', 'Alle 15 Minuten'),
    'Toutes les 3 heures': ('Every 3 hours', 'Cada 3 horas', 'Alle 3 Stunden'),
    'Toutes les 30 minutes': ('Every 30 minutes', 'Cada 30 minutos', 'Alle 30 Minuten'),
    'Toutes les 6 heures': ('Every 6 hours', 'Cada 6 horas', 'Alle 6 Stunden'),
    'Toutes les heures': ('Every hour', 'Cada hora', 'Stündlich'),
    'Un dossier WinluxFileHistory y est créé ; rien d’autre n’est touché.': ('A WinluxFileHistory folder is created there; nothing else is touched.', 'Allí se crea una carpeta WinluxFileHistory; no se toca nada más.', 'Dort wird ein Ordner WinluxFileHistory angelegt; sonst wird nichts verändert.'),
    'Une fois par jour': ('Once a day', 'Una vez al día', 'Einmal täglich'),
    'Vos fichiers sont remis en place en arrière-plan. Les fichiers créés depuis la sauvegarde ne sont pas touchés.': ('Your files are being put back in the background. Files created since the backup are not touched.', 'Sus archivos se restauran en segundo plano. Los archivos creados después de la copia no se tocan.', 'Ihre Dateien werden im Hintergrund zurückgelegt. Nach der Sicherung erstellte Dateien bleiben unberührt.'),
    '{count} fichiers': ('{count} files', '{count} archivos', '{count} Dateien'),
    '{count} sauvegardes plus anciennes sont gardées sur la destination.': ('{count} older backups are kept on the destination.', 'Se conservan {count} copias más antiguas en el destino.', '{count} ältere Sicherungen werden auf dem Ziel aufbewahrt.'),
    '{name} (non branché)': ('{name} (not plugged in)', '{name} (no conectado)', '{name} (nicht angeschlossen)'),
    '{path} (dossier absent, ignoré)': ('{path} (folder missing, skipped)', '{path} (carpeta ausente, omitida)', '{path} (Ordner fehlt, übersprungen)'),
    '{size} libres': ('{size} free', '{size} libres', '{size} frei'),
    '« Conserver les deux » : un fichier modifié depuis la sauvegarde garde sa version actuelle, l’ancienne arrive à côté avec « (restauré …) » dans son nom.': ('“Keep both”: a file changed since the backup keeps its current version; the old one is placed next to it with “(restauré …)” in its name.', '«Conservar ambos»: un archivo modificado después de la copia conserva su versión actual; la antigua se coloca al lado con «(restauré …)» en su nombre.', '„Beide behalten“: Eine seit der Sicherung geänderte Datei behält ihre aktuelle Version; die alte wird mit „(restauré …)“ im Namen daneben abgelegt.'),
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
PAGE_HARDWARE = 34
PAGE_ACCOUNTS = 33
PAGE_WIFI = 35
PAGE_CERTS = 36
PAGE_PHONE = 37
PAGE_BACKUP = 38
PHONE_CONFIG = Path(os.environ.get("WINLUX_PHONE_CONFIG", Path.home() / ".config/winlux/phone"))
PHONE_RUNTIME = Path(os.environ.get("WINLUX_PHONE_RUNTIME") or (Path(os.environ["XDG_RUNTIME_DIR"]) / "winlux-phone" if os.environ.get("XDG_RUNTIME_DIR") else Path.home() / ".cache/winlux/phone"))

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
ACCENTS = ("#6a3df0", "#0078d4", "#0099bc", "#00b294", "#10893e", "#ca5010", "#e81123", "#c239b3")


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


def settings_palette(dark, accent="#6a3df0"):
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
OSK_ICON = (("auto", "Sans clavier physique"), ("always", "Toujours"), ("never", "Jamais"))
POINTER_COLORS = (("default", "Thème du système"), ("white", "Blanc"), ("black", "Noir"), ("lime", "Vert vif"))
POINTER_TRAILS = (("0", "Aucune"), ("3", "Courte"), ("5", "Moyenne"), ("7", "Longue"))
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


# Icônes spéciales du bureau (xfdesktop) : clé, libellé, présence par défaut.
DESKTOP_ICONS = [("trash", "Corbeille"), ("filesystem", "Ce PC (système de fichiers)"),
                 ("home", "Dossier de l’utilisateur"), ("removable", "Lecteurs amovibles et réseau")]
DESKTOP_ICON_DEFAULTS = {"trash": True, "filesystem": False, "home": False, "removable": False}


def desktop_icon_shown(key):
    value = xfconf_get("xfce4-desktop", f"/desktop-icons/file-icons/show-{key}")
    return DESKTOP_ICON_DEFAULTS[key] if value is None else value == "true"


def ambiance_tool():
    """winlux-ambiance installé, ou le script du dépôt à côté de celui-ci."""
    if have("winlux-ambiance"): return ["winlux-ambiance"]
    script = Path(__file__).resolve().with_name("winlux-ambiance.py")
    return ["/usr/bin/python3", str(script)] if script.is_file() else None


def brightness_tool():
    """winlux-brightness installé, ou le script du dépôt à côté de celui-ci."""
    if have("winlux-brightness"): return ["winlux-brightness"]
    script = Path(__file__).resolve().with_name("winlux-brightness.py")
    return ["/usr/bin/python3", str(script)] if script.is_file() else None


def brightness_state(tool=None):
    tool = tool or brightness_tool()
    if tool is None: return {}
    result = run(tool + ["get"], timeout=10)
    try: return json.loads((result.stdout if result else "").strip().splitlines()[-1])
    except (IndexError, ValueError): return {}


def clipboard_tool():
    if have("winlux-clipboard"): return ["winlux-clipboard"]
    script = Path(__file__).resolve().with_name("winlux-clipboard.py")
    return ["/usr/bin/python3", str(script)] if script.is_file() else None


DISPLAY_SCALES = ("1", "1.25", "1.5", "1.75", "2", "2.5", "3")


def rate_label(rate):
    """« 143.97 » → « 144 Hz », comme Windows."""
    try: return f"{round(float(rate))} Hz"
    except ValueError: return rate


def osk_tool():
    if have("winlux-osk"): return ["winlux-osk"]
    script = Path(__file__).resolve().with_name("winlux-osk.py")
    return ["/usr/bin/python3", str(script)] if script.is_file() else None


def mouse_tool():
    if have("winlux-mouse"): return ["winlux-mouse"]
    script = Path(__file__).resolve().with_name("winlux-mouse.py")
    return ["/usr/bin/python3", str(script)] if script.is_file() else None


def ocr_tool():
    if have("winlux-ocr"): return ["winlux-ocr"]
    script = Path(__file__).resolve().with_name("winlux-ocr.py")
    return ["/usr/bin/python3", str(script)] if script.is_file() else None


def ambiance_status():
    tool = ambiance_tool()
    result = run(tool + ["status"]) if tool else None
    return keyvals(result.stdout) if result else {}


def run(args, timeout=5, env=None):
    """Commande courte et silencieuse ; None si elle est absente ou échoue."""
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=timeout, env=env)
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
    except (OSError, ValueError): return {"theme": "dark", "accent": "#6a3df0"}


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

def pactl_env():
    """pactl en locale C : en français, son JSON écrivait les nombres à virgule
    (« "balance":0,00 »), JSON invalide, et ses libellés changent de forme."""
    return dict(os.environ, LC_ALL="C", LANG="C")


def pactl_list(kind):
    result = run(["pactl", "-f", "json", "list", kind], timeout=2, env=pactl_env()) if have("pactl") else None
    if not result: return []
    try: items = json.loads(result.stdout or "[]")
    except ValueError: return []
    return [item for item in items if isinstance(item, dict)]


def pactl_default(kind):
    result = run(["pactl", f"get-default-{kind}"], timeout=2, env=pactl_env()) if have("pactl") else None
    return result.stdout.strip() if result else ""


def volume_percent(item):
    values = []
    for channel in (item.get("volume") or {}).values():
        try: values.append(int(str(channel.get("value_percent", "0")).rstrip("%")))
        except (AttributeError, ValueError): pass
    return max(values) if values else 0


def device_label(device):
    """Nom lisible d'une sortie ou d'une entrée, jamais « (null) »."""
    props = device.get("properties") or {}
    for value in (device.get("description"), props.get("device.description"), props.get("node.description"),
                  props.get("node.nick"), props.get("alsa.card_name")):
        if isinstance(value, str) and value.strip() and value.strip() not in ("(null)", "null"):
            return value.strip()
    return str(device.get("name") or "?")


def placeholder_device(device):
    """Sortie ou entrée factice de PipeWire (« auto_null », « Dummy Output ») :
    elle n'existe que quand aucune carte son n'est reliée."""
    props = device.get("properties") or {}
    name = str(device.get("name", ""))
    if name in ("auto_null", "auto_null.monitor") or name.startswith("null"):
        return True
    return props.get("factory.name") == "support.null-audio-sink" and not props.get("device.api")


def audio_devices():
    sinks = [s for s in pactl_list("sinks") if not placeholder_device(s)]
    sources = [s for s in pactl_list("sources")
               if not str(s.get("name", "")).endswith(".monitor") and s.get("monitor_of_sink") in (None, "", "n/a")
               and not placeholder_device(s)]
    return sinks, sources


def audio_server_running():
    return have("pactl") and run(["pactl", "info"], timeout=2) is not None


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
        # Serveur son en marche : la page s'affiche, même sans carte reconnue,
        # pour le dire clairement au lieu d'une « Sortie factice ».
        sinks, sources = audio_devices()
        return bool(sinks or sources) or audio_server_running()
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
    if page == PAGE_ACCOUNTS:
        # Session Live : comptes éphémères, page cachée.
        return accounts_module() is not None and not Path(os.environ.get(
            "WINLUX_LIVE_FLAG", "/etc/winlux/live-session")).exists()
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
    if page == PAGE_WIFI:
        module = wifi_page()
        return module is not None and module.available()
    if page == PAGE_CERTS:
        module = certificates_page()
        return module is not None and module.available()
    if page == PAGE_BACKUP:
        return have("winlux-backup") or BACKUP_RUN.is_dir()
    if page == PAGE_PHONE:
        # Rien d’associé : pas de page (l’application Téléphone connecté associe).
        return bool(phone_devices())
    return True


# ── Sauvegarde (winlux-backup) ──────────────────────────────────────────────

BACKUP_SHARE = re.compile(r"^(smb://[^\s/|]+/[^\n|]+|//[^\s/|]+/[^\n|]+|nfs://[^\s/|]+/[^\n|]*|[A-Za-z0-9._-]+:/(?!/)[^\n|]*)\Z")
BACKUP_RUN = RUN / "backup"


def backup_user():
    if os.environ.get("WINLUX_BACKUP_USER"): return os.environ["WINLUX_BACKUP_USER"]
    import pwd
    return pwd.getpwuid(os.getuid()).pw_name


def backup_refresh(action, *args, timeout=8):
    """Relevé synchrone (sans le relevé général de winlux-control, lent)."""
    if not have("winlux-control"): return
    run(["winlux-control", action, *args], timeout=timeout, env={**os.environ, "WINLUX_CONTROL_NO_STATUS": "1"})


def backup_state(user):
    backup_refresh("backup-status", user)
    text = read(BACKUP_RUN / user / "status.txt")
    return keyvals(text), text


def backup_drives():
    """Disques montés (clés et disques USB, autres disques) : (chemin, libre)."""
    backup_refresh("backup-drives")
    drives = []
    for line in read(BACKUP_RUN / "drives.txt").splitlines():
        if line.startswith("drive="):
            path, _, free = line[6:].partition("|")
            drives.append((path, free))
    return drives


def backup_drive_name(path):
    return Path(path).name or path


def backup_snapshots(user):
    """Sauvegardes de la destination, de la plus récente à la plus ancienne."""
    backup_refresh("backup-list", user, timeout=20)
    found = []
    for line in read(BACKUP_RUN / user / "snapshots.txt").splitlines():
        parts = line.split("|")
        if len(parts) >= 3 and re.fullmatch(r"[0-9]{8}-[0-9]{6}", parts[0]):
            found.append((parts[0], parts[2] or "0"))
    return sorted(found, reverse=True)


def backup_snapshot_date(name):
    """20261105-103118 → date lisible dans la langue de l’interface."""
    match = re.fullmatch(r"([0-9]{4})([0-9]{2})([0-9]{2})-([0-9]{2})([0-9]{2})[0-9]{2}", name)
    if not match: return name
    year, month, day, hour, minute = match.groups()
    if _LANG == "en": return f"{year}-{month}-{day} {hour}:{minute}"
    if _LANG == "de": return f"{day}.{month}.{year} {hour}:{minute}"
    return f"{day}/{month}/{year} {hour}:{minute}"


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


_ACCOUNTS = []


def accounts_module():
    """winlux-accounts (installé à côté, ou scripts/ du dépôt), chargé une fois."""
    if _ACCOUNTS: return _ACCOUNTS[0]
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    module = None
    for candidate in (here.with_name("winlux-accounts.py"), here.with_name("winlux-accounts")):
        if candidate.is_file():
            try:
                loader = SourceFileLoader("winlux_accounts", str(candidate))
                spec = importlib.util.spec_from_loader("winlux_accounts", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                break
            except Exception:
                module = None
    _ACCOUNTS.append(module)
    return module


ACCOUNT_NAME = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")


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
_WIFI = []


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


def wifi_page():
    """Page Wi-Fi (winlux-wifi, installé à côté), None si absente."""
    if _WIFI: return _WIFI[0]
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    module = None
    for candidate in (here.with_name("winlux-wifi.py"), here.with_name("winlux-wifi")):
        if candidate.is_file():
            try:
                loader = SourceFileLoader("winlux_wifi_page", str(candidate))
                spec = importlib.util.spec_from_loader("winlux_wifi_page", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                break
            except Exception:
                module = None
    _WIFI.append(module)
    return module


_CERTS = []


def certificates_page():
    """Page Certificats (winlux-certificates, installé à côté), None si absente."""
    if _CERTS: return _CERTS[0]
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    module = None
    for candidate in (here.with_name("winlux-certificates.py"), here.with_name("winlux-certificates")):
        if candidate.is_file():
            try:
                loader = SourceFileLoader("winlux_certificates_page", str(candidate))
                spec = importlib.util.spec_from_loader("winlux_certificates_page", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                break
            except Exception:
                module = None
    _CERTS.append(module)
    return module


def store_catalogs():
    """Lignes de « winlux-store-catalogs --list » : id, état, plateforme, nom, résumé, site."""
    if not have("winlux-store-catalogs"): return []
    try:
        out = subprocess.run(["winlux-store-catalogs", "--list"], capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.SubprocessError):
        return []
    return [parts for parts in (line.split("|") for line in out.splitlines()) if len(parts) == 6]
def phone_devices():
    """Téléphones associés (winlux-phone, ~/.config/winlux/phone/devices.json)."""
    try:
        devices = json.loads((PHONE_CONFIG / "devices.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return {key: value for key, value in devices.items() if isinstance(value, dict) and value.get("cert")} \
        if isinstance(devices, dict) else {}




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
                   PAGE_CLIPBOARD: "page_clipboard", PAGE_ACCOUNTS: "page_accounts",
                   PAGE_HARDWARE: "page_hardware", PAGE_WIFI: "page_wifi",
                   PAGE_CERTS: "page_certificates", PAGE_PHONE: "page_phone",
                   PAGE_BACKUP: "page_backup"}
    # Pages gardant leur cadre d’état, complétées par des réglages en dessous.
    EXTRA_SECTIONS = {9: "sections_about", 8: "sections_display", 2: "sections_network", 7: "sections_recovery", 19: "sections_cpu", 24: "sections_graphics"}

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

    # ── Appareils mobiles (Téléphone connecté) ──

    def page_phone(self):
        tool = shutil.which("winlux-phone") or str(Path(__file__).resolve().with_name("winlux-phone.py"))
        try: live = json.loads((PHONE_RUNTIME / "status.json").read_text(encoding="utf-8"))
        except (OSError, ValueError): live = {}
        state = {d.get("id"): d for d in live.get("devices", [])} if live.get("running") else {}
        enabled = True
        try: enabled = json.loads((PHONE_CONFIG / "settings.json").read_text(encoding="utf-8")).get("enabled", True) is not False
        except (OSError, ValueError, AttributeError): pass
        self.section(_t("Téléphone connecté"), _t("Notifications, SMS et fichiers de votre téléphone Android sur ce PC, avec l’application KDE Connect."))
        box = self.card()
        self.switch(box, _t("Autoriser la connexion de mes téléphones"), enabled,
                    lambda on: self.tool([tool, "enable" if on else "disable"]),
                    _t("Ports ouverts sur les réseaux privés seulement."))
        self.actions(box, [(_t("Ouvrir Téléphone connecté"), lambda: self.spawn([tool, "gui"]), enabled)])
        for device_id, saved in phone_devices().items():
            live_device = state.get(device_id, {})
            name = live_device.get("name") or saved.get("name") or device_id
            status = _t("Connecté") if live_device.get("connected") else _t("Hors de portée")
            battery = live_device.get("battery")
            if battery: status += " · " + _t("Batterie {level} %").format(level=battery.get("level"))
            self.section(name, status)
            box = self.card()
            prefs = saved.get("prefs", {})
            for key, title, subtitle in (
                    ("notifications", "Notifications du téléphone", "Affichées sur ce PC, avec réponse rapide."),
                    ("sms", "Messages (SMS) et contacts", "Lire et envoyer des SMS depuis ce PC."),
                    ("files", "Recevoir des fichiers", "Enregistrés dans Téléchargements.")):
                self.switch(box, _t(title), prefs.get(key, True) is not False,
                            lambda on, k=key, d=device_id: self.tool([tool, "set", d, k, "on" if on else "off"]),
                            _t(subtitle))
            self.actions(box, [(_t("Faire sonner"), lambda d=device_id: self.tool([tool, "ring", d], refresh=False),
                                bool(live_device.get("connected"))),
                               (_t("Dissocier"), lambda d=device_id: self.tool([tool, "unpair", d]))])

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
        if have("xfconf-query"):
            # Icônes du bureau d'XFCE : Windows 11 démarre avec la Corbeille seule.
            for key, title in DESKTOP_ICONS:
                self.switch(box, _t(title), desktop_icon_shown(key),
                            lambda on, k=key: xfconf_set("xfce4-desktop", f"/desktop-icons/file-icons/show-{k}",
                                                         "bool", str(on).lower()))
        else:
            self.actions(box, [(_t("Corbeille"), lambda: self.execute("desktop-icon-recycle")),
                               (_t("Ce PC"), lambda: self.execute("desktop-icon-computer"))])
        self.actions(box, [(_t("Explorateur"), lambda: self.execute("desktop-icon-explorer")),
                           (_t("Documents"), lambda: self.execute("desktop-icon-documents"))])
        self.hint(_t("Les raccourcis ajoutés se retirent d’un clic droit. Un clic droit sur une application du menu Démarrer crée aussi son raccourci."), box)
        if have("xfdesktop"):
            # xfdesktop range toujours les icônes sur une grille, comme
            # « Aligner les icônes sur la grille » de Windows : seul le
            # rangement automatique est proposé.
            self.actions(box, [(_t("Réorganiser les icônes"), lambda: self.spawn(["xfdesktop", "--arrange"]))])

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
        self.switch(box, _t("Météo en ligne"), prefs.get("weather"),
                    lambda on: self.tool(["winlux-widgets", "set", "weather", "on" if on else "off"], refresh=False),
                    _t("Désactivée par défaut. Activée, le service wttr.in reçoit votre adresse IP et le lieu."))
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
        if not sinks and not sources:
            self.section(_t("Sortie"))
            box = self.card()
            self.hint(_t("Aucun haut-parleur, casque ni micro n’est reconnu sur ce PC. Si l’ordinateur en a, "
                         "son pilote ou son firmware audio manque : la page Matériel et pilotes l’installe."), box)
            self.actions(box, [(_t("Matériel et pilotes"), lambda: self.open_page(PAGE_HARDWARE))])
            self.watch_audio_cards()
        for kind, title, devices, default in (("sink", "Sortie", sinks, pactl_default("sink")),
                                              ("source", "Entrée", sources, pactl_default("source"))):
            if not devices: continue
            self.section(_t(title))
            box = self.card()
            items = [(d.get("name", ""), device_label(d)) for d in devices]
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

    def watch_audio_cards(self):
        """Carte reconnue plus tard (pilote installé, casque USB branché) : la
        page se met à jour seule, sans redémarrer."""
        if getattr(self, "_audio_watch", None): return

        def check():
            if self.page != PAGE_SOUND:
                self._audio_watch = None
                return False
            sinks, sources = audio_devices()
            if sinks or sources:
                self._audio_watch = None
                self.extra_refresh()
                return False
            return True
        self._audio_watch = GLib.timeout_add_seconds(3, check)

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
        # Heure réglée automatiquement : une date fausse fait refuser tous les sites sécurisés.
        module = certificates_page()
        if module is not None: self.time_switch = module.time_section(self)

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
        # « Adwaita-dark » n'est pas un thème GTK 3 installé (rendu clair).
        if theme is None: theme = "Adwaita"
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
        self.program_cards = {}
        for exe, name in programs:
            self.program_card(module, exe, name)
        GLib.timeout_add(1500, self.forget_uninstalled, self.programs_box)

    def forget_uninstalled(self, programs_box):
        """Un programme désinstallé (fenêtre « Applications installées ») disparaît
        de la liste sans rouvrir la page."""
        if programs_box is not self.programs_box or programs_box.get_parent() is None: return False
        for exe, card in list(self.program_cards.items()):
            if not exe.is_file():
                card.destroy()
                del self.program_cards[exe]
        return True

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
        if not first: self.program_cards[exe] = box
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
        osk = osk_tool()
        if osk is not None:
            state = keyvals(getattr(run(osk + ["icon"], timeout=5), "stdout", "") or "")
            self.combo(box, _t("Icône du clavier tactile"), [(k, _t(v)) for k, v in OSK_ICON], state.get("setting", "auto"),
                       lambda value: self.tool(osk + ["set", value], refresh=False),
                       _t("Dans la barre des tâches. « Sans clavier physique » la montre quand aucun clavier n’est branché."))
            self.actions(box, [(_t("Ouvrir le clavier tactile"), lambda: self.spawn(osk + ["show"]))])
        self.sections_mouse(status)
        if status.get("touchpad_present") != "1": return
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

    def sections_mouse(self, status):
        """Souris : comme Paramètres > Bluetooth et appareils > Souris de Windows 11."""
        tool = mouse_tool()
        result = run(tool + ["status"], timeout=10) if tool else None
        mouse = keyvals(result.stdout) if result else {}
        if not mouse:
            self.section(_t("Souris"))
            box = self.card()
            speeds = [("25", _t("Lente")), ("50", _t("Normale")), ("75", _t("Rapide"))]
            self.combo(box, _t("Vitesse du pointeur"), speeds, status.get("mouse_speed") if status.get("mouse_speed") in ("25", "75") else "50",
                       lambda value: self.execute({"25": "mouse-slow", "75": "mouse-fast"}.get(value, "mouse-normal")))
            return

        def set_value(key, value):
            self.tool(tool + ["set", key, str(value)], refresh=False)
        # Section masquée sans souris libinput (tablette d'une VM, écran tactile seul).
        if "1" in (mouse.get("cap_left"), mouse.get("cap_speed"), mouse.get("cap_precision")):
            self.section(_t("Souris"))
            box = self.card()
        if mouse.get("cap_left") == "1":
            self.combo(box, _t("Bouton principal de la souris"), [("0", _t("Gauche")), ("1", _t("Droit"))],
                       mouse.get("left", "0"), lambda value: set_value("left", value))
        if mouse.get("cap_speed") == "1":
            speed = mouse.get("speed", "50")
            self.level_row(box, _t("Vitesse du pointeur"), int(speed) if speed.isdigit() else 50,
                           lambda value: set_value("speed", value))
        if mouse.get("cap_precision") == "1":
            self.switch(box, _t("Améliorer la précision du pointeur"), mouse.get("precision", "1") == "1",
                        lambda on: set_value("precision", int(on)),
                        _t("Le pointeur va plus loin quand la souris bouge vite."))
        self.section(_t("Défilement"))
        box = self.card()
        lines = mouse.get("lines", "3")
        choices = sorted({1, 2, 3, 4, 5, 6, 8, 10, 15, 20} | ({int(lines)} if lines.isdigit() else set()))
        self.combo(box, _t("Lignes à faire défiler à chaque cran de la molette"), [(str(n), str(n)) for n in choices], lines,
                   lambda value: set_value("lines", value), _t("Programmes Windows ; 3 par défaut."))
        if mouse.get("cap_natural") == "1":
            self.switch(box, _t("Sens de défilement inversé"), mouse.get("natural") == "1",
                        lambda on: set_value("natural", int(on)),
                        _t("Tourner la molette vers le bas fait monter le contenu."))
        self.section(_t("Double-clic"))
        box = self.card()
        double = mouse.get("double", "500")
        scale = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 200, 900, 50)
        scale.set_value(int(double) if double.isdigit() else 500)
        scale.set_inverted(True)  # à droite : plus rapide, comme Windows
        scale.set_draw_value(False)
        scale.set_size_request(260, -1)
        scale.connect("button-release-event", lambda w, _: set_value("double", int(round(w.get_value() / 50) * 50)) and False)
        self.row(box, _t("Vitesse du double-clic"), scale, _t("Plus lente à gauche, plus rapide à droite."))
        test = Gtk.EventBox()
        icon = Gtk.Image.new_from_icon_name("folder", Gtk.IconSize.DIALOG)
        test.add(icon)
        test.set_tooltip_text(_t("Double-cliquez sur le dossier pour tester."))

        def clicked(_widget, event):
            if event.type == Gdk.EventType._2BUTTON_PRESS:
                closed = icon.get_icon_name()[0] == "folder"
                icon.set_from_icon_name("folder-open" if closed else "folder", Gtk.IconSize.DIALOG)
            return True
        test.connect("button-press-event", clicked)
        self.row(box, _t("Zone de test"), test, _t("Double-cliquez sur le dossier : il s’ouvre ou se ferme si la vitesse vous convient."))
        self.section(_t("Pointeur"))
        box = self.card()
        if mouse.get("cap_color") == "1":
            self.combo(box, _t("Couleur du pointeur"), [(k, _t(v)) for k, v in POINTER_COLORS], mouse.get("color", "default"),
                       lambda value: set_value("color", value))
        self.combo(box, _t("Taille du pointeur"), [(i + 1, f"{size} px") for i, size in enumerate(POINTER_SIZES)],
                   a11y_read().get("pointer_size", 1), lambda value: self.set_pointer(int(value)))
        trail = mouse.get("trail", "0")
        self.combo(box, _t("Traînée du pointeur"), [(k, _t(v)) for k, v in POINTER_TRAILS],
                   trail if trail in dict(POINTER_TRAILS) else "0", lambda value: set_value("trail", value),
                   _t("Des copies du pointeur le suivent quand il bouge : il se repère mieux sur un grand écran."))

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

    # ── À propos : « Votre PC », sous la version et le noyau ──

    def sections_about(self):
        hardware = hardware_page()
        if hardware is not None: hardware.pc_section(self, self.status)

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

    # ── Comptes ──

    def account_action(self, args, password=None):
        """winlux-control account-* : Contrôle de compte d’utilisateur, puis
        action en root ; le mot de passe passe par l’entrée standard."""
        try:
            process = subprocess.Popen(["winlux-control", *args], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                       stderr=subprocess.PIPE, text=True)
            process.stdin.write((password or "") + "\n")
            process.stdin.close()
        except OSError as error:
            self.show_error(_t("L’opération a échoué"), str(error))
            return

        def wait():
            if process.poll() is None: return True
            if process.returncode:
                detail = read(RUN / "action.log").strip()[-2000:] or (process.stderr.read() or "").strip()
                self.show_error(_t("L’opération a échoué"), detail)
            self.extra_refresh()
            return False
        GLib.timeout_add(200, wait)

    def account_dialog(self, title, name_field, admin_field):
        """Boîte « Ajouter un compte » / « Changer le mot de passe »."""
        dialog = Gtk.Dialog(title=title, transient_for=self, modal=True)
        dialog.add_button(_t("Annuler"), Gtk.ResponseType.CANCEL)
        dialog.add_button(_t("Valider"), Gtk.ResponseType.OK)
        dialog.set_default_response(Gtk.ResponseType.OK)
        area = dialog.get_content_area()
        area.set_spacing(8)
        area.set_border_width(16)
        fields = {}

        def entry(key, label, secret=False, hint=None):
            area.pack_start(Gtk.Label(label=label, xalign=0), False, False, 0)
            widget = Gtk.Entry(visibility=not secret, activates_default=True)
            widget.get_accessible().set_name(label)
            area.pack_start(widget, False, False, 0)
            if hint: self.hint(hint, area)
            fields[key] = widget
        if name_field:
            entry("name", _t("Nom du compte"), hint=_t("Lettres minuscules, chiffres, - et _ (32 au plus), en commençant par une lettre."))
            entry("fullname", _t("Nom complet (facultatif)"))
        entry("password", _t("Mot de passe"), True, _t("Laissez vide pour un compte sans mot de passe."))
        entry("confirm", _t("Confirmer le mot de passe"), True)
        if admin_field:
            check = Gtk.CheckButton(label=_t("Administrateur de ce PC"))
            area.pack_start(check, False, False, 0)
            self.hint(_t("Un administrateur installe des logiciels et change les réglages du système ; un compte standard en a besoin pour ces actions."), area)
            fields["admin"] = check
        error = Gtk.Label(xalign=0, wrap=True)
        area.pack_start(error, False, False, 0)
        dialog.show_all()
        existing = {account["name"] for account in accounts_module().accounts()}
        while dialog.run() == Gtk.ResponseType.OK:
            values = {key: (w.get_active() if isinstance(w, Gtk.CheckButton) else w.get_text()) for key, w in fields.items()}
            if name_field and not ACCOUNT_NAME.match(values["name"]):
                error.set_text(_t("Lettres minuscules, chiffres, - et _ (32 au plus), en commençant par une lettre."))
            elif name_field and values["name"] in existing:
                error.set_text(_t("Ce nom de compte existe déjà."))
            elif values["password"] != values["confirm"]:
                error.set_text(_t("Les deux mots de passe sont différents."))
            else:
                dialog.destroy()
                return values
        dialog.destroy()
        return None

    def add_account(self):
        values = self.account_dialog(_t("Ajouter un compte"), True, True)
        if values is None: return
        args = ["account-add", values["name"], "admin" if values["admin"] else "user"]
        if values["fullname"].strip(): args.append(values["fullname"].strip().replace(":", " "))
        self.account_action(args, values["password"])

    def change_password(self, name):
        values = self.account_dialog(_t("Nouveau mot de passe pour {name}").format(name=name), False, False)
        if values is not None: self.account_action(["account-password", name], values["password"])

    def change_type(self, account):
        others = [a for a in accounts_module().accounts() if a["admin"] and a["name"] != account["name"]]
        if account["admin"] and not others:
            self.show_error(_t("L’opération a échoué"), _t("Il faut garder au moins un compte administrateur."))
            return
        self.account_action(["account-role", account["name"], "user" if account["admin"] else "admin"])

    def remove_account(self, account):
        dialog = Gtk.MessageDialog(transient_for=self, modal=True, buttons=Gtk.ButtonsType.OK_CANCEL,
                                   text=_t("Supprimer le compte {name} ?").format(name=account["name"]))
        wipe = Gtk.CheckButton(label=_t("Effacer aussi ses fichiers (Bureau, Documents, Images…)"))
        dialog.get_message_area().pack_start(wipe, False, False, 0)
        dialog.show_all()
        answer = dialog.run()
        files = "delete" if wipe.get_active() else "keep"
        dialog.destroy()
        if answer == Gtk.ResponseType.OK: self.account_action(["account-remove", account["name"], files])

    def page_accounts(self):
        module = accounts_module()
        import pwd
        try: me = pwd.getpwuid(os.getuid()).pw_name
        except KeyError: me = os.environ.get("USER", "")
        everyone = module.accounts()
        open_sessions = set()
        try: open_sessions = set(os.listdir(RUN / "sessions"))
        except OSError: pass

        def kind(account):
            return _t("Administrateur") if account["admin"] else _t("Standard")
        self.hint(_t("Comptes locaux sur ce PC. Chacun a son bureau, ses fichiers et ses réglages ; « Changer d’utilisateur » dans le menu Démarrer passe de l’un à l’autre sans rien fermer."))
        mine = next((a for a in everyone if a["name"] == me), None)
        if mine:
            self.section(_t("Votre compte"))
            box = self.card()
            title = f"{mine['fullname']} ({me})" if mine["fullname"] and mine["fullname"] != me else me
            self.row(box, title, None, kind(mine))
            self.actions(box, [(_t("Changer le mot de passe"), lambda: self.change_password(me))])
        self.section(_t("Autres utilisateurs"))
        box = self.card()
        self.row(box, _t("Ajouter un autre utilisateur à ce PC"), None)
        self.actions(box, [(_t("Ajouter un compte"), self.add_account)])
        others = [a for a in everyone if a["name"] != me]
        if not others:
            self.hint(_t("Aucun autre compte sur ce PC."), box)
        for account in others:
            box = self.card()
            title = f"{account['fullname']} ({account['name']})" if account["fullname"] and account["fullname"] != account["name"] else account["name"]
            subtitle = kind(account) + (" · " + _t("Session ouverte") if account["name"] in open_sessions else "")
            self.row(box, title, None, subtitle)
            self.actions(box, [(_t("Changer le type de compte"), lambda a=account: self.change_type(a)),
                               (_t("Changer le mot de passe"), lambda n=account["name"]: self.change_password(n)),
                               (_t("Supprimer"), lambda a=account: self.remove_account(a),
                                account["name"] not in open_sessions)])

    # ── Écran : éclairage nocturne ──

    def sections_screens(self):
        """Chaque écran : définition, fréquence d’actualisation, échelle ; avec
        plusieurs écrans, disposition et écran principal (comme Windows 11)."""
        status = self.status
        try: count = min(4, int(status.get("display_count", "0")))
        except ValueError: count = 0
        if count < 1 or not status.get("display_1_name"): return
        names = [status.get(f"display_{n}_name", "") for n in range(1, count + 1)]

        def display_set(*words):
            self.execute("display-set", *words)
        if count > 1:
            self.section(_t("Plusieurs écrans"))
            box = self.card()
            layouts = [("extend", _t("Étendre ces écrans")), ("duplicate", _t("Dupliquer ces écrans"))]
            layouts += [(f"only:{name}", _t("Afficher seulement sur {name}").format(name=name)) for name in names if name]
            self.combo(box, _t("Disposition"), layouts, status.get("display_layout", "extend"),
                       lambda value: display_set("layout", "only", value[5:]) if value.startswith("only:")
                       else display_set("layout", value))
            primary = next((name for n, name in enumerate(names, 1) if status.get(f"display_{n}_primary") == "1"), names[0])
            self.combo(box, _t("Écran principal"), [(name, name) for name in names if name], primary,
                       lambda value: display_set("primary", value))
        for n, name in enumerate(names, 1):
            if not name: continue
            title = name if count > 1 else _t("Écran")
            if status.get(f"display_{n}_internal") == "1": title += " · " + _t("écran intégré")
            self.section(title)
            box = self.card()
            modes = [m for m in status.get(f"display_{n}_modes", "").split(",") if m]
            if modes:
                self.combo(box, _t("Définition de l’écran"), [(m, m.replace("x", " × ")) for m in modes],
                           status.get(f"display_{n}_mode") or modes[0], lambda value, name=name: display_set("mode", name, value))
            rates = [r for r in status.get(f"display_{n}_rates", "").split(",") if r]
            # Une seule fréquence (VM, écran 60 Hz) : rien à choisir, ligne masquée.
            if len(rates) > 1:
                labels = {}
                for rate in rates: labels.setdefault(rate_label(rate), rate)
                choices = [("auto", _t("Automatique"))] + [(rate, label) for label, rate in labels.items()]
                active = "auto" if status.get(f"display_{n}_rate_auto", "1") == "1" else status.get(f"display_{n}_rate")
                self.combo(box, _t("Fréquence d’actualisation"), choices, active if active in dict(choices) else "auto",
                           lambda value, name=name: display_set("rate", name, value),
                           _t("Plus elle est élevée, plus les mouvements sont fluides (jeux, défilement)."))
            scales = [("auto", _t("Automatique"))] + [(v, f"{round(float(v) * 100)} %") for v in DISPLAY_SCALES]
            self.combo(box, _t("Échelle"), scales,
                       "auto" if status.get(f"display_{n}_scale_auto", "1") == "1" else status.get(f"display_{n}_scale", "auto"),
                       lambda value, name=name: display_set("scale", name, value))
        if status.get("display_restart_hint") == "1":
            self.hint(_t("La nouvelle échelle s’applique complètement à la prochaine ouverture de session."))

    def sections_display(self):
        self.section_brightness()
        self.sections_screens()
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

    # ── Écran : luminosité (winlux-brightness), cachée sans contrôle réel ──

    def section_brightness(self):
        tool = brightness_tool()
        state = brightness_state(tool)
        if state.get("method") in (None, "none") or state.get("percent") is None: return
        self.section(_t("Luminosité et couleur"))
        box = self.card()
        scale = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 5, 100, 1)
        scale.set_name("winlux-brightness")
        scale.set_value(int(state["percent"]))
        scale.set_size_request(240, -1)
        scale.set_draw_value(True)
        scale.set_value_pos(Gtk.PositionType.RIGHT)
        scale.get_accessible().set_name(_t("Luminosité de l’écran, en pourcentage"))
        pending = {}

        def moved(widget):
            # Valeur envoyée quand le curseur s’arrête, pas à chaque pixel.
            if pending.get("id"): GLib.source_remove(pending["id"])
            def send():
                pending["id"] = None
                self.tool(tool + ["set", str(int(widget.get_value()))], refresh=False)
                return False
            pending["id"] = GLib.timeout_add(150, send)
        scale.connect("value-changed", moved)
        self.brightness_scale = scale
        hint = {"backlight": None, "ddc": _t("Écran réglé par son câble (DDC/CI)."),
                "soft": _t("Cet écran n’a pas de rétroéclairage pilotable : Winlux assombrit l’image.")}.get(state["method"])
        self.row(box, _t("Luminosité"), scale, hint)

    # ── Réseau : renvoi vers la page Wi-Fi, seulement avec une carte Wi-Fi ──

    def sections_network(self):
        if not page_available(PAGE_WIFI): return
        self.section(_t("Wi-Fi"))
        box = self.card()
        button = Gtk.Button(label=_t("Réseaux Wi-Fi…"))
        button.connect("clicked", lambda *_: GLib.idle_add(self.open_page, PAGE_WIFI))
        self.row(box, _t("Wi-Fi"), button, _t("Choisir un réseau, entrer sa clé, WPS, réseaux cachés et mode avion."))

    # ── Wi-Fi : réseaux, clé de sécurité, WPS, réseaux cachés (winlux-wifi) ──

    def page_wifi(self):
        module = wifi_page()
        if module is not None:
            self.wifi_panel = module.page(self)

    # ── Certificats : autorités ajoutées, heure, test d'un site (winlux-certificates) ──

    def page_certificates(self):
        module = certificates_page()
        if module is not None:
            self.certificates_panel = module.page(self)

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

    # ── Assistant IA (#286) : Claude, ChatGPT, IA locale, tous désactivés par défaut ──

    ASSISTANTS = (
        ("claude", "Claude",
         "Le modèle d’Anthropic, avec votre compte Claude. Le plus fin pour expliquer pourquoi un programme .exe ne s’ouvre pas.",
         "À l’activation, Claude Code est téléchargé s’il manque (environ 300 Mo), puis vous vous connectez à votre compte Claude (Google, e-mail…)."),
        ("chatgpt", "ChatGPT",
         "Le modèle d’OpenAI, avec votre compte ChatGPT.",
         "À l’activation, Codex, l’outil officiel d’OpenAI, est téléchargé (environ 170 Mo), puis vous vous connectez à votre compte ChatGPT (Google, e-mail…)."),
        ("local", "IA locale",
         "Une petite IA qui tourne sur ce PC, sans compte ni Internet. Trois agents : retrouver un fichier et l’ouvrir dans l’Explorateur, changer un réglage, aider sur un programme.",
         "À l’activation, le moteur llama.cpp (environ 30 Mo) et le modèle Qwen2.5 1,5B (environ 1,1 Go) sont téléchargés. Elle ne tourne que lorsque son panneau est ouvert."),
    )

    def page_assistant(self):
        import sys, threading
        module = assistant_module()
        module.protect_credentials()
        self.hint(_t("Un bouton dans la barre des tâches, et Windows+C, pour poser une question sur Winlux. Activez un ou plusieurs assistants ; s’il y en a plusieurs, on choisit dans le panneau."))
        for key, name, text, before in self.ASSISTANTS:
            backend = module.provider(key)
            if not backend.supported(): continue  # matériel trop juste : option cachée, pas grisée
            enabled = module.is_enabled(key)
            box = self.card()
            self.switch(box, _t(name), enabled, lambda on, k=key: self.assistant_toggle(k, on), _t(text))
            if not enabled:
                self.hint(_t(before), box)
                continue
            account = Gtk.Label(label=_t("Vérification de la connexion…"), xalign=0)
            account.get_style_context().add_class("subtitle")
            if backend.remote:
                self.row(box, _t("Compte {name}").format(name=name), account)
                buttons = self.actions(box, [(_t("Ouvrir l’assistant"), lambda k=key: self.assistant_open(k)),
                                             (_t("Se déconnecter"), lambda k=key: self.tool([sys.executable, module.__file__, "--deconnecter", k]))])
            else:
                self.row(box, _t("Modèle"), account)
                buttons = self.actions(box, [(_t("Ouvrir l’assistant"), lambda k=key: self.assistant_open(k)),
                                             (_t("Supprimer l’IA locale"), lambda: self.tool([sys.executable, module.__file__, "--supprimer", "local"]))])

            def checked(status, account=account, buttons=buttons, key=key, backend=backend):
                if not status.get("installed"): text = _t("Pas encore téléchargé")
                elif not backend.remote: text = _t("Qwen2.5 1,5B, sur ce PC")
                elif status.get("loggedIn"):
                    who = module.account_name(status)
                    text = _t("Connecté : {account}").format(account=who) if who else _t("Connecté au compte")
                else: text = _t("Non connecté")
                if account.get_toplevel() is not None: account.set_text(text)
                if not status.get("loggedIn"):
                    for child in buttons.get_children(): child.destroy()
                    login = Gtk.Button(label=_t("Se connecter au compte") if backend.remote and status.get("installed") else _t("Terminer l’installation"))
                    login.connect("clicked", lambda *_: self.assistant_toggle(key, True))
                    buttons.pack_start(login, False, False, 0)
                    login.show()
                return False
            threading.Thread(target=lambda b=backend, c=checked: GLib.idle_add(c, b.status()), daemon=True).start()
        self.section(_t("Ce qui est envoyé"))
        box = self.card()
        for text in ("Claude et ChatGPT : seuls votre question et la conversation en cours partent vers Anthropic ou OpenAI. Rien n’est gardé sur ce PC une fois la fenêtre fermée.",
                     "Ils ne lisent rien et ne changent rien tout seuls : chaque lecture (journal d’un programme, recherche de fichier) vous montre le texte exact avant l’envoi.",
                     "IA locale : rien ne quitte ce PC. Ce qu’elle lit (recherche de fichier, journal) vous est montré.",
                     "Pour les trois, chaque action (réglage, dossier, page des Paramètres) attend votre clic sur « Faire ».",
                     "Les connexions à vos comptes sont gardées dans votre dossier personnel, lisibles par vous seul.",
                     "Désactivé, un assistant n’apparaît nulle part et rien ne tourne."):
            self.hint(_t(text), box)

    def assistant_open(self, key):
        import sys
        module = assistant_module()
        module.write_config(current=key)
        self.spawn([sys.executable, module.__file__])

    def assistant_toggle(self, key, on):
        """Activer ouvre la fenêtre de configuration (téléchargement, connexion) ;
        l’assistant n’est activé qu’une fois tout prêt. Désactiver est immédiat."""
        import sys
        module = assistant_module()
        if not on:
            self.tool([sys.executable, module.__file__, "--desactiver", key])
            return
        if getattr(self, "assistant_setup", None) is not None and self.assistant_setup.poll() is None: return
        try:
            self.assistant_setup = subprocess.Popen([sys.executable, module.__file__, "--configurer", key], start_new_session=True)
        except OSError as error:
            self.show_error(_t("L’opération a échoué"), str(error))
            return

        def wait():
            if self.assistant_setup.poll() is None: return True
            self.extra_refresh()
            return False
        GLib.timeout_add(500, wait)

    # ── Partage et transfert ──

    # ── Sauvegarde : mes dossiers vers un disque externe ou un partage (#N) ──

    BACKUP_FREQUENCIES = ((10, "Toutes les 10 minutes"), (15, "Toutes les 15 minutes"), (30, "Toutes les 30 minutes"),
                          (60, "Toutes les heures"), (180, "Toutes les 3 heures"), (360, "Toutes les 6 heures"),
                          (720, "Toutes les 12 heures"), (1440, "Une fois par jour"))
    BACKUP_KEEP = ((0, "Toujours"), (7, "1 semaine"), (30, "1 mois"), (90, "3 mois"), (180, "6 mois"),
                   (365, "1 an"), (730, "2 ans"))

    def backup_do(self, action, *args):
        """Action winlux-backup pour ce compte (relais root sur le système installé)."""
        self.execute(action, backup_user(), *args)

    def page_backup(self):
        user = backup_user()
        info, text = backup_state(user)
        if getattr(self, "backup_timer", None):
            GLib.source_remove(self.backup_timer)
            self.backup_timer = None
        self.section(_t("Sauvegarde de mes fichiers"),
                     _t("Copie vos dossiers sur un disque externe ou un partage réseau. Chaque sauvegarde est complète, mais seuls les fichiers modifiés prennent de la place."))
        destination = info.get("destination", "")
        running = info.get("running") == "1"
        box = self.card()
        if running:
            last = _t("Sauvegarde en cours…")
        elif info.get("last_run_text"):
            last = info["last_run_text"]
        else:
            last = _t("Aucune sauvegarde pour l’instant")
        detail = []
        if info.get("last_result") == "unavailable":
            detail.append(_t("Destination indisponible : branchez le disque ou vérifiez le partage."))
        elif info.get("last_result") == "ok" and info.get("used"):
            detail.append(_t("Place utilisée : {size}").format(size=info["used"]))
        if info.get("free"): detail.append(_t("Libre sur la destination : {size}").format(size=info["free"]))
        self.row(box, _t("Dernière sauvegarde"), None, last + ("\n" + " · ".join(detail) if detail else ""))
        self.actions(box, [(_t("Sauvegarder maintenant"), lambda: self.backup_do("backup-run"),
                            bool(destination) and not running)])
        if running:
            # Rafraîchie tant que la sauvegarde tourne, page ouverte.
            page = self.page

            def again():
                self.backup_timer = None
                if self.page == page: self.extra_refresh()
                return False
            self.backup_timer = GLib.timeout_add_seconds(3, again)

        # Destination : disques réellement branchés, sinon le partage choisi.
        self.section(_t("Destination"))
        box = self.card()
        drives = backup_drives()
        items = [(path, f"{backup_drive_name(path)} · " + _t("{size} libres").format(size=free)) for path, free in drives]
        network = info.get("network") == "1"
        if destination and destination not in [path for path, _ in drives]:
            label = destination if network else _t("{name} (non branché)").format(name=backup_drive_name(destination))
            items.insert(0, (destination, label))
        if items:
            self.combo(box, _t("Sauvegarder sur"), items, destination or None,
                       lambda path: path != destination and self.backup_do("backup-set", "DESTINATION", path),
                       _t("Un dossier WinluxFileHistory y est créé ; rien d’autre n’est touché."))
        else:
            self.hint(_t("Aucun disque externe détecté. Branchez un disque ou une clé USB : il apparaîtra ici."), box)
        if info.get("same_disk") == "1":
            self.hint(_t("Attention : cette destination est sur le même disque que vos fichiers ; elle ne les protège pas d’une panne de ce disque."), box)
        buttons = [(_t("Partage réseau…"), self.backup_share_dialog)]
        if destination:
            buttons.append((_t("Ne plus utiliser cette destination"),
                            lambda: self.confirm(_t("Arrêter de sauvegarder sur cette destination ? Les sauvegardes déjà faites y restent.")) and self.backup_do("backup-set", "DESTINATION", "")))
        self.actions(box, buttons)

        if destination:
            self.section(_t("Planification"))
            box = self.card()
            enabled = info.get("enabled") == "1"
            self.switch(box, _t("Sauvegarder automatiquement"), enabled,
                        lambda on: self.backup_do("backup-set", "ENABLED", "1" if on else "0"),
                        _t("Jamais pendant une mise à jour ni pendant une partie (mode jeu)."))
            if enabled:
                self.combo(box, _t("Fréquence"), [(m, _t(t)) for m, t in self.BACKUP_FREQUENCIES], info.get("frequency") or "60",
                           lambda value: self.backup_do("backup-set", "FREQUENCY", value))
                self.combo(box, _t("Conserver les sauvegardes"), [(d, _t(t)) for d, t in self.BACKUP_KEEP], info.get("keep_days") or "0",
                           lambda value: self.backup_do("backup-set", "KEEP_DAYS", value),
                           _t("La sauvegarde la plus récente est toujours gardée."))

        self.section(_t("Dossiers sauvegardés"))
        box = self.card()
        home = info.get("home", "")
        for line in text.splitlines():
            if not line.startswith("folder="): continue
            present, _, path = line[7:].partition("|")
            remove = Gtk.Button(label=_t("Retirer"))
            remove.connect("clicked", lambda *_, p=path: self.backup_do("backup-remove", p))
            name = path[len(home) + 1:] if home and path.startswith(home + "/") else path
            self.row(box, name, remove, path if present == "1" else _t("{path} (dossier absent, ignoré)").format(path=path))

        def pick():
            dialog = Gtk.FileChooserNative.new(_t("Choisir un dossier"), self, Gtk.FileChooserAction.SELECT_FOLDER, None, None)
            if dialog.run() == Gtk.ResponseType.ACCEPT and dialog.get_filename():
                self.backup_do("backup-add", dialog.get_filename())
            dialog.destroy()
        self.actions(box, [(_t("Ajouter un dossier…"), pick)])

        if destination and info.get("available") == "1":
            snapshots = backup_snapshots(user)
            self.section(_t("Historique et restauration"),
                         _t("Restaurer ne supprime aucun fichier récent. Par défaut, un fichier modifié depuis garde sa version actuelle et la version sauvegardée est ajoutée à côté."))
            box = self.card()
            if not snapshots:
                self.hint(_t("Aucune sauvegarde sur cette destination pour l’instant."), box)
            for name, files in snapshots[:10]:
                button = Gtk.Button(label=_t("Restaurer…"))
                button.connect("clicked", lambda *_, n=name: self.backup_restore_dialog(n, text, home))
                self.row(box, backup_snapshot_date(name), button, _t("{count} fichiers").format(count=files))
            if len(snapshots) > 10:
                self.hint(_t("{count} sauvegardes plus anciennes sont gardées sur la destination.").format(count=len(snapshots) - 10), box)
        self.hint(_t("Les points de restauration du système (Récupération) protègent Winlux lui-même ; la sauvegarde protège vos fichiers."))

    def backup_share_dialog(self):
        dialog = Gtk.Dialog(title=_t("Partage réseau"), transient_for=self, modal=True)
        dialog.add_button(_t("Annuler"), Gtk.ResponseType.CANCEL)
        dialog.add_button(_t("Valider"), Gtk.ResponseType.OK)
        dialog.set_default_response(Gtk.ResponseType.OK)
        area = dialog.get_content_area()
        area.set_spacing(8)
        area.set_border_width(16)
        fields = {}
        for key, label, secret in (("url", _t("Adresse du partage"), False), ("account", _t("Compte (facultatif)"), False),
                                   ("password", _t("Mot de passe"), True)):
            area.pack_start(Gtk.Label(label=label, xalign=0), False, False, 0)
            widget = Gtk.Entry(visibility=not secret, activates_default=True)
            widget.get_accessible().set_name(label)
            area.pack_start(widget, False, False, 0)
            fields[key] = widget
        fields["url"].set_placeholder_text("smb://nas/sauvegardes")
        self.hint(_t("Partage Windows ou NAS : smb://serveur/partage. Partage NFS : serveur:/chemin. Le mot de passe est gardé par le système, lisible par root seul."), area)
        error = Gtk.Label(xalign=0, wrap=True)
        area.pack_start(error, False, False, 0)
        dialog.show_all()
        values = None
        while dialog.run() == Gtk.ResponseType.OK:
            url, account = fields["url"].get_text().strip(), fields["account"].get_text().strip()
            if not BACKUP_SHARE.match(url):
                error.set_text(_t("Adresse attendue : smb://serveur/partage ou serveur:/chemin."))
            elif "\n" in account or "|" in account:
                error.set_text(_t("Nom de compte invalide."))
            else:
                values = (url, account, fields["password"].get_text())
                break
        dialog.destroy()
        if values is None: return
        url, account, password = values
        args = ["backup-share", backup_user(), url] + ([account, "-"] if account else [])
        self.account_action(args, password if account else None)

    def backup_restore_dialog(self, snapshot, text, home):
        dialog = Gtk.Dialog(title=_t("Restaurer la sauvegarde du {date}").format(date=backup_snapshot_date(snapshot)),
                            transient_for=self, modal=True)
        dialog.add_button(_t("Annuler"), Gtk.ResponseType.CANCEL)
        dialog.add_button(_t("Restaurer"), Gtk.ResponseType.OK)
        dialog.set_default_response(Gtk.ResponseType.OK)
        area = dialog.get_content_area()
        area.set_spacing(8)
        area.set_border_width(16)
        area.pack_start(Gtk.Label(label=_t("Que restaurer ?"), xalign=0), False, False, 0)
        what = Gtk.ComboBoxText()
        what.append("/", _t("Tous les dossiers sauvegardés"))
        for line in text.splitlines():
            if not line.startswith("folder="): continue
            path = line[7:].partition("|")[2]
            what.append(path, path[len(home) + 1:] if home and path.startswith(home + "/") else path)
        what.set_active(0)
        area.pack_start(what, False, False, 0)
        area.pack_start(Gtk.Label(label=_t("Fichiers déjà présents :"), xalign=0), False, False, 0)
        safe = Gtk.RadioButton.new_with_label(None, _t("Conserver les deux (recommandé)"))
        replace = Gtk.RadioButton.new_with_label_from_widget(safe, _t("Remplacer par la version sauvegardée"))
        elsewhere = Gtk.RadioButton.new_with_label_from_widget(safe, _t("Restaurer dans un autre dossier…"))
        for widget in (safe, replace, elsewhere): area.pack_start(widget, False, False, 0)
        self.hint(_t("« Conserver les deux » : un fichier modifié depuis la sauvegarde garde sa version actuelle, l’ancienne arrive à côté avec « (restauré …) » dans son nom."), area)
        dialog.show_all()
        answer = dialog.run()
        path = what.get_active_id() or "/"
        mode = "replace" if replace.get_active() else "safe"
        other = elsewhere.get_active()
        dialog.destroy()
        if answer != Gtk.ResponseType.OK: return
        if other:
            chooser = Gtk.FileChooserNative.new(_t("Choisir un dossier"), self, Gtk.FileChooserAction.SELECT_FOLDER, None, None)
            target = chooser.get_filename() if chooser.run() == Gtk.ResponseType.ACCEPT else None
            chooser.destroy()
            if not target: return
            mode = target
        elif mode == "replace" and not self.confirm(_t("Remplacer vos fichiers actuels par ceux de la sauvegarde ? Les fichiers modifiés depuis seront écrasés.")):
            return
        self.backup_do("backup-restore", snapshot, path, mode)
        self.show_info(_t("Restauration lancée"), _t("Vos fichiers sont remis en place en arrière-plan. Les fichiers créés depuis la sauvegarde ne sont pas touchés."))

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

    def store_catalogs_section(self):
        """Boutiques de logiciels (docs/BOUTIQUES.md) : aucune fournie ni
        présélectionnée, ordre alphabétique, téléchargées chez l'éditeur."""
        rows = store_catalogs()
        if not rows: return
        self.section(_t("Boutiques de logiciels"), _t("Winlux ne fournit ni n’impose aucune boutique. Celles que vous activez s’ajoutent au Store : le nécessaire est téléchargé chez l’éditeur et vérifié, puis la boutique apparaît dans le menu Démarrer. Sans connexion, elle est ajoutée au démarrage suivant."))
        box = self.card()
        for ident, state, platform, name, summary, _site in rows:
            label = f"{name} · {_t('Logiciels Windows') if platform == 'windows' else _t('Logiciels Linux')}"
            if state == "pending": summary = _t("En attente du réseau : ajoutée au prochain démarrage.") + "\n" + summary
            self.switch(box, label, state != "off",
                        lambda on, i=ident: self.execute(f"store-catalog-{'add' if on else 'remove'}-{i}"), summary)
        self.actions(box, [(_t("Ouvrir le Store"), lambda: self.spawn(["winlux-shell-app", "store"]))])

    def page_apps(self):
        self.store_catalogs_section()
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

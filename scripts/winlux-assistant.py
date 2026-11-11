#!/usr/bin/python3
"""Assistant IA de Winlux : poser une question depuis la barre des tâches.

Trois assistants au choix, tous désactivés par défaut et activés un par un
dans Paramètres > Applications > Assistant IA :
- Claude (Anthropic) : Claude Code (claude.exe à travers la couche PE, même
  installation que l'application du menu Démarrer), connexion au compte
  Claude (Google, e-mail…) avec « claude auth login ».
- ChatGPT (OpenAI) : Codex, l'outil officiel d'OpenAI (programme Linux),
  téléchargé depuis npm avec l'empreinte SHA-512 du registre, connexion au
  compte ChatGPT avec « codex login ».
- IA locale : llama.cpp (llama-server) et le petit modèle Qwen2.5 1,5B, tout
  sur ce PC ; trois agents au plus (fichiers, réglages, aide).
Rien de tout cela n'est dans l'ISO. Tant qu'aucun n'est activé, rien
n'apparaît dans la barre et rien ne tourne : Windows+C ne fait rien.

Garde-fous :
- Claude et ChatGPT n'ont aucun outil : ils ne lisent ni ne touchent rien
  eux-mêmes. Pour lire un journal ou chercher un fichier, ils le demandent ;
  l'utilisateur voit le texte exact qui partirait et accepte ou refuse.
- Les actions (réglages, pages des Paramètres, ouvrir un dossier) viennent
  d'une liste fermée et attendent un clic sur « Faire », pour les trois.
- Seuls partent la question et la conversation en cours ; rien n'est gardé
  sur le disque, la télémétrie est coupée. L'IA locale n'envoie rien.
- Les connexions sont propres à l'assistant (~/.local/share/winlux/
  assistant-comptes) : ~/.claude et ~/.codex des autres outils ne sont jamais
  lus ni réutilisés. Dossiers en 700, jetons en 600 à chaque lancement.

Commandes : winlux-assistant [--configurer | --activer | --desactiver |
--deconnecter] [claude|chatgpt|local], --supprimer local, --etat.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import secrets
import shutil
import socket
import subprocess
import sys
import tarfile
import threading
import time
import unicodedata
import urllib.error
import urllib.request
import zipfile

HOME = Path.home()
CONFIG = Path(os.environ.get("XDG_CONFIG_HOME") or HOME / ".config") / "winlux/assistant.conf"
DATA = Path(os.environ.get("XDG_DATA_HOME") or HOME / ".local/share") / "winlux"
WORK = DATA / "assistant"
CODEX_DIR = DATA / "codex"
LOCAL_DIR = DATA / "ia-locale"
# Connexions propres à l'assistant : jamais celles de ~/.claude ou ~/.codex
# (Claude Code ou Codex installés à part par l'utilisateur).
ACCOUNTS = DATA / "assistant-comptes"
CLAUDE_ACCOUNT = ACCOUNTS / "claude"
CODEX_ACCOUNT = ACCOUNTS / "chatgpt"
# Journal du compte (#633) : /run/winlux/journal-<uid>, celui de root sans uid.
JOURNAL = Path(os.environ.get("WINLUX_JOURNAL_DIR") or ("/run/winlux/journal" if os.geteuid() == 0 else f"/run/winlux/journal-{os.geteuid()}"))
DESKTOP_FILE = "/usr/share/applications/winlux-assistant.desktop"
MODEL = os.environ.get("WINLUX_ASSISTANT_MODEL", "sonnet")
FIRST_PLUGIN = 30  # 1 à 11 sont livrés ou réservés (11 = Widgets) : loin d'eux.
PANEL_AFTER = 8  # séparateur qui pousse les icônes à droite : le bouton vient juste après.
FLOAT_CORNER = 45  # barre flottante : séparateur gardant le coin droit de la capsule libre (#889)
NPM_REGISTRY = os.environ.get("WINLUX_NPM_REGISTRY", "https://registry.npmjs.org").rstrip("/")
# Révisions figées, empreinte SHA-256 écrite ici (#911) : jamais « la dernière
# publiée » (« latest » de llama.cpp, v0.9.0, n'avait même aucun binaire).
# Relevées le 2026-11-08 sur le portable de test (archive téléchargée, empreinte
# recalculée, RUNPATH $ORIGIN, besoins système : libstdc++, libgomp1, libssl3).
LLAMA_TAG = "b12202"
LLAMA_RELEASES = f"https://github.com/ggml-org/llama.cpp/releases/download/{LLAMA_TAG}"
# GitHub répond parfois 504 sur l'adresse de téléchargement : l'API des fichiers
# (même fichier, même empreinte) sert de secours.
LLAMA_ASSET_API = f"https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/{LLAMA_TAG}"
# Moteur selon le PC : processeur seul, ou carte graphique par Vulkan (#928).
LLAMA_ENGINES = {
    "cpu": (f"llama-{LLAMA_TAG}-bin-ubuntu-x64.tar.gz",
            "9076cf9b2ce5b849384711e85b0c37d18c36b1475106c236941de2b6f0899a88", 23066943),
    "vulkan": (f"llama-{LLAMA_TAG}-bin-ubuntu-vulkan-x64.tar.gz",
               "c1eaf8fcaf0b17c1fe9097f7ff08bad484cd451533e7f2075381d18d220694e7", 35880958),
}
# Navigateurs livrés par le catalogue Winlux (winlux-browsers) ou installés à part.
BROWSERS_DIR = Path(os.environ.get("WINLUX_BROWSERS_DIR", "/opt/winlux/browsers"))
BROWSER_BINARIES = ("edge/root/opt/microsoft/msedge/microsoft-edge", "chrome/root/opt/google/chrome/google-chrome",
                    "firefox/root/usr/lib/firefox/firefox")
BROWSER_COMMANDS = ("firefox", "firefox-esr", "chromium", "chromium-browser", "google-chrome-stable",
                    "microsoft-edge-stable", "brave-browser", "epiphany", "falkon")
CODEX_VERSION = "0.167.0"  # relevée le 2026-11-08 (registre npm, empreinte recalculée)
CODEX_SHA512 = {
    "linux-x64": "FNc7danfUng9Sp6w+Fh7kofNHHZpA6VMyNKWsWrS3OwHly69gywTWeiuwrUPXpJSho8Df4xqRsHfHRPzj7zYQg==",
    "linux-arm64": "5cv+sybRWqpRJL6KCv3YSnIx1GHRmjek997C38n1Zfno4HomKDLkNSjWLDOOc2+ISlVNSRRnySWq4zKmzlY12g==",
}
HF_BASE = os.environ.get("WINLUX_HF_BASE", "https://huggingface.co").rstrip("/")
# Modèles Qwen2.5 Instruct Q4_K_M (Apache 2.0), du plus léger au plus capable :
# révision et empreinte figées. Le 7B est en deux morceaux (llama.cpp les lit
# à partir du premier).
LOCAL_MODELS = {
    "1.5b": {"name": "Qwen2.5 1,5B", "repo": "Qwen/Qwen2.5-1.5B-Instruct-GGUF",
             "revision": "91cad51170dc346986eccefdc2dd33a9da36ead9",
             "files": (("qwen2.5-1.5b-instruct-q4_k_m.gguf",
                        "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e", 1117320736),)},
    "3b": {"name": "Qwen2.5 3B", "repo": "Qwen/Qwen2.5-3B-Instruct-GGUF",
           "revision": "7dabda4d13d513e3e842b20f0d435c732f172cbe",
           "files": (("qwen2.5-3b-instruct-q4_k_m.gguf",
                      "626b4a6678b86442240e33df819e00132d3ba7dddfe1cdc4fbb18e0a9615c62d", 2104932768),)},
    "7b": {"name": "Qwen2.5 7B", "repo": "Qwen/Qwen2.5-7B-Instruct-GGUF",
           "revision": "bb5d59e06d9551d752d08b292a50eb208b07ab1f",
           "files": (("qwen2.5-7b-instruct-q4_k_m-00001-of-00002.gguf",
                      "dfce12e3862a5283ccfb88221b48480e58745165de856439950d0f22590580db", 3993201344),
                     ("qwen2.5-7b-instruct-q4_k_m-00002-of-00002.gguf",
                      "539cf93f78e887edea1c04e2d7d8cdaca9d01dae9c9025bcb8accbe29df3d72a", 689872288))},
}
LOCAL_MIN_RAM_GIB = 3  # modèle 1,1 Go + contexte : au-dessous, l'option est cachée
LOCAL_IDLE_SECONDS = 120  # sans question pendant 2 minutes : le moteur s'arrête, mémoire rendue
MAX_AGENTS = 3


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
LANGUAGE_NAMES = {"fr": "français", "en": "English", "es": "español", "de": "Deutsch"}

# Texte français -> (anglais, espagnol, allemand).
_TR = {
    "Télécharger Claude Code (environ 300 Mo)": ("Download Claude Code (about 300 MB)",
                                                 "Descargar Claude Code (unos 300 MB)",
                                                 "Claude Code herunterladen (etwa 300 MB)"),
    "Ajouter le bouton à la barre des tâches": ("Add the button to the taskbar", "Agregar el botón a la barra de tareas",
                                                "Schaltfläche zur Taskleiste hinzufügen"),
    "Vérification de la connexion…": ("Checking the sign-in…", "Comprobando el inicio de sesión…",
                                      "Anmeldung wird geprüft…"),
    "Connectez-vous dans la page qui vient de s'ouvrir (Google, e-mail…).":
        ("Sign in on the page that just opened (Google, email…).",
         "Inicie sesión en la página que se acaba de abrir (Google, correo…).",
         "Melden Sie sich auf der soeben geöffneten Seite an (Google, E-Mail…)."),
    "Si la page affiche un code, collez-le ici :": ("If the page shows a code, paste it here:",
                                                   "Si la página muestra un código, péguelo aquí:",
                                                   "Wenn die Seite einen Code anzeigt, fügen Sie ihn hier ein:"),
    "Valider": ("Confirm", "Confirmar", "Bestätigen"),
    "Aucun navigateur n'est installé sur ce PC. Ouvrez ce lien sur votre téléphone ou un autre ordinateur, "
    "connectez-vous, puis collez ici le code affiché.":
        ("No web browser is installed on this PC. Open this link on your phone or another computer, "
         "sign in, then paste the code shown here.",
         "No hay ningún navegador instalado en este PC. Abra este enlace en su teléfono u otro ordenador, "
         "inicie sesión y pegue aquí el código que aparece.",
         "Auf diesem PC ist kein Browser installiert. Öffnen Sie diesen Link auf Ihrem Telefon oder einem anderen "
         "Computer, melden Sie sich an und fügen Sie den angezeigten Code hier ein."),
    "Aucun navigateur n'est installé sur ce PC. Ouvrez ce lien sur votre téléphone ou un autre ordinateur, "
    "connectez-vous, puis saisissez ce code :":
        ("No web browser is installed on this PC. Open this link on your phone or another computer, "
         "sign in, then enter this code:",
         "No hay ningún navegador instalado en este PC. Abra este enlace en su teléfono u otro ordenador, "
         "inicie sesión e introduzca este código:",
         "Auf diesem PC ist kein Browser installiert. Öffnen Sie diesen Link auf Ihrem Telefon oder einem anderen "
         "Computer, melden Sie sich an und geben Sie diesen Code ein:"),
    "Copier le lien": ("Copy the link", "Copiar el enlace", "Link kopieren"),
    "Lien copié.": ("Link copied.", "Enlace copiado.", "Link kopiert."),
    "Installer un navigateur": ("Install a browser", "Instalar un navegador", "Browser installieren"),
    "Le moteur llama.cpp ne peut pas démarrer sur ce PC : {e}":
        ("The llama.cpp engine cannot start on this PC: {e}", "El motor llama.cpp no puede iniciarse en este PC: {e}",
         "Die llama.cpp-Engine kann auf diesem PC nicht starten: {e}"),
    "Ouvrir la page de connexion": ("Open the sign-in page", "Abrir la página de inicio de sesión",
                                    "Anmeldeseite öffnen"),
    "Le système a arrêté la connexion faute de mémoire. Fermez des applications, puis réessayez.":
        ("The system stopped the sign-in because memory ran out. Close some apps, then try again.",
         "El sistema detuvo el inicio de sesión por falta de memoria. Cierre algunas aplicaciones y vuelva a intentarlo.",
         "Das System hat die Anmeldung wegen Speichermangels beendet. Schließen Sie Apps und versuchen Sie es erneut."),
    "Tant que l'activation n'est pas terminée, {name} reste désactivé. Fermer cette fenêtre l'arrête.":
        ("{name} stays off until activation is complete. Closing this window stops it.",
         "{name} seguirá desactivado hasta que termine la activación. Cerrar esta ventana la detiene.",
         "{name} bleibt deaktiviert, bis die Aktivierung abgeschlossen ist. Wenn Sie dieses Fenster schließen, wird sie beendet."),
    "Arrêter l'activation de {name} ?": ("Stop activating {name}?", "¿Detener la activación de {name}?",
                                          "Aktivierung von {name} beenden?"),
    "{name} restera désactivé. Vous pourrez le réactiver plus tard dans Paramètres > Assistant IA.":
        ("{name} will stay off. You can turn it on later in Settings > AI assistant.",
         "{name} seguirá desactivado. Puede activarlo más tarde en Configuración > Asistente de IA.",
         "{name} bleibt deaktiviert. Sie können es später unter Einstellungen > KI-Assistent aktivieren."),
    "Continuer l'activation": ("Continue activation", "Continuar la activación", "Aktivierung fortsetzen"),
    "Arrêter": ("Stop", "Detener", "Beenden"),
    "La connexion n'a pas abouti : {e}": ("Sign-in did not complete: {e}", "No se completó el inicio de sesión: {e}",
                                          "Die Anmeldung wurde nicht abgeschlossen: {e}"),
    "Ouvrir l'assistant": ("Open the assistant", "Abrir el asistente", "Assistent öffnen"),
    "Réessayer": ("Try again", "Reintentar", "Erneut versuchen"),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "Fermer": ("Close", "Cerrar", "Schließen"),
    "Le téléchargement a échoué : {e}": ("The download failed: {e}", "La descarga falló: {e}",
                                         "Der Download ist fehlgeschlagen: {e}"),
    "Posez votre question…": ("Ask your question…", "Haga su pregunta…", "Stellen Sie Ihre Frage…"),
    "Envoyer": ("Send", "Enviar", "Senden"),
    "Bonjour ! Je peux vous aider à utiliser Winlux, par exemple :":
        ("Hello! I can help you use Winlux, for example:", "¡Hola! Puedo ayudarle a usar Winlux, por ejemplo:",
         "Hallo! Ich helfe Ihnen bei Winlux, zum Beispiel:"),
    "Pourquoi mon programme .exe ne s'ouvre pas ?": ("Why won't my .exe program open?",
                                                    "¿Por qué no se abre mi programa .exe?",
                                                    "Warum startet mein .exe-Programm nicht?"),
    "Où est mon fichier… ?": ("Where is my file…?", "¿Dónde está mi archivo…?", "Wo ist meine Datei…?"),
    "Passe au thème sombre": ("Switch to the dark theme", "Cambia al tema oscuro", "Wechsle zum dunklen Design"),
    "Voir exactement ce qui sera envoyé": ("See exactly what will be sent", "Ver exactamente lo que se enviará",
                                           "Genau ansehen, was gesendet wird"),
    "Refuser": ("Decline", "Rechazar", "Ablehnen"),
    "Faire": ("Do it", "Hacerlo", "Ausführen"),
    "Non merci": ("No thanks", "No, gracias", "Nein danke"),
    "Fait.": ("Done.", "Hecho.", "Erledigt."),
    "Refusé : rien n'a été envoyé.": ("Declined: nothing was sent.", "Rechazado: no se envió nada.",
                                     "Abgelehnt: Es wurde nichts gesendet."),
    "Refusé : rien n'a été changé.": ("Declined: nothing was changed.", "Rechazado: no se cambió nada.",
                                     "Abgelehnt: Es wurde nichts geändert."),
    "L'action a échoué : {e}": ("The action failed: {e}", "La acción falló: {e}", "Die Aktion ist fehlgeschlagen: {e}"),
    "Terminer la configuration": ("Finish setting up", "Terminar la configuración", "Einrichtung abschließen"),
    "Nouvelle conversation": ("New conversation", "Nueva conversación", "Neues Gespräch"),
    # Lectures et actions proposées par Claude.
    "le journal du programme {name}": ("the log of the program {name}", "el registro del programa {name}",
                                       "das Protokoll des Programms {name}"),
    "les dernières erreurs du journal Winlux": ("the latest errors in the Winlux log",
                                                "los últimos errores del registro de Winlux",
                                                "die letzten Fehler im Winlux-Protokoll"),
    "les derniers plantages de programmes": ("the latest program crashes", "los últimos bloqueos de programas",
                                             "die letzten Programmabstürze"),
    "la description de ce PC (version, processeur, mémoire)": ("a description of this PC (version, processor, memory)",
                                                               "la descripción de este equipo (versión, procesador, memoria)",
                                                               "eine Beschreibung dieses PCs (Version, Prozessor, Speicher)"),
    "les noms des fichiers de votre dossier personnel qui contiennent « {name} »":
        ("the names of the files in your home folder that contain “{name}”",
         "los nombres de los archivos de su carpeta personal que contienen «{name}»",
         "die Namen der Dateien in Ihrem persönlichen Ordner, die „{name}“ enthalten"),
    "(aucune ligne trouvée)": ("(no line found)", "(no se encontró ninguna línea)", "(keine Zeile gefunden)"),
    "(aucun fichier trouvé)": ("(no file found)", "(no se encontró ningún archivo)", "(keine Datei gefunden)"),
    "ouvrir Paramètres, page {page}": ("open Settings, page {page}", "abrir Configuración, página {page}",
                                       "Einstellungen öffnen, Seite {page}"),
    "ouvrir le dossier {path}": ("open the folder {path}", "abrir la carpeta {path}", "den Ordner {path} öffnen"),
    "Thème sombre": ("Dark theme", "Tema oscuro", "Dunkles Design"),
    "Thème clair": ("Light theme", "Tema claro", "Helles Design"),
    "Barre des tâches centrée": ("Centered taskbar", "Barra de tareas centrada", "Zentrierte Taskleiste"),
    "Barre des tâches à gauche": ("Left-aligned taskbar", "Barra de tareas a la izquierda", "Taskleiste links"),
    "Monter le son": ("Turn the volume up", "Subir el volumen", "Lauter"),
    "Baisser le son": ("Turn the volume down", "Bajar el volumen", "Leiser"),
    "Couper ou remettre le son": ("Mute or unmute", "Silenciar o reactivar el sonido", "Ton aus- oder einschalten"),
    "Activer le Bluetooth": ("Turn Bluetooth on", "Activar Bluetooth", "Bluetooth einschalten"),
    "Désactiver le Bluetooth": ("Turn Bluetooth off", "Desactivar Bluetooth", "Bluetooth ausschalten"),
    "Renouveler la connexion réseau": ("Renew the network connection", "Renovar la conexión de red",
                                       "Netzwerkverbindung erneuern"),
    "Verrouiller l'écran": ("Lock the screen", "Bloquear la pantalla", "Bildschirm sperren"),
    "Agrandir le texte et les éléments": ("Make text and items bigger", "Agrandar el texto y los elementos",
                                          "Text und Elemente vergrößern"),
    "Réduire le texte et les éléments": ("Make text and items smaller", "Reducir el texto y los elementos",
                                         "Text und Elemente verkleinern"),
    "Échelle d'affichage automatique": ("Automatic display scale", "Escala de pantalla automática",
                                        "Automatische Anzeigeskalierung"),
    "Mode performances": ("Performance mode", "Modo rendimiento", "Leistungsmodus"),
    "Mode équilibré": ("Balanced mode", "Modo equilibrado", "Ausbalancierter Modus"),
    "Mode économie d'énergie": ("Power saving mode", "Modo ahorro de energía", "Energiesparmodus"),
    "Rechercher les mises à jour de Winlux": ("Check for Winlux updates", "Buscar actualizaciones de Winlux",
                                              "Nach Winlux-Updates suchen"),
    # Pages des Paramètres et description du PC, dans la langue choisie.
    "Réseau": ("Network", "Red", "Netzwerk"),
    "Clavier et souris": ('Keyboard & mouse', 'Teclado y mouse', 'Tastatur und Maus'),
    "Mise à jour OS": ("OS update", "Actualización del SO", "OS-Update"),
    "Écran": ("Display", "Pantalla", "Bildschirm"),
    "Programmes Windows": ("Windows programs", "Programas de Windows", "Windows-Programme"),
    "Navigateurs": ("Browsers", "Navegadores", "Browser"),
    "Accessibilité": ("Accessibility", "Accesibilidad", "Barrierefreiheit"),
    "Localisation": ('Language & region', 'Idioma y región', 'Sprache und Region'),
    "Son": ("Sound", "Sonido", "Sound"),
    "Confidentialité et sécurité": ('Privacy & security', 'Privacidad y seguridad', 'Datenschutz und Sicherheit'),
    "Lecteurs réseau": ("Network drives", "Unidades de red", "Netzlaufwerke"),
    "Fond d’écran et couleurs": ('Background & colors', 'Fondo y colores', 'Hintergrund und Farben'),
    "Bluetooth": ("Bluetooth", "Bluetooth", "Bluetooth"),
    "Alimentation et batterie": ('Power & battery', 'Energía y batería', 'Energie und Akku'),
    "Graphismes": ("Graphics", "Gráficos", "Grafik"),
    "Imprimantes et scanners": ("Printers and scanners", "Impresoras y escáneres", "Drucker und Scanner"),
    "Partage et transfert": ("Sharing and transfer", "Compartir y transferir", "Freigabe und Übertragung"),
    "Jeux": ("Games", "Juegos", "Spiele"),
    "Processeur : {model} ({count} cœurs)": ("Processor: {model} ({count} cores)", "Procesador: {model} ({count} núcleos)", "Prozessor: {model} ({count} Kerne)"),
    "Mémoire : {size} Gio": ("Memory: {size} GiB", "Memoria: {size} GiB", "Arbeitsspeicher: {size} GiB"),
    "Carte graphique": ("Graphics card", "Tarjeta gráfica", "Grafikkarte"),
    "Machine virtuelle": ("Virtual machine", "Máquina virtual", "Virtuelle Maschine"),
    # Trois assistants : Claude, ChatGPT, IA locale.
    'Assistant IA': ('AI assistant', 'Asistente de IA', 'KI-Assistent'),
    'IA locale': ('Local AI', 'IA local', 'Lokale KI'),
    "Assistant IA : activer {name}": ('AI assistant: turn on {name}', 'Asistente de IA: activar {name}', 'KI-Assistent: {name} aktivieren'),
    'Annulé.': ('Cancelled.', 'Cancelado.', 'Abgebrochen.'),
    'Archive inattendue : {name}': ('Unexpected archive: {name}', 'Archivo inesperado: {name}', 'Unerwartetes Archiv: {name}'),
    "C'est prêt ! Cliquez sur le bouton de l'assistant dans la barre des tâches ou appuyez sur Windows+C.": ('All set! Click the assistant button on the taskbar or press Windows+C.', '¡Listo! Haga clic en el botón del asistente en la barra de tareas o pulse Windows+C.', 'Fertig! Klicken Sie auf die Assistent-Schaltfläche in der Taskleiste oder drücken Sie Windows+C.'),
    'Connecté au compte {name}{who}.': ('Signed in to the {name} account{who}.', 'Sesión iniciada en la cuenta de {name}{who}.', 'Beim {name}-Konto angemeldet{who}.'),
    "Go": ("GB", "GB", "GB"),
    "Mo": ("MB", "MB", "MB"),
    "Qwen2.5 1,5B": ("Qwen2.5 1.5B", "Qwen2.5 1,5B", "Qwen2.5 1,5B"),
    "Qwen2.5 3B": ("Qwen2.5 3B", "Qwen2.5 3B", "Qwen2.5 3B"),
    "Qwen2.5 7B": ("Qwen2.5 7B", "Qwen2.5 7B", "Qwen2.5 7B"),
    "Pas encore téléchargé": ("Not downloaded yet", "Aún no descargado", "Noch nicht heruntergeladen"),
    "sur la carte graphique {gpu}": ("on the {gpu} graphics card", "en la tarjeta gráfica {gpu}", "auf der Grafikkarte {gpu}"),
    "sur la carte graphique": ("on the graphics card", "en la tarjeta gráfica", "auf der Grafikkarte"),
    "sur le processeur": ("on the processor", "en el procesador", "auf dem Prozessor"),
    " de la carte graphique": (" of graphics card memory", " de la tarjeta gráfica", " der Grafikkarte"),
    " et la carte graphique": (" and the graphics card", " y la tarjeta gráfica", " und die Grafikkarte"),
    "Télécharger le moteur llama.cpp (environ {size})": ("Download the llama.cpp engine (about {size})",
                                                         "Descargar el motor llama.cpp (unos {size})",
                                                         "Die llama.cpp-Engine herunterladen (etwa {size})"),
    "Télécharger le modèle {name} (environ {size})": ("Download the {name} model (about {size})",
                                                      "Descargar el modelo {name} (unos {size})",
                                                      "Das Modell {name} herunterladen (etwa {size})"),
    "Choisi pour ce PC : {model} ({size}), {where}. Pendant que vous lui écrivez, elle prend environ "
    "{memory} de mémoire{gpu_memory} et fait travailler l'ordinateur : ce n'est pas l'idéal sur un PC "
    "déjà occupé. Elle s'arrête seule deux minutes après votre dernière question ; désactivée, elle ne "
    "fait strictement rien.":
        ("Chosen for this PC: {model} ({size}), {where}. While you are writing to it, it takes about {memory}"
         "{gpu_memory} and keeps the computer busy, which is not ideal on a PC that is already working. It stops "
         "on its own two minutes after your last question; when turned off, it does nothing at all.",
         "Elegido para este equipo: {model} ({size}), {where}. Mientras le escribe, ocupa unos {memory} de "
         "memoria{gpu_memory} y hace trabajar al ordenador: no es lo ideal en un equipo ya ocupado. Se detiene "
         "sola dos minutos después de su última pregunta; desactivada, no hace absolutamente nada.",
         "Für diesen PC gewählt: {model} ({size}), {where}. Während Sie ihr schreiben, belegt sie etwa {memory} "
         "Speicher{gpu_memory} und lässt den Computer arbeiten – nicht ideal auf einem PC, der schon ausgelastet "
         "ist. Sie hält zwei Minuten nach Ihrer letzten Frage von selbst an; ausgeschaltet tut sie gar nichts."),
    "IA locale en marche : elle prend {memory} de mémoire{gpu} et fait travailler l'ordinateur. Elle "
    "s'arrête seule deux minutes après votre dernière question.":
        ("Local AI running: it takes {memory} of memory{gpu} and keeps the computer busy. It stops on its own "
         "two minutes after your last question.",
         "IA local en marcha: ocupa {memory} de memoria{gpu} y hace trabajar al ordenador. Se detiene sola dos "
         "minutos después de su última pregunta.",
         "Lokale KI läuft: Sie belegt {memory} Speicher{gpu} und lässt den Computer arbeiten. Sie hält zwei "
         "Minuten nach Ihrer letzten Frage von selbst an."),
    "IA locale en pause : elle ne prend ni mémoire ni processeur. Elle redémarre à votre prochaine "
    "question. Rien n'est envoyé sur Internet.":
        ("Local AI paused: it uses no memory and no processor. It starts again at your next question. "
         "Nothing is sent over the Internet.",
         "IA local en pausa: no ocupa memoria ni procesador. Vuelve a arrancar con su próxima pregunta. "
         "No se envía nada por Internet.",
         "Lokale KI pausiert: Sie belegt weder Speicher noch Prozessor. Sie startet bei Ihrer nächsten Frage "
         "neu. Nichts wird ins Internet gesendet."),
    "Pause : l'IA locale s'est arrêtée pour rendre la mémoire. Elle redémarre à votre prochaine question.":
        ("Paused: the local AI stopped to free the memory. It starts again at your next question.",
         "Pausa: la IA local se detuvo para liberar la memoria. Vuelve a arrancar con su próxima pregunta.",
         "Pause: Die lokale KI hat angehalten, um den Speicher freizugeben. Sie startet bei Ihrer nächsten "
         "Frage neu."),
    "Démarrage de l'IA locale…": ('Starting the local AI…', 'Iniciando la IA local…', 'Lokale KI wird gestartet…'),
    'Envoyer à {name}': ('Send to {name}', 'Enviar a {name}', 'An {name} senden'),
    "Envoyé à {name} ({company}) : votre question et cette conversation. Rien d'autre sans votre accord.": ('Sent to {name} ({company}): your question and this conversation. Nothing else without your consent.', 'Se envía a {name} ({company}): su pregunta y esta conversación. Nada más sin su permiso.', 'An {name} ({company}) gesendet: Ihre Frage und dieses Gespräch. Nichts anderes ohne Ihre Zustimmung.'),
    'Fichier corrompu, empreinte incorrecte : {name}': ('Corrupted file, wrong checksum: {name}', 'Archivo dañado, suma de comprobación incorrecta: {name}', 'Beschädigte Datei, falsche Prüfsumme: {name}'),
    "IA locale : tout reste sur ce PC, rien n'est envoyé sur Internet.": ('Local AI: everything stays on this PC, nothing is sent over the Internet.', 'IA local: todo se queda en este equipo, no se envía nada por Internet.', 'Lokale KI: Alles bleibt auf diesem PC, nichts wird ins Internet gesendet.'),
    "J'ai trouvé {count} résultat(s) pour « {name} ». Voici les premiers :": ('I found {count} result(s) for “{name}”. Here are the first ones:', 'He encontrado {count} resultado(s) para «{name}». Estos son los primeros:', 'Ich habe {count} Ergebnis(se) für „{name}“ gefunden. Hier die ersten:'),
    "Je n'ai trouvé aucun fichier dont le nom contient « {name} ».": ('I found no file whose name contains “{name}”.', 'No encontré ningún archivo cuyo nombre contenga «{name}».', 'Ich habe keine Datei gefunden, deren Name „{name}“ enthält.'),
    'Je ne sais pas faire ce réglage. Essayez dans Paramètres.': ("I can't change that setting. Try in Settings.", 'No sé cambiar ese ajuste. Pruebe en Configuración.', 'Diese Einstellung kann ich nicht ändern. Versuchen Sie es in den Einstellungen.'),
    'Je ne sais pas.': ("I don't know.", 'No lo sé.', 'Das weiß ich nicht.'),
    'Je peux le faire pour vous : cliquez sur « Faire ».': ('I can do it for you: click “Do it”.', 'Puedo hacerlo por usted: haga clic en «Hacerlo».', 'Ich kann das für Sie erledigen: Klicken Sie auf „Ausführen“.'),
    'Je peux ouvrir ce dossier :': ('I can open this folder:', 'Puedo abrir esta carpeta:', 'Ich kann diesen Ordner öffnen:'),
    "L'IA locale est installée sur ce PC : rien de ce que vous lui demandez ne quitte l'ordinateur.": ('The local AI is installed on this PC: nothing you ask it leaves the computer.', 'La IA local está instalada en este equipo: nada de lo que le pida sale del ordenador.', 'Die lokale KI ist auf diesem PC installiert: Nichts, was Sie sie fragen, verlässt den Computer.'),
    "L'IA locale n'a pas démarré : {e}": ('The local AI did not start: {e}', 'La IA local no se inició: {e}', 'Die lokale KI ist nicht gestartet: {e}'),
    "L'IA locale n'est pas installée.": ('The local AI is not installed.', 'La IA local no está instalada.', 'Die lokale KI ist nicht installiert.'),
    "L'assistant est désactivé. Activez-le dans Paramètres.": ('The assistant is turned off. Turn it on in Settings.', 'El asistente está desactivado. Actívelo en Configuración.', 'Der Assistent ist deaktiviert. Aktivieren Sie ihn in den Einstellungen.'),
    "Le moteur de l'IA locale ne répond pas.": ('The local AI engine is not responding.', 'El motor de la IA local no responde.', 'Die Engine der lokalen KI antwortet nicht.'),
    "Le moteur de l'IA locale s'est arrêté (code {code}).": ('The local AI engine stopped (code {code}).', 'El motor de la IA local se detuvo (código {code}).', 'Die Engine der lokalen KI wurde beendet (Code {code}).'),
    "Lu sur ce PC (rien n'est envoyé) : {what}": ('Read on this PC (nothing is sent): {what}', 'Leído en este equipo (no se envía nada): {what}', 'Auf diesem PC gelesen (nichts wird gesendet): {what}'),
    'Quel fichier cherchez-vous ? Donnez une partie de son nom.': ('Which file are you looking for? Give part of its name.', '¿Qué archivo busca? Indique parte de su nombre.', 'Welche Datei suchen Sie? Nennen Sie einen Teil ihres Namens.'),
    "Réponse refusée : ChatGPT a tenté d'utiliser un outil.": ('Answer rejected: ChatGPT tried to use a tool.', 'Respuesta rechazada: ChatGPT intentó usar una herramienta.', 'Antwort abgelehnt: ChatGPT hat versucht, ein Werkzeug zu verwenden.'),
    'Se connecter à votre compte {name}': ('Sign in to your {name} account', 'Iniciar sesión en su cuenta de {name}', 'Bei Ihrem {name}-Konto anmelden'),
    'Téléchargement de Codex': ('Downloading Codex', 'Descargando Codex', 'Codex wird heruntergeladen'),
    'Téléchargement du modèle': ('Downloading the model', 'Descargando el modelo', 'Modell wird heruntergeladen'),
    'Téléchargement du moteur': ('Downloading the engine', 'Descargando el motor', 'Engine wird heruntergeladen'),
    "Télécharger Codex, l'outil officiel d'OpenAI (environ 170 Mo)": ("Download Codex, OpenAI's official tool (about 170 MB)", 'Descargar Codex, la herramienta oficial de OpenAI (unos 170 MB)', 'Codex herunterladen, das offizielle Werkzeug von OpenAI (etwa 170 MB)'),
    'Voir ce qui a été lu': ('See what was read', 'Ver lo que se leyó', 'Ansehen, was gelesen wurde'),
    "Vous n'êtes pas connecté à votre compte {name}.": ('You are not signed in to your {name} account.', 'No ha iniciado sesión en su cuenta de {name}.', 'Sie sind nicht bei Ihrem {name}-Konto angemeldet.'),
    "ouvrir l'emplacement de {path}": ('open the location of {path}', 'abrir la ubicación de {path}', 'den Speicherort von {path} öffnen'),
    'recherche de « {name} » dans votre dossier personnel': ('search for “{name}” in your home folder', 'búsqueda de «{name}» en su carpeta personal', 'Suche nach „{name}“ in Ihrem persönlichen Ordner'),
    '{name} demande à lire : {what}': ('{name} asks to read: {what}', '{name} pide leer: {what}', '{name} möchte lesen: {what}'),
    "{name} n'a pas répondu : {e}": ('{name} did not answer: {e}', '{name} no respondió: {e}', '{name} hat nicht geantwortet: {e}'),
    '{name} propose : {what}': ('{name} suggests: {what}', '{name} propone: {what}', '{name} schlägt vor: {what}'),
    '{name} réfléchit…': ('{name} is thinking…', '{name} está pensando…', '{name} denkt nach…'),
}


def _t(text, **kw):
    row = _TR.get(text)
    out = row[_COL[_LANG]] if row and _LANG in _COL else text
    return out.format(**kw) if kw else out


_LIGATURES = str.maketrans({"œ": "oe", "æ": "ae", "ø": "o", "đ": "d", "ł": "l", "ı": "i"})


def fold(text):
    """Recherche sans casse ni accents (même règle que les Paramètres)."""
    text = unicodedata.normalize("NFKD", text.casefold().translate(_LIGATURES))
    return "".join(c for c in text if not unicodedata.combining(c))


# ── Réglages (assistant.conf, lisible par le seul propriétaire) ─────────────

def read_config():
    try:
        lines = CONFIG.read_text(encoding="utf-8").splitlines()
    except OSError:
        return {}
    values = dict(line.split("=", 1) for line in lines if "=" in line)
    # Avant les trois assistants (#298), « enabled » ne valait que pour Claude.
    if "enabled" in values:
        values.setdefault("enabled_claude", values.pop("enabled"))
    return values


def write_config(**changes):
    values = read_config()
    for key, value in changes.items():
        if value is None:
            values.pop(key, None)
        else:
            values[key] = str(value)
    CONFIG.parent.mkdir(parents=True, exist_ok=True)
    temporary = CONFIG.with_suffix(".tmp")
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as out:
        out.write("".join(f"{key}={value}\n" for key, value in sorted(values.items())))
    os.chmod(temporary, 0o600)
    temporary.replace(CONFIG)


def enabled_providers():
    values = read_config()
    return [key for key in PROVIDERS if values.get(f"enabled_{key}") == "1" and provider(key).supported()]


def is_enabled(key=None):
    enabled = enabled_providers()
    return bool(enabled) if key is None else key in enabled


def current_provider():
    """Assistant ouvert par le bouton : le dernier choisi, sinon le premier activé."""
    enabled = enabled_providers()
    chosen = read_config().get("current")
    return chosen if chosen in enabled else (enabled[0] if enabled else None)


def _mem_total_gib():
    try:
        return next(int(line.split()[1]) for line in Path("/proc/meminfo").read_text().splitlines()
                    if line.startswith("MemTotal")) / 1048576
    except (OSError, StopIteration, ValueError):
        return 0.0


def _machine():
    return platform.machine().lower()


# ── Téléchargements vérifiés (rien de tout cela n'est dans l'ISO) ───────────

def _get(url, timeout=30, accept="application/json, */*"):
    request = urllib.request.Request(url, headers={"User-Agent": "Winlux-assistant", "Accept": accept})
    return urllib.request.urlopen(request, timeout=timeout)


def download(url, target, algorithm, expected, progress=None, cancel=None, label="", accept="*/*"):
    """Télécharge `url` dans `target` ; le fichier n'est gardé que si son
    empreinte `algorithm` (sha256, sha512) vaut exactement `expected` (octets)."""
    target.parent.mkdir(parents=True, exist_ok=True)
    partial = target.with_name(target.name + ".part")
    digest = hashlib.new(algorithm)
    try:
        with _get(url, timeout=60, accept=accept) as response, open(partial, "wb") as out:
            total = int(response.headers.get("Content-Length") or 0)
            done = 0
            while True:
                if cancel is not None and cancel.is_set():
                    raise RuntimeError(_t("Annulé."))
                chunk = response.read(1 << 20)
                if not chunk:
                    break
                out.write(chunk)
                digest.update(chunk)
                done += len(chunk)
                if progress:
                    size = f"{done >> 20} Mo" + (f" / {total >> 20} Mo" if total else "")
                    progress(done / total if total else None, f"{label} : {size}")
        if not expected or digest.digest() != expected:
            raise RuntimeError(_t("Fichier corrompu, empreinte incorrecte : {name}", name=target.name))
        partial.replace(target)
    finally:
        partial.unlink(missing_ok=True)
    return target


def _replace_dir(staging, final):
    """Remplace `final` par `staging` (installation précédente supprimée après)."""
    old = final.with_name(final.name + ".ancien")
    shutil.rmtree(old, ignore_errors=True)
    if final.exists():
        final.rename(old)
    staging.rename(final)
    shutil.rmtree(old, ignore_errors=True)


# ── Assistants : Claude (Anthropic), ChatGPT (OpenAI), IA locale ────────────

class NotSignedIn(Exception):
    pass


class Provider:
    key = ""
    name = ""
    remote = True  # False : rien ne quitte ce PC
    open_login_page = True  # ouvrir nous-mêmes la page de connexion
    code_entry = False  # champ pour coller un code de connexion

    def supported(self):
        return True

    def steps(self):
        return ()

    def launcher(self):
        return None

    def install(self, progress, cancel, mark):
        raise NotImplementedError

    def status(self, launcher=None):
        return {"installed": self.launcher() is not None, "loggedIn": False}

    def login_command(self, launcher, browser=True):
        return None

    def logout(self):
        pass

    def protect(self):
        return []

    def env(self):
        return dict(os.environ)


class ClaudeProvider(Provider):
    """claude.exe à travers la couche PE (même installation que le menu Démarrer)."""
    key, name = "claude", "Claude"
    code_entry = True

    def steps(self):
        return (_t("Télécharger Claude Code (environ 300 Mo)"), _t("Se connecter à votre compte {name}", name=self.name),
                _t("Ajouter le bouton à la barre des tâches"))

    def launcher(self):
        return claude_launcher()

    def install(self, progress, cancel, mark):
        app = claude_code_app()
        if app is None:
            raise RuntimeError("winlux-claude-code.py")
        launcher = app.install(progress, cancel)
        mark(0)
        return launcher

    def status(self, launcher=None):
        return auth_status(launcher)

    def login_command(self, launcher, browser=True):
        # Sans navigateur, la page de connexion s'ouvre sur un autre appareil et
        # son code se colle ici : la page de retour est celle d'Anthropic.
        return [str(launcher), "auth", "login", "--claudeai"]

    def logout(self):
        launcher = self.launcher()
        if launcher:
            subprocess.run([str(launcher), "auth", "logout"], capture_output=True, timeout=60, env=claude_env(),
                           stdin=subprocess.DEVNULL, cwd=work_dir())

    def protect(self):
        return protect_credentials()

    def env(self):
        return claude_env()

    def command(self, name):
        return [str(claude_launcher()), "-p", "--output-format", "json", "--tools", "",
                "--strict-mcp-config", "--disable-slash-commands", "--setting-sources", "",
                "--no-session-persistence", "--model", MODEL, "--system-prompt", system_prompt(name),
                "--json-schema", json.dumps(SCHEMA, ensure_ascii=False)]

    def ask(self, prompt, timeout=240):
        """La conversation part par l'entrée standard, jamais en argument :
        `ps` la montrerait aux autres comptes."""
        result = subprocess.run(self.command(self.name), input=prompt, capture_output=True, text=True,
                                timeout=timeout, cwd=work_dir(), env=claude_env())
        output = result.stdout.strip()
        try:
            data = json.loads(output[output.find("{"):]) if "{" in output else {}
        except ValueError:
            data = {}
        message = str(data.get("result") or result.stderr or output).strip()
        if data.get("is_error") or result.returncode or not data:
            if re.search(r"log ?in|not logged|authenticat|/login|oauth", message, re.I):
                raise NotSignedIn(message)
            raise RuntimeError(message[-400:] or f"code {result.returncode}")
        answer = data.get("structured_output")
        if not isinstance(answer, dict):
            try:
                answer = json.loads(data.get("result") or "")
            except ValueError:
                answer = {"reponse": data.get("result") or "", "demande": "aucune"}
        return answer


class ChatGPTProvider(Provider):
    """Codex, l'outil officiel d'OpenAI (programme Linux natif) : il se connecte
    au compte ChatGPT de l'utilisateur. Téléchargé depuis npm, empreinte
    SHA-512 du registre vérifiée ; rien dans l'ISO."""
    key, name = "chatgpt", "ChatGPT"
    open_login_page = False  # « codex login » ouvre lui-même le navigateur
    TARGETS = {"x86_64": ("linux-x64", "x86_64-unknown-linux-musl"),
               "aarch64": ("linux-arm64", "aarch64-unknown-linux-musl")}

    def supported(self):
        return _machine() in self.TARGETS

    def steps(self):
        return (_t("Télécharger Codex, l'outil officiel d'OpenAI (environ 170 Mo)"),
                _t("Se connecter à votre compte {name}", name=self.name), _t("Ajouter le bouton à la barre des tâches"))

    def home(self):
        return CODEX_ACCOUNT

    def launcher(self):
        forced = os.environ.get("WINLUX_ASSISTANT_CODEX")
        if forced:
            return Path(forced) if Path(forced).is_file() else None
        path = CODEX_DIR / "bin/codex"
        return path if path.is_file() and os.access(path, os.X_OK) else None

    def install(self, progress, cancel, mark):
        suffix, triple = self.TARGETS[_machine()]
        # Version figée et empreinte SHA-512 écrites ici, comme llama.cpp (#911).
        version = CODEX_VERSION
        tarball = f"{NPM_REGISTRY}/@openai/codex/-/codex-{version}-{suffix}.tgz"
        expected = base64.b64decode(CODEX_SHA512[suffix])
        CODEX_DIR.parent.mkdir(parents=True, exist_ok=True)
        archive = download(tarball, CODEX_DIR.with_name("codex.tgz"), "sha512", expected, progress, cancel,
                           _t("Téléchargement de Codex"))
        staging = CODEX_DIR.with_name("codex.nouveau")
        shutil.rmtree(staging, ignore_errors=True)
        prefix = f"package/vendor/{triple}/"
        try:
            with tarfile.open(archive) as tar:
                members = []
                for member in tar.getmembers():
                    if not member.name.startswith(prefix) or "/codex-resources/voice/" in member.name:
                        continue
                    member.name = member.name[len(prefix):]
                    if member.name:
                        members.append(member)
                tar.extractall(staging, members=members, filter="data")
            if not (staging / "bin/codex").is_file():
                raise RuntimeError(_t("Archive inattendue : {name}", name="codex"))
            (staging / "bin/codex").chmod(0o755)
            (staging / "VERSION").write_text(version + "\n")
            _replace_dir(staging, CODEX_DIR)
        finally:
            archive.unlink(missing_ok=True)
            shutil.rmtree(staging, ignore_errors=True)
        mark(0)
        return self.launcher()

    def env(self):
        env = dict(os.environ)
        env.pop("OPENAI_API_KEY", None)  # le compte ChatGPT, pas une clé d'API
        _private_dir(CODEX_ACCOUNT)
        env["CODEX_HOME"] = str(CODEX_ACCOUNT)  # jamais ~/.codex
        env["PATH"] = f"{CODEX_DIR / 'codex-path'}:{env.get('PATH', '/usr/bin:/bin')}"
        return env

    def status(self, launcher=None):
        launcher = launcher or self.launcher()
        if not launcher:
            return {"installed": False, "loggedIn": False}
        try:
            result = subprocess.run([str(launcher), "login", "status"], capture_output=True, text=True, timeout=30,
                                    env=self.env(), stdin=subprocess.DEVNULL, cwd=work_dir())
            out = result.stdout + result.stderr
            logged = result.returncode == 0 and "Logged in" in out
        except (OSError, subprocess.SubprocessError):
            out, logged = "", False
        return {"installed": True, "loggedIn": logged, "account": "ChatGPT" if logged and "ChatGPT" in out else ""}

    def login_command(self, launcher, browser=True):
        # Sans navigateur sur ce PC, le retour sur 127.0.0.1:1455 est impossible :
        # connexion par code à saisir sur un autre appareil (#912).
        return [str(launcher), "login"] if browser else [str(launcher), "login", "--device-auth"]

    def logout(self):
        launcher = self.launcher()
        if launcher:
            subprocess.run([str(launcher), "logout"], capture_output=True, timeout=60, env=self.env(),
                           stdin=subprocess.DEVNULL, cwd=work_dir())

    def protect(self):
        """Jeton du compte ChatGPT (auth.json) lisible par le seul propriétaire."""
        base, fixed = self.home(), []
        for path, mode in ((base, 0o700), (base / "auth.json", 0o600)):
            try:
                if path.stat().st_mode & 0o077:
                    path.chmod(mode)
                    fixed.append(path)
            except OSError:
                pass
        return fixed

    def command(self, schema_file, out_file):
        """Aucun outil : ni commandes (shell), ni recherche web, ni fichiers de
        réglages de l'utilisateur (serveurs MCP), bac à sable en lecture seule
        dans un dossier vide, rien de gardé (« --ephemeral »)."""
        return [str(self.launcher()), "exec", "--json", "--ephemeral", "--ignore-user-config",
                "--skip-git-repo-check", "--sandbox", "read-only", "-C", str(WORK),
                "--output-schema", str(schema_file), "-o", str(out_file),
                "-c", "features.shell_tool=false", "-c", "features.unified_exec=false",
                "-c", "features.apply_patch_freeform=false", "-c", "features.view_image_tool=false",
                "-c", 'web_search="disabled"', "-c", "analytics.enabled=false",
                "-c", "check_for_update_on_startup=false", "-"]

    def ask(self, prompt, timeout=240):
        schema_file, out_file = WORK / "schema.json", WORK / "reponse.txt"
        schema_file.write_text(json.dumps(SCHEMA, ensure_ascii=False), encoding="utf-8")
        out_file.unlink(missing_ok=True)
        try:
            result = subprocess.run(self.command(schema_file, out_file),
                                    input=system_prompt(self.name) + "\n\n" + prompt,
                                    capture_output=True, text=True, timeout=timeout, cwd=work_dir(), env=self.env())
            text = out_file.read_text(encoding="utf-8") if out_file.is_file() else ""
        finally:
            out_file.unlink(missing_ok=True)
        errors, last = [], ""
        for line in result.stdout.splitlines():
            try:
                event = json.loads(line)
            except ValueError:
                continue
            item = event.get("item") or {}
            if item.get("type") in ("command_execution", "file_change", "mcp_tool_call", "web_search"):
                # Ne devrait jamais arriver (outils coupés) : la réponse est refusée.
                raise RuntimeError(_t("Réponse refusée : ChatGPT a tenté d'utiliser un outil."))
            if item.get("type") == "agent_message":
                last = item.get("text") or last
            if event.get("type") in ("error", "turn.failed"):
                errors.append(str(event.get("message") or (event.get("error") or {}).get("message") or ""))
        text = (text or last).strip()
        if result.returncode or not text:
            message = " ".join(errors + [result.stderr.strip()]).strip()
            if re.search(r"log ?in|not logged|unauthori[sz]ed|401|auth", message, re.I):
                raise NotSignedIn(message)
            raise RuntimeError(message[-400:] or f"code {result.returncode}")
        try:
            answer = json.loads(text[text.find("{"):text.rindex("}") + 1])
        except ValueError:
            answer = {"reponse": text, "demande": "aucune"}
        return answer if isinstance(answer, dict) else {"reponse": str(answer), "demande": "aucune"}


def engine_problem(server):
    """Bibliothèque système manquante au moteur : dit tout de suite, plutôt qu'à
    l'ouverture du panneau. Chaîne vide si llama-server démarre."""
    env = dict(os.environ)
    env["LD_LIBRARY_PATH"] = ":".join(filter(None, (str(server.parent), env.get("LD_LIBRARY_PATH"))))
    try:
        result = subprocess.run([str(server), "--version"], capture_output=True, text=True, timeout=30, env=env,
                                stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return ""
    except OSError as error:
        return str(error)
    text = result.stdout + result.stderr
    found = re.search(r"error while loading shared libraries: ([^:\s]+)", text)
    return found.group(1) if found else ""


# ── IA locale adaptée au PC : mémoire, carte graphique (#928) ─────────────────

DRM_DIR = Path(os.environ.get("WINLUX_DRM_DIR", "/sys/class/drm"))
VULKAN_ICD_DIRS = tuple(Path(p) for p in os.environ.get(
    "WINLUX_VULKAN_ICD_DIRS", "/usr/share/vulkan/icd.d:/etc/vulkan/icd.d").split(":") if p)
GPU_VENDORS = {"0x10de": "NVIDIA", "0x1002": "AMD", "0x8086": "Intel"}
# Pilote -> fichier Vulkan qui le sert ; nouveau et les cartes intégrées restent au processeur.
GPU_VULKAN = {"nvidia": "nvidia_icd", "amdgpu": "radeon_icd"}


def _read(path):
    try:
        return Path(path).read_text(encoding="utf-8", errors="replace").strip()
    except OSError:
        return ""


def _nvidia_vram_gib():
    tool = shutil.which("nvidia-smi")
    if not tool:
        return 0.0
    try:
        out = subprocess.run([tool, "--query-gpu=memory.total", "--format=csv,noheader,nounits"],
                             capture_output=True, text=True, timeout=10).stdout
        return max((int(line.strip()) for line in out.splitlines() if line.strip().isdigit()), default=0) / 1024
    except (OSError, subprocess.SubprocessError):
        return 0.0


def _vulkan_icd(name):
    return any((folder / f"{name}.json").is_file() or any(folder.glob(f"{name}*.json")) for folder in VULKAN_ICD_DIRS)


def graphics_cards():
    """Cartes graphiques vues par le noyau : [{"vendor", "driver", "vram", "vulkan"}].
    « vram » en Gio, 0 pour une carte intégrée (mémoire partagée)."""
    cards = []
    for card in sorted(DRM_DIR.glob("card[0-9]*")):
        if "-" in card.name:  # connecteurs (card0-HDMI-A-1)
            continue
        device = card / "device"
        vendor = _read(device / "vendor").lower()
        try:
            driver = os.path.basename(os.readlink(device / "driver"))
        except OSError:
            driver = ""
        vram = 0.0
        if driver == "amdgpu":
            total = _read(device / "mem_info_vram_total")
            vram = int(total) / (1 << 30) if total.isdigit() else 0.0
        elif driver == "nvidia":
            vram = _nvidia_vram_gib()
        icd = GPU_VULKAN.get(driver)
        cards.append({"vendor": GPU_VENDORS.get(vendor, vendor), "driver": driver, "vram": vram,
                      "vulkan": bool(icd) and _vulkan_icd(icd)})
    return cards


def pc_plan():
    """Modèle et moteur adaptés à ce PC, choisis sans rien demander, et dits :
    {"model", "engine", "gpu", "ram", "vram"}.
    - carte graphique dédiée utilisable par Vulkan, 7 Gio ou plus : 7B sur la carte ;
    - de 3,5 à 7 Gio : 3B sur la carte ;
    - sinon processeur seul : 3B avec 12 Gio de mémoire ou plus, 1,5B en dessous."""
    ram = _mem_total_gib()
    usable = [card for card in graphics_cards() if card["vulkan"] and card["vram"] >= 3.5]
    best = max(usable, key=lambda card: card["vram"], default=None)
    if best is not None:
        model = "7b" if best["vram"] >= 7 else "3b"
        return {"model": model, "engine": "vulkan", "gpu": best["vendor"], "ram": ram, "vram": best["vram"]}
    return {"model": "3b" if ram >= 12 else "1.5b", "engine": "cpu", "gpu": None, "ram": ram, "vram": 0.0}


def model_size(key):
    return sum(size for _, _, size in LOCAL_MODELS[key]["files"])


def size_text(size):
    """1,1 Go ; 450 Mo (séparateur décimal de la langue choisie)."""
    if size >= 1 << 30:
        text = f"{size / (1 << 30):.1f}"
        return (text.replace(".", ",") if _LANG in ("fr", "de", "es") else text) + " " + _t("Go")
    return f"{max(1, size >> 20)} " + _t("Mo")


def plan_text(plan):
    """Ce qui a été choisi pour ce PC et ce que cela coûte, en clair."""
    model = LOCAL_MODELS[plan["model"]]
    where = (_t("sur la carte graphique {gpu}", gpu=plan["gpu"]) if plan["engine"] == "vulkan"
             else _t("sur le processeur"))
    memory = size_text(model_size(plan["model"]) + (512 << 20))
    return _t("Choisi pour ce PC : {model} ({size}), {where}. Pendant que vous lui écrivez, elle prend environ "
              "{memory} de mémoire{gpu_memory} et fait travailler l'ordinateur : ce n'est pas l'idéal sur un PC "
              "déjà occupé. Elle s'arrête seule deux minutes après votre dernière question ; désactivée, elle ne "
              "fait strictement rien.", model=_t(model["name"]), size=size_text(model_size(plan["model"])),
              where=where, memory=memory,
              gpu_memory=_t(" de la carte graphique") if plan["engine"] == "vulkan" else "")


def local_model_text():
    """Modèle installé et où il tourne, pour Paramètres > Assistant IA."""
    local = provider("local")
    key = local.model_key()
    if key is None:
        return _t("Pas encore téléchargé")
    where = _t("sur la carte graphique") if local.engine_kind() == "vulkan" else _t("sur le processeur")
    return f"{_t(LOCAL_MODELS[key]['name'])}, {where}"


class LocalProvider(Provider):
    """Petite IA sur ce PC : llama.cpp (llama-server) et un modèle Qwen2.5
    Instruct choisi selon le PC (licence Apache 2.0) : 1,5B, 3B ou 7B, sur le
    processeur ou la carte graphique (Vulkan). Rien ne quitte ce PC, et le moteur
    ne tourne que pendant qu'on lui écrit."""
    key, name = "local", "IA locale"
    remote = False

    def supported(self):
        return _machine() in ("x86_64", "amd64") and _mem_total_gib() >= LOCAL_MIN_RAM_GIB

    def plan(self):
        return pc_plan()

    def steps(self):
        plan = self.plan()
        model = LOCAL_MODELS[plan["model"]]
        _, _, engine_size = LLAMA_ENGINES[plan["engine"]]
        return (_t("Télécharger le moteur llama.cpp (environ {size})", size=size_text(engine_size)),
                _t("Télécharger le modèle {name} (environ {size})", name=_t(model["name"]),
                   size=size_text(model_size(plan["model"]))),
                _t("Ajouter le bouton à la barre des tâches"))

    def engine_kind(self):
        kind = _read(LOCAL_DIR / "moteur/TYPE")
        return kind if kind in LLAMA_ENGINES else "cpu"

    def engine(self):
        forced = os.environ.get("WINLUX_ASSISTANT_LLAMA")
        if forced:
            return Path(forced) if Path(forced).is_file() else None
        found = sorted(LOCAL_DIR.glob("moteur/**/llama-server"))
        return found[0] if found and os.access(found[0], os.X_OK) else None

    def model_key(self):
        """Modèle installé : celui retenu à l'installation (assistant.conf)."""
        key = read_config().get("local_model", "")
        if key in LOCAL_MODELS and all((LOCAL_DIR / name).is_file() for name, _, _ in LOCAL_MODELS[key]["files"]):
            return key
        return next((key for key in LOCAL_MODELS
                     if all((LOCAL_DIR / name).is_file() for name, _, _ in LOCAL_MODELS[key]["files"])), None)

    def model(self):
        key = self.model_key()
        return LOCAL_DIR / LOCAL_MODELS[key]["files"][0][0] if key else None

    def launcher(self):
        return self.engine() if self.engine() and self.model() else None

    def gpu_layers(self):
        """Toutes les couches sur la carte graphique avec le moteur Vulkan."""
        return ["-ngl", "99"] if self.engine_kind() == "vulkan" and not os.environ.get("WINLUX_ASSISTANT_LLAMA") else []

    def download_engine(self, kind, progress, cancel):
        name, sha256, _ = LLAMA_ENGINES[kind]
        label = _t("Téléchargement du moteur")
        try:
            return download(f"{LLAMA_RELEASES}/{name}", LOCAL_DIR / name, "sha256", bytes.fromhex(sha256), progress,
                            cancel, label)
        except (OSError, urllib.error.URLError) as error:
            if cancel is not None and cancel.is_set():
                raise
            # 504 de GitHub sur l'adresse de téléchargement : même fichier par l'API.
            try:
                with _get(LLAMA_ASSET_API) as response:
                    assets = json.loads(response.read().decode("utf-8")).get("assets", [])
                url = next(asset["url"] for asset in assets if asset.get("name") == name)
            except (OSError, ValueError, StopIteration, KeyError):
                raise error
            return download(url, LOCAL_DIR / name, "sha256", bytes.fromhex(sha256), progress, cancel, label,
                            accept="application/octet-stream")

    def install_engine(self, progress, cancel, kind="cpu"):
        LOCAL_DIR.mkdir(parents=True, exist_ok=True)
        archive = self.download_engine(kind, progress, cancel)
        staging = LOCAL_DIR / "moteur.nouveau"
        shutil.rmtree(staging, ignore_errors=True)
        try:
            if archive.name.endswith(".zip"):
                with zipfile.ZipFile(archive) as bundle:
                    bundle.extractall(staging)
            else:
                with tarfile.open(archive) as tar:
                    tar.extractall(staging, filter="data")
            servers = sorted(staging.glob("**/llama-server"))
            if not servers:
                raise RuntimeError(_t("Archive inattendue : {name}", name="llama.cpp"))
            for path in servers[0].parent.iterdir():  # zip ne garde pas le droit d'exécution
                if path.is_file() and (path.name.startswith("llama-") or ".so" in path.name):
                    path.chmod(0o755)
            missing = engine_problem(servers[0])
            if missing:
                raise RuntimeError(_t("Le moteur llama.cpp ne peut pas démarrer sur ce PC : {e}", e=missing))
            (staging / "TYPE").write_text(kind + "\n", encoding="utf-8")
            _replace_dir(staging, LOCAL_DIR / "moteur")
        finally:
            archive.unlink(missing_ok=True)
            shutil.rmtree(staging, ignore_errors=True)

    def install_model(self, progress, cancel, key="1.5b"):
        model = LOCAL_MODELS[key]
        for name, sha256, _ in model["files"]:
            if not (LOCAL_DIR / name).is_file():
                download(f"{HF_BASE}/{model['repo']}/resolve/{model['revision']}/{name}", LOCAL_DIR / name,
                         "sha256", bytes.fromhex(sha256), progress, cancel, _t("Téléchargement du modèle"))
        write_config(local_model=key)
        # L'ancien modèle (PC changé, carte graphique ajoutée) ne garde pas sa place sur le disque.
        keep = {name for name, _, _ in model["files"]}
        for other in LOCAL_MODELS.values():
            for name, _, _ in other["files"]:
                if name not in keep:
                    (LOCAL_DIR / name).unlink(missing_ok=True)

    def install(self, progress, cancel, mark):
        plan = self.plan()
        if self.engine() is None or self.engine_kind() != plan["engine"]:
            try:
                self.install_engine(progress, cancel, plan["engine"])
            except RuntimeError:
                if plan["engine"] == "cpu" or (cancel is not None and cancel.is_set()):
                    raise
                # Vulkan inutilisable ici : processeur seul, et le modèle qui lui convient.
                plan = dict(plan, engine="cpu", gpu=None, model="3b" if plan["ram"] >= 12 else "1.5b")
                self.install_engine(progress, cancel, "cpu")
        mark(0)
        if self.model_key() != plan["model"]:
            self.install_model(progress, cancel, plan["model"])
        mark(1)
        return self.launcher()

    def status(self, launcher=None):
        installed = (launcher or self.launcher()) is not None
        return {"installed": installed, "loggedIn": installed}

    def remove(self):
        shutil.rmtree(LOCAL_DIR, ignore_errors=True)
        write_config(local_model=None)


PROVIDERS = ("claude", "chatgpt", "local")
_PROVIDERS = {cls.key: cls() for cls in (ClaudeProvider, ChatGPTProvider, LocalProvider)}


def provider(key):
    return _PROVIDERS[key]


def display_name(key):
    return _t(provider(key).name)


# ── Claude Code : même installation que l'application du menu Démarrer ──────

def claude_code_app():
    """Module winlux-claude-code.py (téléchargement vérifié de claude.exe)."""
    import importlib.util
    here = Path(__file__).resolve()
    for candidate in (os.environ.get("WINLUX_CLAUDE_CODE_APP"),
                      here.parent.parent / "integration/native/claude-code/winlux-claude-code.py",
                      "/usr/share/winlux/native/claude-code/winlux-claude-code.py"):
        if candidate and Path(candidate).is_file():
            spec = importlib.util.spec_from_file_location("winlux_claude_code", candidate)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            return module
    return None


def claude_launcher():
    forced = os.environ.get("WINLUX_ASSISTANT_CLAUDE")
    if forced:
        return Path(forced) if Path(forced).is_file() else None
    app = claude_code_app()
    return app.installed_launcher() if app else None


def _private_dir(path):
    path.mkdir(parents=True, exist_ok=True)
    path.chmod(0o700)
    return path


def login_failure(returncode):
    """Message d'une connexion qui s'arrête seule. -9 : processus tué par le
    système, faute de mémoire (#997), jamais par l'assistant hors « Annuler »."""
    if returncode == -9:
        return _t("Le système a arrêté la connexion faute de mémoire. Fermez des applications, puis réessayez.")
    return _t("La connexion n'a pas abouti : {e}", e=f"code {returncode}")


def work_dir():
    """Dossier de travail de claude.exe et codex : jamais un dossier personnel
    (#913), sinon la couche PE demande l'accès aux fichiers personnels."""
    return _private_dir(WORK)


def has_browser():
    """Un vrai navigateur est-il installé sur ce PC (#912) ? L'ISO n'en livre aucun."""
    if any(os.access(BROWSERS_DIR / path, os.X_OK) for path in BROWSER_BINARIES):
        return True
    # Autres navigateurs du catalogue (Brave, Opera, Tor Browser…, #1716) :
    # winlux-browsers note leur programme dans <id>/program.
    try:
        for noted in BROWSERS_DIR.glob("*/program"):
            relative = noted.read_text().split("\n", 1)[0].strip()
            if relative.startswith("root/") and ".." not in relative.split("/") and os.access(noted.parent / relative, os.X_OK):
                return True
    except OSError:
        pass
    return any(shutil.which(name) for name in BROWSER_COMMANDS)


def _runs_pe(launcher):
    """Le lanceur de Claude Code passe-t-il par la couche PE (claude.exe) ?"""
    try:
        return launcher is not None and Path(launcher).stat().st_size < 4096 and \
            "claude.exe" in Path(launcher).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False


def windows_path(path):
    """Chemin Linux vu par un programme Windows : Z:\\ est la racine (paths::to_windows)."""
    return "Z:" + str(path).replace("/", "\\")


def claude_env(launcher=None):
    """Rien d'autre que la conversation ne part : télémétrie et rapports coupés.
    Connexion propre à l'assistant (CLAUDE_CONFIG_DIR), jamais ~/.claude."""
    env = dict(os.environ)
    env.pop("ANTHROPIC_API_KEY", None)  # le compte Claude, pas une clé d'API
    env.update(DISABLE_TELEMETRY="1", DISABLE_ERROR_REPORTING="1", DISABLE_BUG_COMMAND="1",
               CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1", DISABLE_AUTOUPDATER="1")
    _private_dir(CLAUDE_ACCOUNT)
    launcher = launcher or claude_launcher()
    env["CLAUDE_CONFIG_DIR"] = windows_path(CLAUDE_ACCOUNT) if _runs_pe(launcher) else str(CLAUDE_ACCOUNT)
    return env


def protect_credentials():
    """Connexions de l'assistant lisibles par le seul propriétaire.

    Créées par claude.exe à travers la couche PE, elles pouvaient hériter de
    droits 644 : dossiers en 700, jetons et compte en 600. ~/.claude et
    ~/.codex ne sont jamais touchés."""
    base = CLAUDE_ACCOUNT
    fixed = []
    for path, mode in ((ACCOUNTS, 0o700), (base, 0o700), (base / ".credentials.json", 0o600),
                       (base / ".claude.json", 0o600)):
        try:
            if path.stat().st_mode & 0o077:
                path.chmod(mode)
                fixed.append(path)
        except OSError:
            pass
    return fixed + _PROVIDERS["chatgpt"].protect()


def auth_status(launcher=None, timeout=60):
    """{"loggedIn": bool, ...} d'après « claude auth status --json »."""
    launcher = launcher or claude_launcher()
    if not launcher:
        return {"loggedIn": False, "installed": False}
    try:
        out = subprocess.run([str(launcher), "auth", "status", "--json"], capture_output=True, text=True,
                             timeout=timeout, env=claude_env(launcher), stdin=subprocess.DEVNULL,
                             cwd=work_dir()).stdout
        start = out.find("{")
        status = json.loads(out[start:out.rindex("}") + 1]) if start >= 0 else {}
    except (OSError, subprocess.SubprocessError, ValueError):
        status = {}
    status["installed"] = True
    status["loggedIn"] = bool(status.get("loggedIn"))
    return status


def account_name(status):
    for key in ("email", "emailAddress", "account"):
        value = status.get(key)
        if isinstance(value, str) and value:
            return value
    account = status.get("oauthAccount")
    if isinstance(account, dict):
        return account.get("emailAddress") or ""
    return ""


# ── Bouton de la barre des tâches (greffon lanceur xfce4-panel) ─────────────

def _xfconf(*args):
    try:
        return subprocess.run(["xfconf-query", "-c", "xfce4-panel", *args], capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return None


def _panel_ids():
    """Ordre des greffons ; xfconf-query l'écrit après « Value is an array with N items: »."""
    result = _xfconf("-p", "/panels/panel-1/plugin-ids")
    if result is None or result.returncode:
        return None
    return [int(line) for line in result.stdout.splitlines() if line.strip().isdigit()]


def _set_panel_ids(ids):
    args = []
    for number in ids:
        args += ["-t", "int", "-s", str(number)]
    result = _xfconf("-p", "/panels/panel-1/plugin-ids", "-a", *args)
    return result is not None and not result.returncode


def _plugin_type(number):
    result = _xfconf("-p", f"/plugins/plugin-{number}")
    return result.stdout.strip() if result is not None and not result.returncode else None


def _plugin_is_ours(number):
    result = _xfconf("-p", f"/plugins/plugin-{number}/items")
    return result is not None and not result.returncode and "winlux-assistant.desktop" in result.stdout


def _panel_restart():
    """La barre en cours ne charge un greffon ajouté ou retiré qu'en redémarrant."""
    if not shutil.which("xfce4-panel"):
        return
    try:
        if subprocess.run(["pgrep", "-u", str(os.getuid()), "-x", "xfce4-panel"], capture_output=True,
                          timeout=5).returncode == 0:
            subprocess.run(["xfce4-panel", "-r"], capture_output=True, timeout=15)
    except (OSError, subprocess.SubprocessError):
        pass


def panel_add():
    """Ajoute le bouton de l'assistant juste à gauche des icônes de la barre (un seul,
    quel que soit le nombre d'assistants activés)."""
    if not shutil.which("xfconf-query"):
        return False
    ids = _panel_ids()
    if ids is None:
        return False
    number = read_config().get("panel_plugin", "")
    if not (number.isdigit() and _plugin_type(number) == "launcher" and _plugin_is_ours(number)):
        # 40 à 49 : marges de la barre flottante (winlux-control, #889).
        number = next(str(n) for n in range(FIRST_PLUGIN, 400) if not 40 <= n < 50 and _plugin_type(n) is None)
        for prop, kind, value in ((f"/plugins/plugin-{number}", "string", "launcher"),
                                  (f"/plugins/plugin-{number}/show-label", "bool", "false")):
            _xfconf("-p", prop, "-n", "-t", kind, "-s", value)
        _xfconf("-p", f"/plugins/plugin-{number}/items", "-n", "-a", "-t", "string", "-s", DESKTOP_FILE)
    write_config(panel_plugin=number)
    if int(number) not in ids:
        if FLOAT_CORNER in ids:  # barre flottante (#889) : avant la réserve du coin droit
            position = ids.index(FLOAT_CORNER)
        else:
            position = ids.index(PANEL_AFTER) + 1 if PANEL_AFTER in ids else max(len(ids) - 1, 0)
        ids.insert(position, int(number))
        if not _set_panel_ids(ids):
            return False
        _panel_restart()
    return True


def panel_remove():
    number = read_config().get("panel_plugin", "")
    if not number.isdigit() or not shutil.which("xfconf-query"):
        return False
    ids = _panel_ids()
    shown = ids is not None and int(number) in ids
    if shown:
        _set_panel_ids([n for n in ids if n != int(number)])
    if _plugin_is_ours(number):
        _xfconf("-p", f"/plugins/plugin-{number}", "-r", "-R")
    write_config(panel_plugin=None)
    if shown:
        _panel_restart()
    return True


def enable(key="claude"):
    write_config(**{f"enabled_{key}": 1, "current": key})
    panel_add()


def disable(key="claude"):
    """Le bouton ne disparaît qu'une fois le dernier assistant désactivé."""
    write_config(**{f"enabled_{key}": 0})
    if read_config().get("current") == key:
        write_config(current=None)
    if not enabled_providers():
        panel_remove()


# ── Ce que Claude peut demander (toujours confirmé par l'utilisateur) ───────

# Réglage proposé -> (libellé, action de winlux-control).
SETTINGS = {
    "theme_sombre": ("Thème sombre", "theme-dark"),
    "theme_clair": ("Thème clair", "theme-light"),
    "barre_centree": ("Barre des tâches centrée", "align-center"),
    "barre_gauche": ("Barre des tâches à gauche", "align-left"),
    "son_plus": ("Monter le son", "volume-up"),
    "son_moins": ("Baisser le son", "volume-down"),
    "son_couper": ("Couper ou remettre le son", "volume-mute"),
    "bluetooth_on": ("Activer le Bluetooth", "bluetooth-on"),
    "bluetooth_off": ("Désactiver le Bluetooth", "bluetooth-off"),
    "reseau_renouveler": ("Renouveler la connexion réseau", "network-renew"),
    "verrouiller": ("Verrouiller l'écran", "lock-now"),
    "echelle_plus": ("Agrandir le texte et les éléments", "display-scale-up"),
    "echelle_moins": ("Réduire le texte et les éléments", "display-scale-down"),
    "echelle_auto": ("Échelle d'affichage automatique", "display-scale-auto"),
    "mode_performances": ("Mode performances", "power-profile-performance"),
    "mode_equilibre": ("Mode équilibré", "power-profile-balanced"),
    "mode_economie": ("Mode économie d'énergie", "power-profile-saver"),
    "mises_a_jour": ("Rechercher les mises à jour de Winlux", "update-check"),
}
# Pages des Paramètres que Claude peut proposer d'ouvrir (numéros stables).
SETTINGS_PAGES = {2: "Réseau", 4: "Clavier et souris", 7: "Mise à jour OS", 8: "Écran", 10: "Programmes Windows",
                  11: "Navigateurs", 12: "Accessibilité", 14: "Localisation", 16: "Son",
                  18: "Confidentialité et sécurité", 20: "Lecteurs réseau", 21: "Fond d’écran et couleurs",
                  22: "Bluetooth", 23: "Alimentation et batterie", 24: "Graphismes", 26: "Imprimantes et scanners",
                  28: "Partage et transfert", 29: "Jeux", 30: "Assistant IA"}
REQUESTS = ("aucune", "lire_journal", "lire_plantages", "lire_systeme", "chercher_fichier",
            "ouvrir_parametres", "ouvrir_dossier", "reglage")
SCHEMA = {
    "type": "object",
    "properties": {
        "reponse": {"type": "string", "description": "Réponse montrée à l'utilisateur, texte simple."},
        "demande": {"type": "string", "enum": list(REQUESTS)},
        "argument": {"type": "string", "description": "Programme, nom de fichier, page, dossier ou réglage."},
    },
    # Tous requis : le format strict d'OpenAI l'exige (argument vide si inutile).
    "required": ["reponse", "demande", "argument"],
    "additionalProperties": False,
}


def system_prompt(name="Claude"):
    pages = ", ".join(f"{number} = {name}" for number, name in SETTINGS_PAGES.items())
    settings = ", ".join(f"{key} ({label})" for key, (label, _) in SETTINGS.items())
    return f"""Tu es l'assistant {name} intégré à Winlux. Winlux est un système Linux qui lance les programmes
Windows (.exe) directement, grâce à sa propre couche de compatibilité PE (ce n'est pas Wine), avec un bureau
qui ressemble à Windows 11 : menu Démarrer, barre des tâches, Explorateur, Paramètres. Les fichiers Windows
C:\\Users\\<nom> sont le dossier personnel Linux. Le journal Winlux note les DLL et fonctions Windows qui
manquent à un programme ; un .exe qui ne s'ouvre pas manque souvent d'une DLL ou d'une fonction encore non
fournie, ou plante (journal des plantages). Les pilotes noyau et anti-triche ne sont pas pris en charge.

Réponds toujours en {LANGUAGE_NAMES[_LANG]}, la langue choisie pour Winlux, même si la question est posée dans une
autre langue ; simplement, en quelques phrases, pour quelqu'un qui n'est pas informaticien.
Tu n'as aucun outil : n'exécute aucune commande et ne lis aucun fichier toi-même. Pour lire une information
ou agir, remplis « demande » (une seule par réponse) et « argument » (vide si inutile) ; l'utilisateur voit la demande et l'accepte ou la refuse. Demandes possibles :
- aucune : simple réponse.
- lire_journal : lignes du journal Winlux d'un programme ; argument = nom du .exe (vide = dernières erreurs).
- lire_plantages : derniers plantages de programmes.
- lire_systeme : version de Winlux, processeur, mémoire, carte graphique.
- chercher_fichier : chercher un fichier par son nom dans le dossier personnel ; argument = morceau du nom.
- ouvrir_parametres : argument = numéro de page ({pages}).
- ouvrir_dossier : ouvrir l'Explorateur sur un dossier ; argument = chemin (~ = dossier personnel).
- reglage : argument = un de : {settings}.
N'invente jamais le contenu d'un journal : demande-le. Ce qui n'est pas dans cette liste (par exemple
l'éclairage nocturne, s'il n'y figure pas), explique comment le faire à la main ou dis que Winlux ne le
propose pas encore. Les données lues avec l'accord de l'utilisateur arrivent entre <donnees> et </donnees>.
"""


def _home_relative(text):
    return text.replace(str(HOME), "~")


def _journal_lines():
    lines = []
    for name in ("winlux.log.1", "winlux.log"):
        try:
            lines += (JOURNAL / name).read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            pass
    return lines


def _crash_blocks():
    try:
        text = (JOURNAL / "crash.log").read_text(encoding="utf-8", errors="replace")
    except OSError:
        return []
    return [block.strip() for block in re.split(r"(?=^\[CRASH\])", text, flags=re.M) if block.strip()]


def read_journal(program):
    program = Path(program.replace("\\", "/")).name.strip()
    lines = _journal_lines()
    if program:
        wanted = fold(program)
        lines = [line for line in lines if wanted in fold(line)]
        crashes = [block for block in _crash_blocks() if wanted in fold(block)]
    else:
        lines = [line for line in lines if "[error]" in line or "[warn]" in line]
        crashes = []
    text = "\n".join(lines[-80:])
    if crashes:
        text += "\n\n" + "\n\n".join(crashes[-3:])
    return _home_relative(text.strip()[-9000:]) or _t("(aucune ligne trouvée)")


def read_crashes():
    return _home_relative("\n\n".join(_crash_blocks()[-5:])[-9000:]) or _t("(aucune ligne trouvée)")


def read_system():
    lines = []
    try:
        info = dict(line.split("=", 1) for line in Path("/usr/share/winlux/BUILD-INFO").read_text().splitlines()
                    if "=" in line)
        lines.append(f"Winlux {info.get('version', '?')}")
    except OSError:
        pass
    lines.append("Linux " + os.uname().release)
    try:
        model = next(line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                     if line.startswith("model name"))
        lines.append(_t("Processeur : {model} ({count} cœurs)", model=model, count=os.cpu_count()))
    except (OSError, StopIteration):
        pass
    try:
        total = next(int(line.split()[1]) for line in Path("/proc/meminfo").read_text().splitlines()
                     if line.startswith("MemTotal"))
        lines.append(_t("Mémoire : {size} Gio", size=f"{total / 1048576:.1f}"))
    except (OSError, StopIteration, ValueError):
        pass
    try:
        status = dict(line.split("=", 1) for line in Path("/run/winlux/control.txt").read_text().splitlines()
                      if "=" in line)
        for key, label in (("gpu", "Carte graphique"), ("display_mode", "Écran"), ("virtual_machine", "Machine virtuelle")):
            if status.get(key):
                lines.append(f"{_t(label)} : {status[key]}")
    except OSError:
        pass
    return "\n".join(lines)


def find_paths(part, limit=40, budget=4.0):
    """Fichiers et dossiers du dossier personnel dont le nom contient `part`
    (sans casse ni accents), dossiers cachés exclus."""
    wanted = fold(Path(part.replace("\\", "/")).name.strip())
    if not wanted:
        return []
    found, deadline = [], time.monotonic() + budget
    for root, dirs, files in os.walk(HOME):
        dirs[:] = sorted(d for d in dirs if not d.startswith(".") and d not in ("node_modules", "__pycache__"))
        for name in sorted(dirs + files):
            if wanted in fold(name):
                found.append(os.path.join(root, name))
                if len(found) >= limit:
                    return found
        if time.monotonic() > deadline:
            break
    return found


def find_files(part, limit=40, budget=4.0):
    return "\n".join(_home_relative(path) for path in find_paths(part, limit, budget)) or _t("(aucun fichier trouvé)")


def describe_request(kind, argument):
    """(texte affiché, fonction qui lit les données) d'une demande de lecture."""
    if kind == "lire_journal":
        name = Path(argument.replace("\\", "/")).name.strip()
        what = _t("le journal du programme {name}", name=name) if name else _t("les dernières erreurs du journal Winlux")
        return what, lambda: read_journal(argument)
    if kind == "lire_plantages":
        return _t("les derniers plantages de programmes"), read_crashes
    if kind == "lire_systeme":
        return _t("la description de ce PC (version, processeur, mémoire)"), read_system
    if kind == "chercher_fichier":
        return (_t("les noms des fichiers de votre dossier personnel qui contiennent « {name} »", name=argument),
                lambda: find_files(argument))
    return None


def describe_action(kind, argument):
    """(texte affiché, commande exacte) d'une action ; None si elle n'est pas permise."""
    if kind == "reglage" and argument in SETTINGS:
        label, action = SETTINGS[argument]
        return _t(label), ["winlux-control", action]
    if kind == "ouvrir_parametres" and argument.strip().isdigit() and int(argument) in SETTINGS_PAGES:
        page = int(argument)
        return (_t("ouvrir Paramètres, page {page}", page=f"« {_t(SETTINGS_PAGES[page])} »"),
                ["winlux-shell-app", "settings", str(page)])
    if kind == "ouvrir_dossier":
        path = Path(os.path.expanduser(argument.strip())) if argument.strip() else None
        if path is not None and path.is_absolute() and path.is_dir():
            return _t("ouvrir le dossier {path}", path=_home_relative(str(path))), ["winlux-shell-app", "explorer", str(path)]
        if path is not None and path.is_absolute() and path.is_file():
            # Un fichier : l'Explorateur s'ouvre sur le dossier qui le contient.
            return (_t("ouvrir l'emplacement de {path}", path=_home_relative(str(path))),
                    ["winlux-shell-app", "explorer", str(path.parent)])
    return None


def run_action(command):
    """Lance l'action confirmée ; renvoie un message d'erreur ou None."""
    try:
        if command[0] == "winlux-shell-app":
            subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
            return None
        result = subprocess.run(command, capture_output=True, text=True, timeout=60)
        if result.returncode:
            return (result.stderr or result.stdout).strip()[-400:] or f"code {result.returncode}"
        return None
    except (OSError, subprocess.SubprocessError) as error:
        return str(error)


# ── Conversation avec Claude ou ChatGPT ─────────────────────────────────────

class Conversation:
    """Historique envoyé à chaque question ; rien n'est écrit sur le disque."""

    def __init__(self, key="claude"):
        self.key = key
        self.entries = []  # (rôle, texte) : user, assistant, data

    def prompt(self):
        parts = []
        for role, text in self.entries[-16:]:
            if role == "user":
                parts.append(f"Utilisateur : {text}")
            elif role == "assistant":
                parts.append(f"Assistant : {text}")
            else:
                parts.append(f"<donnees>\n{text}\n</donnees>")
        return "\n\n".join(parts)

    def command(self):
        return provider("claude").command(provider(self.key).name)

    def ask(self, timeout=240):
        """Envoie la conversation ; renvoie (réponse, demande, argument)."""
        WORK.mkdir(parents=True, exist_ok=True)
        WORK.chmod(0o700)
        backend = provider(self.key)
        backend.protect()
        try:
            answer = backend.ask(self.prompt(), timeout)
        finally:
            backend.protect()
        reply = str(answer.get("reponse") or "").strip()
        kind = answer.get("demande") if answer.get("demande") in REQUESTS else "aucune"
        argument = str(answer.get("argument") or "")
        self.entries.append(("assistant", reply + (f" [demande : {kind} {argument}]".rstrip() if kind != "aucune" else "")))
        return reply, kind, argument


# ── IA locale : llama-server sur 127.0.0.1, seulement fenêtre ouverte ───────

def _die_with_parent():
    try:
        import ctypes
        import signal
        ctypes.CDLL(None).prctl(1, signal.SIGTERM)  # PR_SET_PDEATHSIG
    except (OSError, AttributeError):
        pass


class LocalServer:
    """llama-server démarre quand le panneau s'ouvre et s'arrête à sa fermeture.
    Il n'écoute que 127.0.0.1, sur un port libre, protégé par une clé tirée au
    hasard (passée par l'environnement, pas en argument visible par `ps`)."""

    def __init__(self):
        self.process = None
        self.port = None
        self.key = secrets.token_hex(16)

    def running(self):
        return self.process is not None and self.process.poll() is None

    def spawn(self):
        """Lance le moteur. À appeler depuis le fil principal : le moteur reçoit
        SIGTERM si l'assistant meurt (PR_SET_PDEATHSIG suit le fil qui l'a lancé)."""
        if self.running():
            return
        local = provider("local")
        engine, model = local.engine(), local.model()
        if engine is None or model is None:
            raise RuntimeError(_t("L'IA locale n'est pas installée."))
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            self.port = probe.getsockname()[1]
        env = dict(os.environ, LLAMA_API_KEY=self.key)
        env["LD_LIBRARY_PATH"] = ":".join(filter(None, (str(engine.parent), env.get("LD_LIBRARY_PATH"))))
        WORK.mkdir(parents=True, exist_ok=True)
        with open(WORK / "ia-locale.log", "wb") as log:
            self.process = subprocess.Popen([str(engine), "-m", str(model), "--host", "127.0.0.1",
                                             "--port", str(self.port), "-c", "4096", "-np", "1",
                                             *local.gpu_layers()],
                                            stdin=subprocess.DEVNULL, stdout=log, stderr=log, env=env,
                                            preexec_fn=_die_with_parent)

    def wait_ready(self, timeout=180):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            process = self.process
            if process is None:
                raise RuntimeError(_t("Le moteur de l'IA locale ne répond pas."))
            if process.poll() is not None:
                raise RuntimeError(_t("Le moteur de l'IA locale s'est arrêté (code {code}).", code=process.returncode))
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{self.port}/health", timeout=2) as response:
                    if response.status == 200:
                        return
            except OSError:
                pass
            time.sleep(0.3)
        self.stop()
        raise RuntimeError(_t("Le moteur de l'IA locale ne répond pas."))

    def start(self, timeout=180):
        self.spawn()
        self.wait_ready(timeout)

    def memory(self):
        """Mémoire vive prise par le moteur (octets), 0 s'il est arrêté (#928)."""
        process = self.process
        if process is None or process.poll() is not None:
            return 0
        for line in _read(f"/proc/{process.pid}/status").splitlines():
            if line.startswith("VmRSS:"):
                return int(line.split()[1]) * 1024
        return 0

    def stop(self):
        process, self.process = self.process, None
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(5)
            except subprocess.TimeoutExpired:
                process.kill()

    def chat(self, system, user, schema, max_tokens=300, timeout=120):
        """Une réponse JSON contrainte par `schema` (grammaire de llama.cpp)."""
        body = json.dumps({"messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
                           "temperature": 0.2, "max_tokens": max_tokens,
                           "response_format": {"type": "json_schema",
                                               "json_schema": {"name": "reponse", "schema": schema}}}).encode()
        request = urllib.request.Request(f"http://127.0.0.1:{self.port}/v1/chat/completions", data=body,
                                         headers={"Content-Type": "application/json",
                                                  "Authorization": f"Bearer {self.key}"})
        with urllib.request.urlopen(request, timeout=timeout) as response:
            content = json.loads(response.read().decode("utf-8"))["choices"][0]["message"]["content"]
        answer = json.loads(content[content.find("{"):content.rindex("}") + 1])
        if not isinstance(answer, dict):
            raise ValueError(content)
        return answer


# Dossiers que l'agent « fichiers » sait ouvrir -> nom XDG.
FOLDERS = {"documents": "DOCUMENTS", "images": "PICTURES", "musique": "MUSIC", "videos": "VIDEOS",
           "telechargements": "DOWNLOAD", "bureau": "DESKTOP"}
_FOLDER_DEFAULTS = {"documents": "Documents", "images": "Images", "musique": "Musique", "videos": "Vidéos",
                    "telechargements": "Téléchargements", "bureau": "Bureau"}


def user_folder(key):
    try:
        out = subprocess.run(["xdg-user-dir", FOLDERS[key]], capture_output=True, text=True, timeout=5).stdout.strip()
        if out and Path(out).is_dir() and Path(out) != HOME:
            return Path(out)
    except (OSError, subprocess.SubprocessError):
        pass
    return HOME / _FOLDER_DEFAULTS[key]


# Demande de fichier reconnue sans le modèle, dans les quatre langues : « Où est
# mon fichier » allait à l'agent aide, qui proposait une page sans rapport (#1002).
FILE_WORDS = re.compile(r"\b(fichiers?|documents?|dossiers?|photos?|files?|folders?|dateien|datei|ordner|"
                        r"archivos?|carpetas?|documentos?|fotos?)\b")
# Mots qui ne sont pas un nom de fichier : chercher « fichier » ne trouve rien d'utile.
GENERIC_TERMS = {"fichier", "fichiers", "document", "documents", "mon fichier", "file", "my file", "datei",
                 "meine datei", "archivo", "mi archivo", "documento", "dossier", "folder", "ordner", "carpeta"}

# Mots de chaque famille de réglages (texte sans accents, quatre langues). Le
# petit modèle choisissait un réglage même pour « Bonjour » (#1033) : un réglage
# n'est proposé que si la question en parle.
SETTING_WORDS = {
    "theme": r"\b(theme|themes|sombre|clair|dark|light|dunkel|hell|oscuro|claro|tema)\b",
    "barre": r"\b(barre|taskbar|taskleiste|barra)\b",
    "son": r"\b(son|sons|volume|sound|ton|lautstarke|sonido|muet|mute|stumm|silencio|audio)\b",
    "bluetooth": r"\bbluetooth\b",
    "reseau": r"\b(reseau|wifi|wi-fi|internet|network|netzwerk|red|connexion|connection|verbindung|conexion)\b",
    "verrouiller": r"\b(verrouill\w*|lock|sperr\w*|bloque\w*)\b",
    "echelle": r"\b(texte|taille|echelle|scale|zoom|text|grosse|schrift|tamano|letra|agrandi\w*|redui\w*)\b",
    "mode": r"\b(mode|energie|batterie|performances?|economie|power|battery|akku|energy|energia|bateria|rendimiento)\b",
    "mises": r"\b(mises? a jour|update\w*|aktualis\w*|actualiz\w*)\b",
}
_SETTING_FAMILY = {"theme_": "theme", "barre_": "barre", "son_": "son", "bluetooth_": "bluetooth",
                   "reseau_": "reseau", "verrouiller": "verrouiller", "echelle_": "echelle", "mode_": "mode",
                   "mises_": "mises"}
# Mots qui désignent chaque dossier connu (#1034 : « Où est mon fichier » ouvrait Documents).
FOLDER_WORDS = {"documents": r"\b(documents?|dokumente|documentos?)\b",
                "images": r"\b(images?|photos?|pictures?|bilder|imagenes|fotos?)\b",
                "musique": r"\b(musique|music|musik|musica)\b", "videos": r"\b(videos?)\b",
                "telechargements": r"\b(telecharg\w*|downloads?|descargas?)\b",
                "bureau": r"\b(bureau|desktop|schreibtisch|escritorio)\b"}


def setting_relevant(key, question):
    """Le réglage choisi par le modèle a-t-il un rapport avec la question ?"""
    family = next((f for prefix, f in _SETTING_FAMILY.items() if key.startswith(prefix)), None)
    return family is not None and re.search(SETTING_WORDS[family], fold(question)) is not None


def mentions_setting(question):
    text = fold(question)
    return any(re.search(words, text) for words in SETTING_WORDS.values()) or \
        re.search(r"\b(parametres?|reglages?|settings|einstellungen|configuracion|ajustes)\b", text) is not None


AGENTS = ("fichiers", "reglages", "aide")
assert len(AGENTS) <= MAX_AGENTS
ROUTER_SCHEMA = {"type": "object", "properties": {"agent": {"type": "string", "enum": list(AGENTS)}},
                 "required": ["agent"], "additionalProperties": False}
FILES_SCHEMA = {"type": "object", "properties": {
    "terme": {"type": "string"}, "dossier": {"type": "string", "enum": ["aucun", *FOLDERS]}},
    "required": ["terme", "dossier"], "additionalProperties": False}
SETTINGS_SCHEMA = {"type": "object", "properties": {
    "reglage": {"type": "string", "enum": ["aucun", *SETTINGS]},
    "page": {"type": "string", "enum": ["aucune", *(str(n) for n in SETTINGS_PAGES)]}},
    "required": ["reglage", "page"], "additionalProperties": False}
HELP_SCHEMA = {"type": "object", "properties": {
    "reponse": {"type": "string"},
    "page": {"type": "string", "enum": ["aucune", *(str(n) for n in SETTINGS_PAGES)]}},
    "required": ["reponse", "page"], "additionalProperties": False}


# Mots qui rendent une page des Paramètres pertinente pour l'agent aide (#1002).
PAGE_WORDS = {2: ("reseau", "wifi", "wi-fi", "internet", "network", "netzwerk", "red"),
              8: ("ecran", "display", "screen", "bildschirm", "pantalla", "resolution"),
              12: ("accessibilite", "accessibility", "barrierefreiheit", "accesibilidad"),
              16: ("son", "audio", "sound", "ton", "sonido", "volume"),
              22: ("bluetooth",), 23: ("batterie", "battery", "akku", "bateria", "alimentation"),
              26: ("imprimante", "printer", "drucker", "impresora", "scanner")}


def page_relevant(number, question, program=False):
    """L'agent aide ne propose une page que si elle a un rapport avec la
    question : Programmes Windows pour un .exe nommé, sinon un mot de la
    page dans la question. À défaut, aucune action (#1002)."""
    if number == 10:
        return program
    words = fold(question)
    names = PAGE_WORDS.get(number, ()) + tuple(w for w in re.split(r"[\s’']+", fold(SETTINGS_PAGES.get(number, "")))
                                               if len(w) > 3)
    return any(re.search(rf"\b{re.escape(name)}\b", words) for name in names)


class LocalAssistant:
    """Petite IA à trois agents au plus. Un aiguilleur choisit l'agent ; chaque
    agent ne remplit que des champs fermés, le code fait le reste :
    - fichiers : terme cherché ou dossier connu -> recherche sur ce PC, puis
      « ouvrir l'emplacement » pour chaque résultat (clic « Faire ») ;
    - reglages : un réglage de la liste fermée ou une page des Paramètres ;
    - aide : réponse courte ; le journal d'un .exe nommé est lu sur ce PC.
    Les lectures restent sur ce PC (rien n'est envoyé) ; les actions attendent
    toujours un clic."""

    def __init__(self, server=None):
        self.server = server or LocalServer()

    def route(self, question):
        text = fold(question)
        if re.search(r"\S+\.exe\b", text):
            return "aide"
        if FILE_WORDS.search(text):
            return "fichiers"
        system = ("Tu aiguilles la demande d'un utilisateur de Winlux (bureau façon Windows 11) vers un agent. "
                  "fichiers : chercher, trouver ou ouvrir un fichier, un document, une photo ou un dossier. "
                  "reglages : changer un réglage (thème sombre ou clair, son, Bluetooth, réseau, barre des tâches, "
                  "taille du texte, mode d'énergie, verrouiller, mises à jour) ou ouvrir une page des Paramètres. "
                  "aide : toute autre question, par exemple un programme qui ne s'ouvre pas.")
        try:
            agent = self.server.chat(system, question, ROUTER_SCHEMA, max_tokens=20).get("agent")
        except (OSError, ValueError, KeyError, IndexError):
            agent = None
        if agent == "reglages" and not mentions_setting(question):
            return "aide"  # « Bonjour » ne propose aucun réglage (#1033)
        return agent if agent in AGENTS else "aide"

    def handle(self, question):
        """{"agent", "reply", "reads": [(quoi, texte)], "actions": [(quoi, commande)]}."""
        agent = self.route(question)
        result = {"agent": agent, "reply": "", "reads": [], "actions": []}
        getattr(self, f"agent_{agent}")(question, result)
        return result

    def agent_fichiers(self, question, result):
        system = ("Tu aides à retrouver un fichier. Mets dans « terme » le nom ou le morceau de nom du fichier "
                  "cherché, tel que l'utilisateur l'écrit, sans les mots « fichier », « document », « mon ». "
                  "Si l'utilisateur veut seulement ouvrir un de ses dossiers (Documents, Images, Musique, Vidéos, "
                  "Téléchargements, Bureau), mets-le dans « dossier » et laisse « terme » vide ; sinon "
                  "« dossier » vaut aucun.")
        answer = self.server.chat(system, question, FILES_SCHEMA, max_tokens=60)
        term = str(answer.get("terme") or "").strip().strip("«»\"' ")
        if fold(term) in GENERIC_TERMS:
            term = ""
        folder = answer.get("dossier")
        if folder in FOLDERS and not re.search(FOLDER_WORDS[folder], fold(question)):
            folder = None  # dossier que la question ne nomme pas (#1034)
        if folder in FOLDERS and not term:
            action = describe_action("ouvrir_dossier", str(user_folder(folder)))
            if action:
                result["actions"].append(action)
                result["reply"] = _t("Je peux ouvrir ce dossier :")
                return
        if not term:
            result["reply"] = _t("Quel fichier cherchez-vous ? Donnez une partie de son nom.")
            return
        paths = find_paths(term)
        result["reads"].append((_t("recherche de « {name} » dans votre dossier personnel", name=term),
                                "\n".join(_home_relative(p) for p in paths) or _t("(aucun fichier trouvé)")))
        if not paths:
            result["reply"] = _t("Je n'ai trouvé aucun fichier dont le nom contient « {name} ».", name=term)
            return
        result["reply"] = _t("J'ai trouvé {count} résultat(s) pour « {name} ». Voici les premiers :",
                             count=len(paths), name=term)
        for path in paths[:5]:
            action = describe_action("ouvrir_dossier", path)
            if action:
                result["actions"].append(action)

    def agent_reglages(self, question, result):
        settings = "; ".join(f"{key} = {label}" for key, (label, _) in SETTINGS.items())
        pages = "; ".join(f"{number} = {name}" for number, name in SETTINGS_PAGES.items())
        system = ("Tu choisis le réglage de Winlux que l'utilisateur demande. Réglages possibles : " + settings
                  + ". S'il n'y en a pas, « reglage » vaut aucun et « page » est la page des Paramètres la plus "
                  "proche : " + pages + " ; sinon « page » vaut aucune.")
        answer = self.server.chat(system, question, SETTINGS_SCHEMA, max_tokens=40)
        key = str(answer.get("reglage"))
        action = describe_action("reglage", key) if setting_relevant(key, question) else None
        if action is None and str(answer.get("page")).isdigit() and \
                page_relevant(int(answer["page"]), question):
            action = describe_action("ouvrir_parametres", str(answer.get("page")))
        if action:
            result["actions"].append(action)
            result["reply"] = _t("Je peux le faire pour vous : cliquez sur « Faire ».")
        else:
            result["reply"] = _t("Je ne sais pas faire ce réglage. Essayez dans Paramètres.")

    def agent_aide(self, question, result):
        data = ""
        program = re.search(r"([\w.+ -]*?\w)\.exe\b", question, re.I)
        if program:
            name = program.group(1).split()[-1] + ".exe"
            data = read_journal(name)
            result["reads"].append((_t("le journal du programme {name}", name=name), data))
        pages = "; ".join(f"{number} = {name}" for number, name in SETTINGS_PAGES.items())
        system = (f"Tu es la petite IA locale de Winlux, un système Linux qui lance les programmes Windows (.exe) "
                  f"avec sa propre couche de compatibilité et un bureau façon Windows 11. Réponds en "
                  f"{LANGUAGE_NAMES[_LANG]} (la langue choisie pour Winlux), en trois phrases au plus, simplement. Un .exe qui ne s'ouvre pas manque "
                  f"souvent d'une DLL ou d'une fonction Windows : le journal le dit (« missing », « unimplemented »). "
                  f"N'invente rien ; si tu ne sais pas, dis-le. « page » : la page des Paramètres utile ({pages}), "
                  f"sinon aucune.")
        user = question + (f"\n\nJournal :\n{data[-3000:]}" if data else "")
        answer = self.server.chat(system, user, HELP_SCHEMA, max_tokens=300)
        result["reply"] = str(answer.get("reponse") or "").strip() or _t("Je ne sais pas.")
        # Un .exe nommé : la page Programmes Windows est toujours proposée, quel
        # que soit le choix du modèle (#1035) ; sinon seulement une page en rapport.
        page = "10" if program else str(answer.get("page"))
        if page.isdigit() and page_relevant(int(page), question, bool(program)):
            action = describe_action("ouvrir_parametres", page)
            if action:
                result["actions"].append(action)


# ── Interface GTK ───────────────────────────────────────────────────────────

CSS = b"""
window.assistant, .assistant-body { background: #1c1b25; color: #ecedf5; }
.assistant-head { padding: 12px 14px; border-bottom: 1px solid #2f3142; }
.assistant-title { font-size: 15px; font-weight: bold; }
.bubble-user { background: #6A3DF0; color: #ffffff; border-radius: 12px; padding: 8px 12px; }
.bubble-claude { background: #2a2c3a; color: #ecedf5; border-radius: 12px; padding: 8px 12px; }
.bubble-note { color: #aeb3c7; font-size: 12px; }
.request { background: #262838; border: 1px solid #6A3DF0; border-radius: 10px; padding: 10px; }
.hint { color: #9aa0b6; font-size: 11px; }
.chip { background: #2a2c3a; color: #ecedf5; border: 1px solid #3c3f55; border-radius: 14px; padding: 4px 10px; }
entry { background: #14131b; color: #ecedf5; border: 1px solid #48536b; border-radius: 8px; padding: 8px; }
button { background: #30384b; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 6px 12px; box-shadow: none; }
button.suggested-action { background: #6A3DF0; border-color: #B69CFF; color: #ffffff; }
textview, textview text { background: #14131b; color: #d6d9e6; font-family: monospace; font-size: 11px; }
"""


def _gtk():
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gdk, GLib, Gtk
    provider = Gtk.CssProvider()
    provider.load_from_data(CSS)
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), provider, 600)
    return Gdk, GLib, Gtk


def open_url(url):
    for opener in ("xdg-open", "exo-open"):
        path = shutil.which(opener)
        if path:
            try:
                subprocess.Popen([path, url], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
                return True
            except OSError:
                pass
    return False


def run_setup(key="claude"):
    """Activation d'un assistant : téléchargement, connexion au compte, bouton."""
    backend = provider(key)
    name = display_name(key)
    Gdk, GLib, Gtk = _gtk()
    # Une seule fenêtre par assistant (#910) : un deuxième clic sur l'interrupteur
    # ramène celle qui est ouverte (elle a pu passer derrière les Paramètres).
    from gi.repository import Gio
    app = Gtk.Application(application_id=f"org.winlux.AssistantSetup.{key}", flags=Gio.ApplicationFlags.FLAGS_NONE)
    try:
        app.register(None)
    except GLib.Error:
        pass  # pas de bus de session : fenêtre simple
    if app.get_is_registered() and app.get_is_remote():
        app.activate()
        return
    heading_text = _t("Assistant IA : activer {name}", name=name)
    win = Gtk.Window(title=heading_text)
    win.set_role("winlux-assistant-setup")
    win.set_default_size(540, -1)
    win.set_resizable(False)
    win.set_position(Gtk.WindowPosition.CENTER)
    win.set_icon_name("winlux-assistant")
    win.get_style_context().add_class("assistant")
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12, margin=24)
    title = Gtk.Label(xalign=0)
    heading = GLib.markup_escape_text(heading_text)
    title.set_markup(f"<span size='x-large' weight='bold'>{heading}</span>")
    box.pack_start(title, False, False, 0)
    if not backend.remote:
        # Ce qui sera pris sur ce PC, dit avant le téléchargement (#928).
        cost = Gtk.Label(label=plan_text(backend.plan()), xalign=0, wrap=True, max_width_chars=62)
        cost.get_style_context().add_class("hint")
        box.pack_start(cost, False, False, 0)
    steps = []
    for text in backend.steps():
        label = Gtk.Label(label="○  " + text, xalign=0)
        box.pack_start(label, False, False, 0)
        steps.append((label, text))
    bar = Gtk.ProgressBar()
    box.pack_start(bar, False, False, 0)
    status = Gtk.Label(xalign=0, wrap=True, max_width_chars=62, selectable=True)
    box.pack_start(status, False, False, 0)
    code_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
    code_box.pack_start(Gtk.Label(label=_t("Si la page affiche un code, collez-le ici :"), xalign=0), False, False, 0)
    code_line = Gtk.Box(spacing=8)
    code = Gtk.Entry(hexpand=True)
    send_code = Gtk.Button(label=_t("Valider"))
    code_line.pack_start(code, True, True, 0)
    code_line.pack_start(send_code, False, False, 0)
    code_box.pack_start(code_line, False, False, 0)
    box.pack_start(code_box, False, False, 0)
    # Sans navigateur sur ce PC (#912) : le lien à ouvrir sur un autre appareil.
    link_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
    link_text = Gtk.Label(xalign=0, wrap=True, max_width_chars=62)
    link_url = Gtk.Label(xalign=0, wrap=True, max_width_chars=62, selectable=True)
    link_url.set_line_wrap_mode(2)  # Pango.WrapMode.CHAR : une URL n'a pas d'espace
    device_code = Gtk.Label(xalign=0, selectable=True)
    link_line = Gtk.Box(spacing=8)
    copy_link = Gtk.Button(label=_t("Copier le lien"))
    get_browser = Gtk.Button(label=_t("Installer un navigateur"))
    link_line.pack_start(copy_link, False, False, 0)
    link_line.pack_start(get_browser, False, False, 0)
    for widget in (link_text, link_url, device_code, link_line):
        link_box.pack_start(widget, False, False, 0)
    box.pack_start(link_box, False, False, 0)
    box.reorder_child(link_box, box.get_children().index(code_box))  # le lien, puis le code à coller
    buttons = Gtk.Box(spacing=8, halign=Gtk.Align.END)
    open_page = Gtk.Button(label=_t("Ouvrir la page de connexion"))
    retry = Gtk.Button(label=_t("Réessayer"))
    finish = Gtk.Button(label=_t("Ouvrir l'assistant"))
    finish.get_style_context().add_class("suggested-action")
    close = Gtk.Button(label=_t("Annuler"))
    for button in (open_page, retry, finish, close):
        buttons.pack_start(button, False, False, 0)
    box.pack_start(buttons, False, False, 0)
    # Fermer avant la fin laisse l'assistant désactivé : c'est dit, et (#1001)
    # confirmé avant d'arrêter (pas de réglage caché).
    notice = Gtk.Label(label=_t("Tant que l'activation n'est pas terminée, {name} reste désactivé. "
                                "Fermer cette fenêtre l'arrête.", name=name),
                       xalign=0, wrap=True, max_width_chars=62)
    notice.get_style_context().add_class("hint")
    box.pack_start(notice, False, False, 0)
    win.add(box)
    state = {"cancel": threading.Event(), "process": None, "url": None, "code": None, "browser": True,
             "finished": False}

    def mark(index, done=True):
        label, text = steps[index]
        label.set_text(("✓  " if done else "●  ") + text)
        return False

    def show(fraction, text):
        if fraction is None:
            bar.pulse()
        else:
            bar.set_fraction(fraction)
        status.set_text(text)
        return False

    def failed(text):
        state["finished"] = True
        status.set_text(text)
        code_box.hide()
        link_box.hide()
        open_page.hide()
        retry.show()
        close.set_label(_t("Fermer"))
        return False

    def ready(status_info):
        state["finished"] = True
        notice.hide()
        for index in range(len(steps) - 1):
            mark(index)
        enable(key)
        mark(len(steps) - 1)
        bar.set_fraction(1.0)
        if backend.remote:
            who = account_name(status_info)
            done = _t("Connecté au compte {name}{who}.", name=name, who=f" ({who})" if who and who != name else "")
        else:
            done = _t("L'IA locale est installée sur ce PC : rien de ce que vous lui demandez ne quitte l'ordinateur.")
        status.set_text(done + "\n\n" + _t("C'est prêt ! Cliquez sur le bouton de l'assistant dans la barre des "
                                           "tâches ou appuyez sur Windows+C."))
        code_box.hide()
        link_box.hide()
        open_page.hide()
        finish.show()
        finish.grab_focus()
        close.set_label(_t("Fermer"))
        return False

    def got_url():
        if backend.code_entry:
            code_box.show_all()
        if state["browser"]:
            open_page.show()
            if backend.open_login_page:
                open_url(state["url"])
            return False
        # Aucun navigateur : rien n'est ouvert ici, le lien part sur un autre appareil.
        status.set_text("")
        link_text.set_text(_t("Aucun navigateur n'est installé sur ce PC. Ouvrez ce lien sur votre téléphone ou un "
                              "autre ordinateur, connectez-vous, puis collez ici le code affiché.") if backend.code_entry
                           else _t("Aucun navigateur n'est installé sur ce PC. Ouvrez ce lien sur votre téléphone ou "
                                   "un autre ordinateur, connectez-vous, puis saisissez ce code :"))
        link_url.set_text(state["url"])
        link_box.show_all()
        device_code.set_visible(bool(state["code"]))
        return False

    def got_code():
        device_code.set_markup(f"<span size='x-large' weight='bold'>{GLib.markup_escape_text(state['code'])}</span>")
        device_code.show()
        return False

    def login_output(process):
        for line in iter(process.stdout.readline, ""):
            line = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", line)  # couleurs du terminal
            found = re.search(r"https://\S+", line)
            if found and not state["url"]:
                state["url"] = found.group(0)
                GLib.idle_add(got_url)
            elif state["url"] and not state["code"] and not state["browser"]:
                code_found = re.search(r"\b[A-Z0-9]{4}-[A-Z0-9]{4,6}\b", line)
                if code_found:
                    state["code"] = code_found.group(0)
                    GLib.idle_add(got_code)
        process.wait()
        backend.protect()
        info = backend.status()
        if info.get("loggedIn"):
            GLib.idle_add(ready, info)
        elif not state["cancel"].is_set():
            GLib.idle_add(failed, login_failure(process.returncode))

    def login(launcher):
        mark(1, False)
        bar.set_fraction(0.0)
        status.set_text(_t("Connectez-vous dans la page qui vient de s'ouvrir (Google, e-mail…)."))
        state["browser"] = has_browser()
        env = backend.env()
        if not state["browser"]:
            # Ni claude.exe ni codex n'ouvrent « Ouvrir le lien » par-dessus cette fenêtre.
            env.update(WINLUX_BROWSER="true", BROWSER="true")
            status.set_text("")
        try:
            # Groupe de processus à part : Annuler ferme aussi ce que la connexion a lancé.
            process = subprocess.Popen(backend.login_command(launcher, state["browser"]), stdin=subprocess.PIPE,
                                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1,
                                       env=env, cwd=work_dir(), start_new_session=True)
        except OSError as error:
            return failed(_t("La connexion n'a pas abouti : {e}", e=error))
        state["process"] = process
        threading.Thread(target=login_output, args=(process,), daemon=True).start()
        return False

    def work():
        launcher = backend.launcher()
        try:
            if launcher is None:
                launcher = backend.install(lambda f, t: GLib.idle_add(show, f, t), state["cancel"],
                                           lambda index: GLib.idle_add(mark, index))
            else:
                for index in range(len(steps) - (2 if backend.remote else 1)):
                    GLib.idle_add(mark, index)
            if launcher is None:
                raise RuntimeError(_t("Archive inattendue : {name}", name=name))
            GLib.idle_add(show, None, _t("Vérification de la connexion…") if backend.remote else "")
            info = backend.status(launcher)
        except Exception as error:  # réseau, disque, empreinte, annulation
            if not state["cancel"].is_set():
                GLib.idle_add(failed, _t("Le téléchargement a échoué : {e}", e=error))
            return
        if info.get("loggedIn"):
            GLib.idle_add(ready, info)
        else:
            GLib.idle_add(login, launcher)

    def start(*_):
        retry.hide()
        finish.hide()
        open_page.hide()
        code_box.hide()
        close.set_label(_t("Annuler"))
        state["finished"] = False  # « Réessayer » : une nouvelle activation en cours
        state["cancel"] = threading.Event()
        state["url"] = None
        state["code"] = None
        link_box.hide()
        mark(0, False)
        threading.Thread(target=work, daemon=True).start()

    def submit_code(*_):
        process = state["process"]
        if process and process.poll() is None and code.get_text().strip():
            try:
                process.stdin.write(code.get_text().strip() + "\n")
                process.stdin.flush()
            except OSError:
                pass
            code.set_text("")

    def quit_setup(*_):
        state["cancel"].set()
        process = state["process"]
        if process and process.poll() is None:
            try:
                os.killpg(process.pid, 15)  # et le navigateur lancé par la connexion (#912)
            except OSError:
                process.terminate()
        Gtk.main_quit()

    def stop_confirmed():
        """Vrai si la fenêtre peut se fermer : activation finie, ou arrêt
        confirmé. « Continuer l'activation » est le choix par défaut."""
        if state["finished"]:
            return True
        dialog = Gtk.MessageDialog(transient_for=win, modal=True, message_type=Gtk.MessageType.QUESTION,
                                   text=_t("Arrêter l'activation de {name} ?", name=name))
        dialog.format_secondary_text(_t("{name} restera désactivé. Vous pourrez le réactiver plus tard dans "
                                        "Paramètres > Assistant IA.", name=name))
        dialog.add_button(_t("Arrêter"), Gtk.ResponseType.REJECT)
        keep = dialog.add_button(_t("Continuer l'activation"), Gtk.ResponseType.ACCEPT)
        # Le choix par défaut se voit : bouton violet Prisme, focus clavier (#1036).
        keep.get_style_context().add_class("suggested-action")
        keep.set_can_default(True)
        dialog.set_default_response(Gtk.ResponseType.ACCEPT)
        keep.grab_focus()
        answer = dialog.run()
        dialog.destroy()
        return answer == Gtk.ResponseType.REJECT

    def request_close(*_):
        if stop_confirmed():
            win.destroy()

    def copy_url(*_):
        if state["url"]:
            Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).set_text(state["url"], -1)
            status.set_text(_t("Lien copié."))

    def install_browser(*_):
        # Choix explicite : la page Navigateurs des Paramètres (Edge, Firefox, Chrome).
        try:
            subprocess.Popen(["winlux-shell-app", "settings", "11"], start_new_session=True)
        except OSError:
            pass

    def open_assistant(*_):
        subprocess.Popen([sys.executable, str(Path(__file__).resolve())], start_new_session=True)
        quit_setup()

    open_page.connect("clicked", lambda *_: state["url"] and open_url(state["url"]))
    copy_link.connect("clicked", copy_url)
    get_browser.connect("clicked", install_browser)
    send_code.connect("clicked", submit_code)
    code.connect("activate", submit_code)
    retry.connect("clicked", start)
    finish.connect("clicked", open_assistant)
    close.connect("clicked", request_close)
    win.connect("delete-event", lambda *_: not stop_confirmed())
    win.connect("destroy", quit_setup)
    app.connect("activate", lambda *_: win.present())
    app.hold()
    win.show_all()
    link_box.hide()
    win.present()
    start()
    Gtk.main()


def run_chat():
    Gdk, GLib, Gtk = _gtk()
    from gi.repository import Gio

    class Window(Gtk.ApplicationWindow):
        def __init__(self, app):
            super().__init__(application=app, title=_t("Assistant IA"))
            self.set_role("winlux-assistant")
            self.set_icon_name("winlux-assistant")
            self.set_decorated(False)
            self.set_keep_above(True)
            self.set_skip_taskbar_hint(True)  # le bouton de la barre l'ouvre et la ferme
            self.get_style_context().add_class("assistant")
            self.key = current_provider() or "claude"
            self.conversation = Conversation(self.key)
            self.server = LocalServer()
            self.local = LocalAssistant(self.server)
            self.busy = False
            self.idle_source = None  # arrêt du moteur après LOCAL_IDLE_SECONDS sans question (#928)
            self.watch_source = None
            self.place()
            outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
            outer.get_style_context().add_class("assistant-body")
            self.add(outer)
            head = Gtk.Box(spacing=10)
            head.get_style_context().add_class("assistant-head")
            head.pack_start(Gtk.Image.new_from_icon_name("winlux-assistant", Gtk.IconSize.LARGE_TOOLBAR), False, False, 0)
            enabled = enabled_providers()
            if len(enabled) > 1:
                # Plusieurs assistants activés : on choisit ici, le choix est retenu.
                self.chooser = Gtk.ComboBoxText()
                for key in enabled:
                    self.chooser.append(key, display_name(key))
                self.chooser.set_active_id(self.key)
                self.chooser.connect("changed", lambda combo: self.switch_provider(combo.get_active_id()))
                head.pack_start(self.chooser, True, True, 0)
                self.title = None
            else:
                self.chooser = None
                self.title = Gtk.Label(xalign=0)
                self.title.get_style_context().add_class("assistant-title")
                head.pack_start(self.title, True, True, 0)
            new = Gtk.Button.new_from_icon_name("view-refresh-symbolic", Gtk.IconSize.BUTTON)
            new.set_tooltip_text(_t("Nouvelle conversation"))
            new.connect("clicked", lambda *_: self.reset())
            head.pack_start(new, False, False, 0)
            close = Gtk.Button.new_from_icon_name("window-close-symbolic", Gtk.IconSize.BUTTON)
            close.set_tooltip_text(_t("Fermer"))
            close.connect("clicked", lambda *_: self.close())
            head.pack_start(close, False, False, 0)
            outer.pack_start(head, False, False, 0)
            self.scroll = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER)
            self.messages = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=14)
            self.scroll.add(self.messages)
            outer.pack_start(self.scroll, True, True, 0)
            bottom = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6, margin=12)
            line = Gtk.Box(spacing=8)
            self.entry = Gtk.Entry(placeholder_text=_t("Posez votre question…"), hexpand=True)
            self.entry.connect("activate", lambda *_: self.send())
            self.send_button = Gtk.Button(label=_t("Envoyer"))
            self.send_button.get_style_context().add_class("suggested-action")
            self.send_button.connect("clicked", lambda *_: self.send())
            line.pack_start(self.entry, True, True, 0)
            line.pack_start(self.send_button, False, False, 0)
            bottom.pack_start(line, False, False, 0)
            self.hint = Gtk.Label(xalign=0, wrap=True, max_width_chars=56)
            self.hint.get_style_context().add_class("hint")
            bottom.pack_start(self.hint, False, False, 0)
            outer.pack_start(bottom, False, False, 0)
            self.connect("key-press-event", self.on_key)
            self.connect("destroy", lambda *_: self.stop_local())
            self.start_state()

        @property
        def name(self):
            return display_name(self.key)

        def place(self):
            """Panneau en bas à droite, au-dessus de la barre des tâches."""
            width, height = 420, 640
            display = Gdk.Display.get_default()
            monitor = display.get_primary_monitor() or display.get_monitor(0)
            area = monitor.get_workarea()
            height = min(height, area.height - 24)
            self.set_default_size(width, height)
            self.move(area.x + area.width - width - 12, area.y + area.height - height - 12)

        def on_key(self, _, event):
            if event.keyval == Gdk.KEY_Escape:
                self.close()
                return True
            return False

        def scroll_down(self):
            adjustment = self.scroll.get_vadjustment()
            GLib.timeout_add(60, lambda: adjustment.set_value(adjustment.get_upper()) or False)

        def bubble(self, text, kind):
            label = Gtk.Label(label=text, xalign=0, wrap=True, selectable=True, max_width_chars=40)
            label.get_style_context().add_class({"user": "bubble-user", "claude": "bubble-claude"}.get(kind, "bubble-note"))
            label.set_halign(Gtk.Align.END if kind == "user" else Gtk.Align.START)
            self.messages.pack_start(label, False, False, 0)
            label.show()
            self.scroll_down()
            return label

        def switch_provider(self, key):
            if key and key != self.key and not self.busy:
                write_config(current=key)
                self.key = key
                self.stop_local()
                self.reset()

        def stop_local(self):
            for name in ("idle_source", "watch_source"):
                source = getattr(self, name)
                if source is not None:
                    GLib.source_remove(source)
                    setattr(self, name, None)
            self.server.stop()

        def local_hint(self):
            """Ce que l'IA locale prend en ce moment, dit franchement (#928)."""
            if provider(self.key).remote:
                self.watch_source = None
                return False
            used = self.server.memory()
            if used:
                gpu = _t(" et la carte graphique") if provider("local").gpu_layers() else ""
                self.hint.set_text(_t("IA locale en marche : elle prend {memory} de mémoire{gpu} et fait travailler "
                                      "l'ordinateur. Elle s'arrête seule deux minutes après votre dernière question.",
                                      memory=size_text(used), gpu=gpu))
            elif self.server.running():
                self.hint.set_text(_t("Démarrage de l'IA locale…"))
            else:
                self.hint.set_text(_t("IA locale en pause : elle ne prend ni mémoire ni processeur. Elle redémarre "
                                      "à votre prochaine question. Rien n'est envoyé sur Internet."))
            return True

        def watch_local(self):
            if self.watch_source is None:
                self.watch_source = GLib.timeout_add_seconds(3, self.local_hint)
            self.local_hint()

        def arm_idle(self):
            if self.idle_source is not None:
                GLib.source_remove(self.idle_source)
            self.idle_source = GLib.timeout_add_seconds(LOCAL_IDLE_SECONDS, self.idle_stop)

        def idle_stop(self):
            self.idle_source = None
            if self.busy:
                self.arm_idle()
                return False
            if self.server.running():
                self.server.stop()
                self.bubble(_t("Pause : l'IA locale s'est arrêtée pour rendre la mémoire. Elle redémarre à votre "
                               "prochaine question."), "note")
            self.local_hint()
            return False

        def reset(self):
            self.conversation = Conversation(self.key)
            for child in self.messages.get_children():
                child.destroy()
            self.start_state()

        def start_state(self):
            if self.title is not None:
                self.title.set_text(_t("Assistant IA") + " · " + self.name)
            if provider(self.key).remote:
                company = {"claude": "Anthropic", "chatgpt": "OpenAI"}[self.key]
                self.hint.set_text(_t("Envoyé à {name} ({company}) : votre question et cette conversation. "
                                      "Rien d'autre sans votre accord.", name=self.name, company=company))
            else:
                self.watch_local()
            if not is_enabled(self.key):
                self.blocked(_t("L'assistant est désactivé. Activez-le dans Paramètres."))
                return
            self.bubble(_t("Bonjour ! Je peux vous aider à utiliser Winlux, par exemple :"), "claude")
            for text in (_t("Pourquoi mon programme .exe ne s'ouvre pas ?"), _t("Où est mon fichier… ?"),
                         _t("Passe au thème sombre")):
                chip = Gtk.Button(label=text, halign=Gtk.Align.START)
                chip.get_style_context().add_class("chip")
                chip.connect("clicked", lambda _, value=text: self.suggest(value))
                self.messages.pack_start(chip, False, False, 0)
            self.messages.show_all()
            if not provider(self.key).remote:
                self.start_local()

        def start_local(self):
            """Le moteur local ne tourne que fenêtre ouverte."""
            self.set_busy(True)
            waiting = self.bubble(_t("Démarrage de l'IA locale…"), "note")

            def work():
                try:
                    self.server.wait_ready()
                    error = None
                except Exception as problem:
                    error = str(problem)
                GLib.idle_add(started, error)

            def started(error):
                waiting.destroy()
                self.set_busy(False)
                if error:
                    self.blocked(_t("L'IA locale n'a pas démarré : {e}", e=error))
                else:
                    self.arm_idle()
                self.local_hint()
                self.entry.grab_focus()
                return False
            try:
                self.server.spawn()  # fil principal : le moteur meurt avec l'assistant
            except Exception as problem:
                return started(str(problem))
            threading.Thread(target=work, daemon=True).start()

        def suggest(self, text):
            # « Où est mon fichier… ? » attend le nom : curseur après « fichier ».
            self.entry.set_text(text.split("…")[0] + " " if "…" in text else text)
            self.entry.grab_focus_without_selecting()
            self.entry.set_position(-1)

        def blocked(self, text):
            self.bubble(text, "claude")
            button = Gtk.Button(label=_t("Terminer la configuration"), halign=Gtk.Align.START)
            button.get_style_context().add_class("suggested-action")
            button.connect("clicked", lambda *_: subprocess.Popen(["winlux-shell-app", "settings", "30"],
                                                                  start_new_session=True) and self.close())
            self.messages.pack_start(button, False, False, 0)
            button.show()

        def set_busy(self, busy):
            self.busy = busy
            self.send_button.set_sensitive(not busy)
            self.entry.set_sensitive(not busy)
            if self.chooser is not None:
                self.chooser.set_sensitive(not busy)

        def send(self):
            text = self.entry.get_text().strip()
            if not text or self.busy or not is_enabled(self.key):
                return
            self.entry.set_text("")
            self.bubble(text, "user")
            if provider(self.key).remote:
                self.conversation.entries.append(("user", text))
                self.ask()
            else:
                self.ask_local(text)

        def ask(self):
            self.set_busy(True)
            waiting = self.bubble(_t("{name} réfléchit…", name=self.name), "note")

            def work():
                try:
                    outcome = ("ok",) + self.conversation.ask()
                except NotSignedIn:
                    outcome = ("login",)
                except subprocess.TimeoutExpired:
                    outcome = ("error", "timeout")
                except Exception as error:
                    outcome = ("error", str(error))
                GLib.idle_add(self.answered, waiting, outcome)
            threading.Thread(target=work, daemon=True).start()

        def answered(self, waiting, outcome):
            waiting.destroy()
            self.set_busy(False)
            if outcome[0] == "login":
                self.blocked(_t("Vous n'êtes pas connecté à votre compte {name}.", name=self.name))
                return False
            if outcome[0] == "error":
                self.bubble(_t("{name} n'a pas répondu : {e}", name=self.name, e=outcome[1]), "note")
                return False
            _, reply, kind, argument = outcome
            if reply:
                self.bubble(reply, "claude")
            reading = describe_request(kind, argument)
            if reading:
                self.request_card(*reading)
            else:
                action = describe_action(kind, argument)
                if action:
                    self.action_card(*action)
            self.entry.grab_focus()
            return False

        def ask_local(self, text):
            self.set_busy(True)
            waiting = self.bubble(_t("{name} réfléchit…", name=self.name), "note")
            restart = None
            if not self.server.running():
                # En pause après inactivité : relancé ici, dans le fil principal
                # (le moteur meurt avec l'assistant, PR_SET_PDEATHSIG).
                try:
                    self.server.spawn()
                except Exception as problem:
                    restart = str(problem)
            self.local_hint()

            def work():
                try:
                    if restart:
                        raise RuntimeError(restart)
                    self.server.wait_ready()
                    outcome = ("ok", self.local.handle(text))
                except Exception as error:
                    outcome = ("error", str(error))
                GLib.idle_add(self.answered_local, waiting, outcome)
            threading.Thread(target=work, daemon=True).start()

        def answered_local(self, waiting, outcome):
            waiting.destroy()
            self.set_busy(False)
            self.arm_idle()
            self.local_hint()
            if outcome[0] == "error":
                self.bubble(_t("{name} n'a pas répondu : {e}", name=self.name, e=outcome[1]), "note")
                return False
            result = outcome[1]
            for what, data in result["reads"]:
                self.local_read_card(what, data)
            if result["reply"]:
                self.bubble(result["reply"], "claude")
            for what, command in result["actions"]:
                self.action_card(what, command)
            self.entry.grab_focus()
            return False

        def card(self, title):
            box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
            box.get_style_context().add_class("request")
            box.pack_start(Gtk.Label(label=title, xalign=0, wrap=True, max_width_chars=44), False, False, 0)
            self.messages.pack_start(box, False, False, 0)
            return box

        def data_view(self, box, label, data):
            expander = Gtk.Expander(label=label)
            view = Gtk.TextView(editable=False, wrap_mode=Gtk.WrapMode.WORD_CHAR, cursor_visible=False)
            view.get_buffer().set_text(data)
            frame = Gtk.ScrolledWindow(min_content_height=140, max_content_height=220, propagate_natural_height=True)
            frame.add(view)
            expander.add(frame)
            box.pack_start(expander, False, False, 0)

        def local_read_card(self, what, data):
            """Lecture de l'IA locale : faite sur ce PC, montrée, jamais envoyée."""
            box = self.card(_t("Lu sur ce PC (rien n'est envoyé) : {what}", what=what))
            self.data_view(box, _t("Voir ce qui a été lu"), data)
            box.show_all()
            self.scroll_down()

        def request_card(self, what, reader):
            """Lecture demandée : le texte exact est montré avant tout envoi."""
            box = self.card(_t("{name} demande à lire : {what}", name=self.name, what=what))
            data = reader()
            self.data_view(box, _t("Voir exactement ce qui sera envoyé"), data)
            line = Gtk.Box(spacing=8)
            accept = Gtk.Button(label=_t("Envoyer à {name}", name=self.name))
            accept.get_style_context().add_class("suggested-action")
            refuse = Gtk.Button(label=_t("Refuser"))
            line.pack_start(accept, False, False, 0)
            line.pack_start(refuse, False, False, 0)
            box.pack_start(line, False, False, 0)

            def answer(send):
                line.destroy()
                if send:
                    self.conversation.entries.append(("data", data))
                    self.ask()
                else:
                    self.conversation.entries.append(("user", "(Refusé : ne lis pas cette information.)"))
                    self.bubble(_t("Refusé : rien n'a été envoyé."), "note")
            accept.connect("clicked", lambda *_: answer(True))
            refuse.connect("clicked", lambda *_: answer(False))
            box.show_all()
            self.scroll_down()

        def action_card(self, what, command):
            """Action proposée : rien ne change sans un clic sur « Faire »."""
            box = self.card(_t("{name} propose : {what}", name=self.name, what=what))
            detail = Gtk.Label(label=" ".join(command), xalign=0, selectable=True, wrap=True, max_width_chars=50)
            detail.get_style_context().add_class("hint")
            box.pack_start(detail, False, False, 0)
            line = Gtk.Box(spacing=8)
            accept = Gtk.Button(label=_t("Faire"))
            accept.get_style_context().add_class("suggested-action")
            refuse = Gtk.Button(label=_t("Non merci"))
            line.pack_start(accept, False, False, 0)
            line.pack_start(refuse, False, False, 0)
            box.pack_start(line, False, False, 0)

            def answer(do):
                line.destroy()
                if do:
                    error = run_action(command)
                    self.bubble(_t("L'action a échoué : {e}", e=error) if error else _t("Fait."), "note")
                    outcome = f"échec : {error}" if error else "faite"
                    self.conversation.entries.append(("user", f"(Action {' '.join(command)} {outcome}.)"))
                else:
                    self.bubble(_t("Refusé : rien n'a été changé."), "note")
                    self.conversation.entries.append(("user", "(Action refusée.)"))
            accept.connect("clicked", lambda *_: answer(True))
            refuse.connect("clicked", lambda *_: answer(False))
            box.show_all()
            self.scroll_down()

    class App(Gtk.Application):
        """Une seule fenêtre : un nouveau clic sur le bouton (ou Windows+C) la ferme."""

        def __init__(self):
            super().__init__(application_id="org.winlux.Assistant", flags=Gio.ApplicationFlags.FLAGS_NONE)
            self.window = None

        def do_activate(self):
            if self.window is not None:
                self.window.close()
                return
            self.window = Window(self)
            self.window.connect("destroy", lambda *_: setattr(self, "window", None))
            self.window.show_all()
            self.window.present()
            self.window.entry.grab_focus()

    return App().run(None)


def main(argv):
    command = argv[1] if len(argv) > 1 else ""
    key = argv[2] if len(argv) > 2 else "claude"
    if command.startswith("--") and command != "--etat" and key not in PROVIDERS:
        print(f"winlux-assistant : assistant inconnu « {key} » ({', '.join(PROVIDERS)})", file=sys.stderr)
        return 2
    if command == "--activer":
        enable(key)
        return 0
    if command == "--desactiver":
        disable(key)
        return 0
    if command == "--deconnecter":
        provider(key).logout()
        disable(key)
        return 0
    if command == "--supprimer":
        disable(key)
        if key == "local":
            provider("local").remove()
        return 0
    if command == "--etat":
        state = {}
        for name in PROVIDERS:
            backend = provider(name)
            status = backend.status() if backend.supported() and backend.launcher() else {}
            state[name] = {"supported": backend.supported(), "enabled": is_enabled(name),
                           "installed": bool(status.get("installed")), "loggedIn": bool(status.get("loggedIn")),
                           "account": account_name(status)}
        print(json.dumps(state))
        return 0
    if command == "--configurer":
        run_setup(key)
        return 0
    # Bouton de la barre ou Windows+C : rien du tout tant qu'aucun assistant n'est activé.
    if not is_enabled():
        return 0
    protect_credentials()
    return run_chat()


if __name__ == "__main__":
    sys.exit(main(sys.argv))

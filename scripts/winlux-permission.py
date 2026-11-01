#!/usr/bin/python3
"""Autorisations des applications Windows (.exe), au style de Windows 11.

Usage :
  winlux-permission ask CAPACITÉ NOM CHEMIN_EXE   fenêtre « Autoriser … ? »
        code de sortie 0 : autorisé, 1 : refusé, 2 : fermée sans réponse
  winlux-permission list                          décisions mémorisées (TSV)
  winlux-permission set CAPACITÉ allow|deny CHEMIN_EXE
  winlux-permission forget CHEMIN_EXE             tout redemander à cette appli

Capacités : files, camera, microphone, screen, network. Le runtime Winlux
(runtime/src/permissions.rs) pose la question et mémorise la réponse dans
~/.config/winlux/permissions.tsv (capacité<TAB>allow|deny<TAB>chemin).
"""
import os
import sys
from pathlib import Path


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
    "Autoriser « {app} » à accéder à vos fichiers personnels ?": (
        "Allow “{app}” to access your personal files?",
        "¿Permitir que «{app}» acceda a sus archivos personales?",
        "Darf „{app}“ auf Ihre persönlichen Dateien zugreifen?"),
    "Autoriser « {app} » à utiliser votre caméra ?": (
        "Allow “{app}” to use your camera?",
        "¿Permitir que «{app}» use la cámara?",
        "Darf „{app}“ Ihre Kamera verwenden?"),
    "Autoriser « {app} » à utiliser votre micro ?": (
        "Allow “{app}” to use your microphone?",
        "¿Permitir que «{app}» use el micrófono?",
        "Darf „{app}“ Ihr Mikrofon verwenden?"),
    "Autoriser « {app} » à voir votre écran ?": (
        "Allow “{app}” to see your screen?",
        "¿Permitir que «{app}» vea su pantalla?",
        "Darf „{app}“ Ihren Bildschirm sehen?"),
    "Autoriser « {app} » à utiliser le réseau ?": (
        "Allow “{app}” to use the network?",
        "¿Permitir que «{app}» use la red?",
        "Darf „{app}“ das Netzwerk verwenden?"),
    "Documents, Images, Musique, Vidéos, Bureau et Téléchargements. L’application pourra les lire et les modifier.": (
        "Documents, Pictures, Music, Videos, Desktop and Downloads. The app will be able to read and change them.",
        "Documentos, Imágenes, Música, Vídeos, Escritorio y Descargas. La aplicación podrá leerlos y modificarlos.",
        "Dokumente, Bilder, Musik, Videos, Desktop und Downloads. Die App kann sie lesen und ändern."),
    "L’application pourra voir l’image de votre webcam tant qu’elle est ouverte.": (
        "The app will be able to see your webcam picture while it is open.",
        "La aplicación podrá ver la imagen de su cámara web mientras esté abierta.",
        "Die App kann das Bild Ihrer Webcam sehen, solange sie geöffnet ist."),
    "L’application pourra entendre ce que capte votre micro tant qu’elle est ouverte.": (
        "The app will be able to hear what your microphone picks up while it is open.",
        "La aplicación podrá oír lo que capta el micrófono mientras esté abierta.",
        "Die App kann hören, was Ihr Mikrofon aufnimmt, solange sie geöffnet ist."),
    "L’application pourra capturer tout ce qui s’affiche, y compris les fenêtres des autres programmes.": (
        "The app will be able to capture everything on screen, including other programs' windows.",
        "La aplicación podrá capturar todo lo que se muestra, incluidas las ventanas de otros programas.",
        "Die App kann alles erfassen, was angezeigt wird, auch die Fenster anderer Programme."),
    "L’application pourra se connecter à Internet et à votre réseau local.": (
        "The app will be able to connect to the Internet and your local network.",
        "La aplicación podrá conectarse a Internet y a su red local.",
        "Die App kann sich mit dem Internet und Ihrem lokalen Netzwerk verbinden."),
    "Autoriser": ("Allow", "Permitir", "Zulassen"),
    "Ne pas autoriser": ("Don't allow", "No permitir", "Nicht zulassen"),
    "Vous pourrez changer d’avis dans Paramètres > Confidentialité et sécurité.": (
        "You can change your mind in Settings > Privacy & security.",
        "Puede cambiar de opinión en Configuración > Privacidad y seguridad.",
        "Sie können dies unter Einstellungen > Datenschutz und Sicherheit ändern."),
    "Demande d’autorisation": ("Permission request", "Solicitud de permiso", "Berechtigungsanfrage"),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


CAPABILITIES = ("files", "camera", "microphone", "screen", "network")
QUESTIONS = {
    "files": ("Autoriser « {app} » à accéder à vos fichiers personnels ?",
              "Documents, Images, Musique, Vidéos, Bureau et Téléchargements. L’application pourra les lire et les modifier.",
              "folder-documents"),
    "camera": ("Autoriser « {app} » à utiliser votre caméra ?",
               "L’application pourra voir l’image de votre webcam tant qu’elle est ouverte.", "camera-web"),
    "microphone": ("Autoriser « {app} » à utiliser votre micro ?",
                   "L’application pourra entendre ce que capte votre micro tant qu’elle est ouverte.",
                   "audio-input-microphone"),
    "screen": ("Autoriser « {app} » à voir votre écran ?",
               "L’application pourra capturer tout ce qui s’affiche, y compris les fenêtres des autres programmes.",
               "video-display"),
    "network": ("Autoriser « {app} » à utiliser le réseau ?",
                "L’application pourra se connecter à Internet et à votre réseau local.", "network-workgroup"),
}
TIMEOUT_SECONDS = 120


# ── Mémoire des choix (même format que le runtime) ──────────────────────────

def store_path():
    if os.environ.get("WINLUX_PERMISSIONS_FILE"):
        return Path(os.environ["WINLUX_PERMISSIONS_FILE"])
    config = os.environ.get("XDG_CONFIG_HOME") or str(Path.home() / ".config")
    return Path(config) / "winlux/permissions.tsv"


def load():
    """[(capacité, décision, chemin)] dans l'ordre du fichier, une ligne par paire."""
    entries = {}
    try:
        text = store_path().read_text(errors="replace")
    except OSError:
        return []
    for line in text.splitlines():
        parts = line.split("\t", 2)
        if len(parts) == 3 and parts[0] in CAPABILITIES and parts[1] in ("allow", "deny") and parts[2]:
            entries[(parts[0], parts[2])] = parts[1]
    return [(cap, decision, exe) for (cap, exe), decision in entries.items()]


def save(entries):
    path = store_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + f".tmp.{os.getpid()}")
    tmp.write_text("".join(f"{cap}\t{decision}\t{exe}\n" for cap, decision, exe in entries))
    os.replace(tmp, path)


def set_decision(cap, decision, exe):
    if cap not in CAPABILITIES or decision not in ("allow", "deny") or not exe:
        raise ValueError("capacité, décision ou programme invalide")
    entries = [e for e in load() if not (e[0] == cap and e[2] == exe)]
    entries.append((cap, decision, exe))
    save(entries)


def forget(exe):
    save([e for e in load() if e[2] != exe])


def by_app():
    """{chemin: {capacité: décision}} trié par nom d'application."""
    apps = {}
    for cap, decision, exe in load():
        apps.setdefault(exe, {})[cap] = decision
    return dict(sorted(apps.items(), key=lambda item: Path(item[0]).name.casefold()))


# ── Fenêtre de demande ──────────────────────────────────────────────────────

def ask(cap, name, exe):
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GLib, Gtk

    question, detail, icon = QUESTIONS[cap]
    result = {"code": 2}
    css = Gtk.CssProvider()
    css.load_from_data(b"""
      window.winlux-consent { background: #202020; color: #f3f3f3; }
      .consent-title { font-size: 19px; font-weight: 600; }
      .consent-detail { color: #d0d0d0; font-size: 13px; }
      .consent-path { color: #9a9a9a; font-size: 11px; }
      .consent-footer { background: #2b2b2b; padding: 18px 24px; }
      .consent-note { color: #a8a8a8; font-size: 11px; }
      button.consent-allow { background: #4cc2ff; color: #000; border: none; border-radius: 4px;
                             padding: 7px 22px; font-weight: 600; box-shadow: none; text-shadow: none; }
      button.consent-allow:hover { background: #48b2e9; }
      button.consent-deny { background: #3a3a3a; color: #f3f3f3; border: 1px solid #4a4a4a; border-radius: 4px;
                            padding: 7px 22px; box-shadow: none; text-shadow: none; }
      button.consent-deny:hover { background: #454545; }
    """)
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), css, 900)
    window = Gtk.Window(title=_t("Demande d’autorisation"))
    window.get_style_context().add_class("winlux-consent")
    window.set_type_hint(Gdk.WindowTypeHint.DIALOG)
    window.set_keep_above(True)
    window.set_resizable(False)
    window.set_position(Gtk.WindowPosition.CENTER)
    window.set_default_size(460, -1)
    window.set_icon_name(icon)

    def finish(code):
        result["code"] = code
        Gtk.main_quit()

    window.connect("delete-event", lambda *_: finish(2) or False)
    outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
    window.add(outer)
    body = Gtk.Box(spacing=16, margin=24)
    outer.pack_start(body, False, False, 0)
    image = Gtk.Image.new_from_icon_name(icon, Gtk.IconSize.DIALOG)
    image.set_valign(Gtk.Align.START)
    body.pack_start(image, False, False, 0)
    text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
    body.pack_start(text, True, True, 0)
    for content, style in ((_t(question).format(app=name), "consent-title"), (_t(detail), "consent-detail"),
                           (exe, "consent-path")):
        label = Gtk.Label(label=content, xalign=0, wrap=True, max_width_chars=48)
        label.get_style_context().add_class(style)
        text.pack_start(label, False, False, 0)
    footer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
    footer.get_style_context().add_class("consent-footer")
    outer.pack_start(footer, False, False, 0)
    buttons = Gtk.Box(spacing=8, homogeneous=True)
    allow = Gtk.Button(label=_t("Autoriser"))
    allow.get_style_context().add_class("consent-allow")
    allow.connect("clicked", lambda *_: finish(0))
    deny = Gtk.Button(label=_t("Ne pas autoriser"))
    deny.get_style_context().add_class("consent-deny")
    deny.connect("clicked", lambda *_: finish(1))
    buttons.pack_start(allow, True, True, 0)
    buttons.pack_start(deny, True, True, 0)
    footer.pack_start(buttons, False, False, 0)
    note = Gtk.Label(label=_t("Vous pourrez changer d’avis dans Paramètres > Confidentialité et sécurité."),
                     xalign=0, wrap=True)
    note.get_style_context().add_class("consent-note")
    footer.pack_start(note, False, False, 0)
    # Par prudence, Entrée ne vaut pas « Autoriser » : le focus va sur le refus.
    window.show_all()
    deny.grab_focus()
    window.present()
    GLib.timeout_add_seconds(TIMEOUT_SECONDS, lambda: finish(2) or False)
    Gtk.main()
    return result["code"]


def main(argv):
    command = argv[1] if len(argv) > 1 else ""
    try:
        if command == "ask" and len(argv) == 5 and argv[2] in QUESTIONS:
            return ask(argv[2], argv[3], argv[4])
        if command == "list" and len(argv) == 2:
            for cap, decision, exe in load():
                print(f"{cap}\t{decision}\t{exe}")
            return 0
        if command == "set" and len(argv) == 5:
            set_decision(argv[2], argv[3], argv[4])
            return 0
        if command == "forget" and len(argv) == 3:
            forget(argv[2])
            return 0
    except (OSError, ValueError) as error:
        print(f"winlux-permission : {error}", file=sys.stderr)
        return 3
    print(__doc__.strip(), file=sys.stderr)
    return 3


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

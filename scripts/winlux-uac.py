#!/usr/bin/python3
"""Contrôle de compte d'utilisateur Winlux, au style de Windows 11.

Usage : winlux-uac --app NOM [--path CHEMIN_WINDOWS]

Affiché par le runtime quand un .exe demande les droits administrateur
(manifeste requireAdministrator, verbe « runas »). Code de sortie 0 si
l'utilisateur accepte et que le mot de passe local est bon, 1 sinon.

Compte administrateur : membre de sudo, wheel ou admin, rôle root/admin de
/etc/winlux/users.conf, ou root. Un administrateur confirme avec son propre
mot de passe ; un compte standard choisit un administrateur et tape le sien,
comme sous Windows. Le mot de passe est vérifié comme sur l'écran de
verrouillage (crypt() sur /etc/shadow, fichier Live), sinon avec
`sudo -S -k` pour son propre compte quand /etc/shadow n'est pas lisible.
"""
import os
import runpy
import subprocess
import sys
from pathlib import Path


def _load_lock():
    """Fonctions de compte et de mot de passe de l'écran de verrouillage."""
    here = Path(__file__).resolve().parent
    candidates = (here / "winlux-lock.py", here / "winlux-lock", here.parent / "share/winlux/winlux-lock.py",
                  Path("/usr/bin/winlux-lock"), Path("/usr/share/winlux/winlux-lock.py"))
    for candidate in candidates:
        if candidate.is_file():
            return runpy.run_path(str(candidate), run_name="winlux_lock_lib")
    raise SystemExit("winlux-uac : winlux-lock introuvable")


LOCK = _load_lock()
_LANG = LOCK["_LANG"]
_COL = {"en": 0, "es": 1, "de": 2}

_TR = {
    "Contrôle de compte d'utilisateur": (
        "User Account Control", "Control de cuentas de usuario", "Benutzerkontensteuerung"),
    "Voulez-vous autoriser cette application à apporter des modifications à votre appareil ?": (
        "Do you want to allow this app to make changes to your device?",
        "¿Quieres permitir que esta aplicación haga cambios en el dispositivo?",
        "Möchten Sie zulassen, dass durch diese App Änderungen an Ihrem Gerät vorgenommen werden?"),
    "Éditeur : inconnu": ("Publisher: Unknown", "Editor: desconocido", "Herausgeber: Unbekannt"),
    "Origine du fichier : disque dur de cet ordinateur": (
        "File origin: Hard drive on this computer",
        "Origen del archivo: unidad de disco duro de este equipo",
        "Dateiursprung: Festplatte auf diesem Computer"),
    "Pour continuer, entrez le mot de passe de {name}.": (
        "To continue, enter the password for {name}.",
        "Para continuar, escribe la contraseña de {name}.",
        "Geben Sie zum Fortfahren das Kennwort für {name} ein."),
    "Pour continuer, entrez le mot de passe d'un administrateur.": (
        "To continue, enter an admin password.",
        "Para continuar, escribe la contraseña de un administrador.",
        "Geben Sie zum Fortfahren ein Administratorkennwort ein."),
    "Mot de passe": ("Password", "Contraseña", "Kennwort"),
    "Le mot de passe est incorrect. Réessayez.": (
        "The password is incorrect. Try again.",
        "La contraseña no es correcta. Inténtelo de nuevo.",
        "Das Kennwort ist falsch. Versuchen Sie es erneut."),
    "Aucun compte administrateur n'existe sur cet appareil.": (
        "There is no administrator account on this device.",
        "No hay ninguna cuenta de administrador en este dispositivo.",
        "Auf diesem Gerät ist kein Administratorkonto vorhanden."),
    "Oui": ("Yes", "Sí", "Ja"),
    "Non": ("No", "No", "Nein"),
}


def _t(text, **values):
    row = _TR.get(text)
    out = row[_COL[_LANG]] if row and _LANG in _COL else text
    return out.format(**values) if values else out


ADMIN_GROUPS = ("sudo", "wheel", "admin")


def admin_accounts():
    """Comptes administrateurs, le compte personnel avant root."""
    names = []
    try:
        for line in LOCK["path"]("/etc/group").read_text(errors="replace").splitlines():
            parts = line.split(":")
            if len(parts) >= 4 and parts[0] in ADMIN_GROUPS:
                names += [m for m in parts[3].split(",") if m]
    except OSError:
        pass
    try:
        for line in LOCK["path"]("/etc/winlux/users.conf").read_text(errors="replace").splitlines():
            parts = line.split(":", 2)
            if len(parts) == 3 and parts[1] in ("root", "admin", "administrateur"):
                names.append(parts[0])
    except OSError:
        pass
    names.append("root")
    seen, out = set(), []
    for name in names:
        if name not in seen:
            seen.add(name)
            out.append(name)
    return [n for n in out if n != "root"] + (["root"] if "root" in out else [])


def target_account():
    """(compte à saisir, l'utilisateur est-il lui-même administrateur ?)."""
    me = LOCK["session_account"]()
    admins = admin_accounts()
    if me in admins:
        return me, True
    return (admins[0] if admins else None), False


def _known(user):
    """Le mot de passe de ce compte est-il lisible ici (shadow ou fichier Live) ?"""
    return LOCK["shadow_hash"](user) is not None or LOCK["users_conf_password"](user) is not None


def needs_password(user):
    if user is None:
        return False
    if _known(user):
        return LOCK["requires_password"](user)
    # Session non root : /etc/shadow est illisible, on demande quand même.
    return os.geteuid() != 0


def verify(user, password):
    if _known(user):
        return LOCK["check_password"](user, password)
    # sudo vérifie le mot de passe de l'utilisateur courant et son
    # appartenance aux administrateurs.
    if os.geteuid() != 0 and user == LOCK["session_account"]():
        try:
            run = subprocess.run(["sudo", "-S", "-k", "-p", "", "true"], input=password + "\n",
                                 text=True, capture_output=True, timeout=20)
            return run.returncode == 0
        except (OSError, subprocess.TimeoutExpired):
            return False
    return False


def run_dialog(app, win_path):
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, Gtk

    user, _self_admin = target_account()
    ask_password = needs_password(user)
    result = {"ok": False}

    css = b"""
      .uac { background: #f3f3f3; }
      .uac-band { background: #1f1f1f; color: white; padding: 10px 18px; font-size: 13px; }
      .uac-question { font-size: 20px; font-weight: 600; color: #1b1b1b; }
      .uac-app { font-size: 15px; font-weight: 600; color: #1b1b1b; }
      .uac-info { color: #5d5d5d; font-size: 13px; }
      .uac-error { color: #c42b1c; font-size: 13px; }
      button.uac-yes { background: #0067c0; color: white; border: none; border-radius: 4px;
                       padding: 6px 24px; box-shadow: none; text-shadow: none; }
      button.uac-no { background: white; color: #1b1b1b; border: 1px solid #d1d1d1;
                      border-radius: 4px; padding: 6px 24px; box-shadow: none; }
    """
    provider = Gtk.CssProvider()
    provider.load_from_data(css)
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), provider, 900)

    window = Gtk.Window(title=_t("Contrôle de compte d'utilisateur"))
    window.get_style_context().add_class("uac")
    window.set_keep_above(True)
    window.set_position(Gtk.WindowPosition.CENTER_ALWAYS)
    window.set_resizable(False)
    window.set_type_hint(Gdk.WindowTypeHint.DIALOG)
    # Taille adaptée à l'écran (DPI compris), jamais minuscule.
    monitor = Gdk.Display.get_default().get_primary_monitor() or Gdk.Display.get_default().get_monitor(0)
    width = max(440, min(560, monitor.get_geometry().width // 3))
    window.set_default_size(width, -1)

    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
    window.add(box)
    band = Gtk.Label(label=_t("Contrôle de compte d'utilisateur"), xalign=0)
    band.get_style_context().add_class("uac-band")
    box.pack_start(band, False, False, 0)

    body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=22)
    box.pack_start(body, True, True, 0)
    question = Gtk.Label(label=_t("Voulez-vous autoriser cette application à apporter des modifications à votre appareil ?"),
                         xalign=0, wrap=True, max_width_chars=40)
    question.get_style_context().add_class("uac-question")
    body.pack_start(question, False, False, 0)
    name = Gtk.Label(label=app, xalign=0, margin_top=8, wrap=True)
    name.get_style_context().add_class("uac-app")
    body.pack_start(name, False, False, 0)
    for line in (_t("Éditeur : inconnu"), _t("Origine du fichier : disque dur de cet ordinateur"), win_path):
        if line:
            label = Gtk.Label(label=line, xalign=0, wrap=True)
            label.get_style_context().add_class("uac-info")
            body.pack_start(label, False, False, 0)

    entry = Gtk.Entry(visibility=False, placeholder_text=_t("Mot de passe"), activates_default=True)
    error = Gtk.Label(xalign=0)
    error.get_style_context().add_class("uac-error")
    if user is None:
        error.set_text(_t("Aucun compte administrateur n'existe sur cet appareil."))
        body.pack_start(error, False, False, 6)
    elif ask_password:
        prompt = (_t("Pour continuer, entrez le mot de passe de {name}.", name=user) if user != "root"
                  else _t("Pour continuer, entrez le mot de passe d'un administrateur."))
        body.pack_start(Gtk.Label(label=prompt, xalign=0, wrap=True, margin_top=10), False, False, 0)
        body.pack_start(entry, False, False, 0)
        body.pack_start(error, False, False, 0)

    buttons = Gtk.Box(spacing=8, halign=Gtk.Align.FILL, homogeneous=True, margin_top=14)
    yes = Gtk.Button(label=_t("Oui"), can_default=True)
    yes.get_style_context().add_class("uac-yes")
    no = Gtk.Button(label=_t("Non"))
    no.get_style_context().add_class("uac-no")
    buttons.pack_start(yes, True, True, 0)
    buttons.pack_start(no, True, True, 0)
    body.pack_start(buttons, False, False, 0)
    yes.set_sensitive(user is not None)

    def on_yes(_button):
        if ask_password and not verify(user, entry.get_text()):
            entry.set_text("")
            error.set_text(_t("Le mot de passe est incorrect. Réessayez."))
            entry.grab_focus()
            return
        result["ok"] = True
        Gtk.main_quit()

    yes.connect("clicked", on_yes)
    no.connect("clicked", lambda _b: Gtk.main_quit())
    window.connect("destroy", lambda _w: Gtk.main_quit())
    window.connect("key-press-event",
                   lambda _w, ev: Gtk.main_quit() if ev.keyval == Gdk.KEY_Escape else False)
    window.show_all()
    window.set_default(yes)
    (entry if ask_password else no).grab_focus()
    Gtk.main()
    return result["ok"]


def main(argv):
    app, win_path = "", ""
    args = list(argv[1:])
    while args:
        flag = args.pop(0)
        if flag == "--app" and args:
            app = args.pop(0)
        elif flag == "--path" and args:
            win_path = args.pop(0)
        else:
            print(__doc__.strip(), file=sys.stderr)
            return 2
    return 0 if run_dialog(app or "?", win_path) else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

#!/usr/bin/python3
"""Contrôle de compte d'utilisateur Winlux.

Usage : winlux-uac [--elevate] --app NOM [--path CHEMIN_WINDOWS] [--publisher ÉDITEUR]

--publisher : éditeur dont le runtime a vérifié la signature Authenticode
(sinon « Éditeur : inconnu »).
--elevate : demandé par le relais root (winlux-root) avant une action
système du bureau ; le mot de passe est vérifié par le relais, qui accorde
alors 5 minutes d'élévation à ce compte.

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
    "Clavier tactile": ("Touch keyboard", "Teclado táctil", "Bildschirmtastatur"),
    "Contrôle de compte d'utilisateur": (
        "User Account Control", "Control de cuentas de usuario", "Benutzerkontensteuerung"),
    "Voulez-vous autoriser cette application à apporter des modifications à votre appareil ?": (
        "Do you want to allow this app to make changes to your device?",
        "¿Quieres permitir que esta aplicación haga cambios en el dispositivo?",
        "Möchten Sie zulassen, dass durch diese App Änderungen an Ihrem Gerät vorgenommen werden?"),
    "Éditeur : inconnu": ("Publisher: Unknown", "Editor: desconocido", "Herausgeber: Unbekannt"),
    "Éditeur vérifié : Winlux": ("Verified publisher: Winlux", "Editor comprobado: Winlux", "Verifizierter Herausgeber: Winlux"),
    "Éditeur vérifié : {name}": (
        "Verified publisher: {name}", "Editor comprobado: {name}", "Verifizierter Herausgeber: {name}"),
    "Origine du fichier : disque dur de cet ordinateur": (
        "File origin: Hard drive on this computer",
        "Origen del archivo: unidad de disco duro de este equipo",
        "Dateiursprung: Festplatte auf diesem Computer"),
    "Pour continuer, entrez le mot de passe de {name}.": (
        "To continue, enter the password for {name}.",
        "Para continuar, escribe la contraseña de {name}.",
        "Geben Sie zum Fortfahren das Kennwort für {name} ein."),
    "Pour continuer, choisissez un administrateur et entrez son mot de passe.": (
        "To continue, choose an administrator and enter their password.",
        "Para continuar, elige un administrador y escribe su contraseña.",
        "Wählen Sie zum Fortfahren einen Administrator aus, und geben Sie sein Kennwort ein."),
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


# ── Verrou des fenêtres de consentement (#1696) ─────────────────────────────
# Partagé avec la saisie simulée du runtime (runtime/src/dlls/saisie.rs) :
# la fenêtre le prend en exclusif avant de s'afficher et le garde jusqu'à sa
# fermeture. Un envoi SendInput en cours se termine d'abord ; aucun autre ne
# commence tant que la fenêtre est là. Seule la personne peut y répondre.

def consent_lock_path():
    if os.environ.get("WINLUX_CONSENT_LOCK"):
        return os.environ["WINLUX_CONSENT_LOCK"]
    display = os.environ.get("DISPLAY", "").rpartition(":")[2].split(".")[0]
    number = "".join(c for c in display if c.isdigit()) or "0"
    return f"/tmp/winlux-consent-{number}.lock"


def _lock_exclusive(path, timeout):
    """Descripteur verrouillé en exclusif, ou None (lien, autre compte,
    délai dépassé)."""
    import fcntl
    import stat
    import time
    # Jamais à travers un lien symbolique posé dans /tmp par un autre compte ;
    # la saisie simulée refuse elle-même tout envoi sans ces verrous.
    try:
        fd = os.open(path, os.O_RDONLY | os.O_CREAT | os.O_NOFOLLOW, 0o644)
    except OSError:
        return None
    info = os.fstat(fd)
    if not stat.S_ISREG(info.st_mode) or info.st_uid not in (os.getuid(), 0):
        os.close(fd)
        return None
    end = time.monotonic() + timeout
    while True:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return fd
        except OSError:
            if time.monotonic() >= end:
                os.close(fd)
                return None
            time.sleep(0.005)


def hold_consent_lock(timeout=10.0):
    """Verrous à garder ouverts jusqu'à la fermeture de la fenêtre (#1711).

    D'abord la porte : dès qu'elle est prise, aucune nouvelle saisie simulée
    ne démarre (elle n'y passe qu'un instant). Ensuite le verrou principal :
    on attend la fin des envois déjà commencés, un lot au plus. Si un verrou
    est impossible (fichier posé par un autre compte), la fenêtre s'affiche
    quand même : la saisie simulée refuse alors déjà tout."""
    path = consent_lock_path()
    gate = _lock_exclusive(os.path.splitext(path)[0] + ".porte", timeout)
    main = _lock_exclusive(path, timeout)
    return [fd for fd in (gate, main) if fd is not None]


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


def admin_choices():
    """Administrateurs qui peuvent autoriser l'élévation de la session (#2198).

    Un administrateur s'autorise lui-même. Pour un compte standard, n'importe
    quel administrateur de l'appareil convient, comme sous Windows ; root
    n'est proposé que s'il n'y a aucun compte administrateur personnel."""
    me = LOCK["session_account"]()
    admins = admin_accounts()
    if me in admins:
        return [me]
    personal = [a for a in admins if a != "root"]
    return personal or admins


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
    # Autre compte (un compte standard tape le mot de passe d'un
    # administrateur) dont le mot de passe est dans /etc/shadow, illisible
    # ici : le relais root vérifie que c'est un administrateur et que le mot
    # de passe est le sien (#2292). Un compte installé figure aussi dans
    # users.conf avec « shadow » : il ne passe pas pour autant par la
    # vérification locale, que le relais refuse pour un autre compte (#2415).
    if os.geteuid() != 0 and user != LOCK["session_account"]() and LOCK["shadow_hash"](user) is None \
            and LOCK["users_conf_password"](user) in (None, "shadow"):
        answer = LOCK["relay_check"](user, password, "admin-check")
        return bool(answer)
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


def run_dialog(app, win_path, elevate=False, publisher=""):
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, Gtk

    user, self_admin = target_account()
    # Compte standard : la personne choisit l'administrateur qui autorise
    # (#2198) ; le mot de passe vérifié est celui du compte choisi.
    choices = [] if self_admin else admin_choices()
    if choices:
        user = choices[0]
    # --elevate : le relais root (winlux-root) ouvre l'élévation après avoir
    # vérifié lui-même le mot de passe ; il est donc toujours demandé.
    ask_password = elevate or needs_password(user)
    result = {"ok": False, "user": user}

    css = b"""
      .uac { background: #f3f3f3; color: #1b1b1b; }
      /* The user gtk.css (dark theme) must not repaint the inner boxes. */
      .uac box, .uac label { background-color: transparent; }
      .uac .uac-band { background: #1f1f1f; color: white; padding: 10px 18px; font-size: 13px; }
      .uac .uac-question { font-size: 20px; font-weight: 600; color: #1b1b1b; }
      .uac .uac-app { font-size: 15px; font-weight: 600; color: #1b1b1b; }
      .uac .uac-info { color: #5d5d5d; font-size: 13px; }
      .uac .uac-error { color: #c42b1c; font-size: 13px; }
      button.uac-yes { background: #6a3df0; color: white; border: none; border-radius: 4px;
                       padding: 6px 24px; box-shadow: none; text-shadow: none; }
      button.uac-no { background: white; color: #1b1b1b; border: 1px solid #d1d1d1;
                      border-radius: 4px; padding: 6px 24px; box-shadow: none; }
    """
    provider = Gtk.CssProvider()
    provider.load_from_data(css)
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), provider, 900)

    window = Gtk.Window(title=_t("Contrôle de compte d'utilisateur"))
    window.set_icon_name("security-high")
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
    # --elevate : un outil de Winlux lui-même (Store, Paramètres) passe par le
    # relais root ; comme Windows pour ses composants, l'éditeur est connu.
    # Sinon, l'éditeur est celui dont le runtime a vérifié la signature
    # Authenticode (#1382) ; un binaire non signé ou mal signé reste inconnu.
    if elevate:
        shown = _t("Éditeur vérifié : Winlux")
    elif publisher:
        shown = _t("Éditeur vérifié : {name}", name=publisher)
    else:
        shown = _t("Éditeur : inconnu")
    for line in (shown, _t("Origine du fichier : disque dur de cet ordinateur"), win_path):
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
        if len(choices) > 1:
            prompt = _t("Pour continuer, choisissez un administrateur et entrez son mot de passe.")
        elif user != "root":
            prompt = _t("Pour continuer, entrez le mot de passe de {name}.", name=user)
        else:
            prompt = _t("Pour continuer, entrez le mot de passe d'un administrateur.")
        body.pack_start(Gtk.Label(label=prompt, xalign=0, wrap=True, margin_top=10), False, False, 0)
        if len(choices) > 1:
            picker = Gtk.ComboBoxText()
            for account in choices:
                picker.append(account, account)
            picker.set_active_id(user)

            def on_pick(combo):
                result["user"] = combo.get_active_id() or choices[0]
                entry.set_text("")
                error.set_text("")
            picker.connect("changed", on_pick)
            body.pack_start(picker, False, False, 0)
        osk = osk_command()
        if osk:
            # Tablette sans clavier : bouton du clavier tactile dans le champ,
            # et clavier ouvert d'office s'il n'y a aucun clavier physique.
            entry.set_icon_from_icon_name(Gtk.EntryIconPosition.SECONDARY, "input-keyboard-symbolic")
            entry.set_icon_tooltip_text(Gtk.EntryIconPosition.SECONDARY, _t("Clavier tactile"))
            entry.connect("icon-press", lambda *_: osk_run(osk, "show"))
            if not physical_keyboard(osk):
                osk_run(osk, "show")
                result["osk"] = osk
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
        chosen = result["user"]
        checked = (LOCK["relay_check"](chosen, entry.get_text(), "elevate") if elevate
                   else verify(chosen, entry.get_text()) if ask_password else True)
        if not checked:
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
    result["lock"] = hold_consent_lock()
    window.show_all()
    window.set_default(yes)
    (entry if ask_password else no).grab_focus()
    Gtk.main()
    for fd in result.get("lock") or []:
        os.close(fd)
    if result.get("osk"):
        # Le clavier ouvert pour ce mot de passe se referme avec la fenêtre.
        osk_run(result["osk"], "hide")
    return result["ok"]


def osk_command():
    """Clavier tactile (winlux-osk), installé ou à côté de ce script."""
    if os.path.exists("/usr/bin/winlux-osk"):
        return ["/usr/bin/winlux-osk"]
    script = os.path.join(os.path.dirname(os.path.abspath(__file__)), "winlux-osk.py")
    return [sys.executable, script] if os.path.exists(script) else None


def physical_keyboard(command):
    try:
        out = subprocess.run(command + ["detect"], capture_output=True, text=True, timeout=5).stdout
    except (OSError, subprocess.SubprocessError):
        return True
    return not out.startswith("keyboard=0")


def osk_run(command, verb):
    try:
        subprocess.Popen(command + [verb], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    except OSError:
        pass
    return False


def main(argv):
    app, win_path, elevate, publisher = "", "", False, ""
    args = list(argv[1:])
    while args:
        flag = args.pop(0)
        if flag == "--elevate":
            elevate = True
        elif flag == "--app" and args:
            app = args.pop(0)
        elif flag == "--path" and args:
            win_path = args.pop(0)
        elif flag == "--publisher" and args:
            publisher = args.pop(0)
        else:
            print(__doc__.strip(), file=sys.stderr)
            return 2
    return 0 if run_dialog(app or "?", win_path, elevate, publisher) else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

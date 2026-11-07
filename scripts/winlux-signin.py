#!/usr/bin/python3
"""Options de connexion Winlux : code PIN et empreinte digitale (#659).

À l'écran de connexion et de verrouillage (winlux-lock), un compte local peut
taper un code PIN court ou poser le doigt sur le lecteur au lieu de son mot
de passe, comme sous Windows 11. Le mot de passe reste toujours possible, et
le Contrôle de compte d'utilisateur le demande toujours.

Côté root (lancé par le relais winlux-root, jamais par le bureau) :
  winlux-signin status COMPTE        JSON : pin, pin_locked, reader, fingers
  winlux-signin pin-set COMPTE       nouveau code sur l'entrée standard
  winlux-signin pin-check COMPTE     code sur l'entrée standard
  winlux-signin pin-remove COMPTE
  winlux-signin pin-unblock COMPTE   après un mot de passe juste
  winlux-signin fp-verify COMPTE     attend un doigt (une ligne JSON par événement)
  winlux-signin fp-enroll COMPTE     enregistre un doigt de plus
  winlux-signin fp-delete COMPTE     efface les empreintes du compte
  winlux-signin forget COMPTE        compte supprimé : code et empreintes effacés
  winlux-signin simulate match|nomatch|off|on   lecteur simulé (VM, tests)

Code PIN : 4 à 32 chiffres, ni « 1111 » ni « 1234 ». Il est haché avec
scrypt (sel aléatoire) dans /var/lib/winlux/signin/COMPTE.pin, lisible par
root seul : le bureau ne le lit jamais, le relais compare. Après 5 codes
faux, le code est bloqué jusqu'au prochain mot de passe juste ; c'est ce
blocage, pas la longueur du code, qui le protège (comme la puce TPM de
Windows), et le code ne quitte jamais ce PC.

Empreinte : fprintd (installé par Paramètres > Matériel quand un lecteur est
détecté), sur le bus D-Bus système. fprintd tourne en root et ouvre lui-même
le lecteur : aucun droit sur le périphérique n'est donné aux comptes (pas de
logind, donc pas d'ACL uaccess, comme /dev/rfkill en #604). La règle polkit
49-winlux-fprint.rules ne laisse parler à fprintd que root, donc ce relais.
Sans lecteur ou sans fprintd, rien ne s'affiche.

Lecteur simulé : WINLUX_FPRINT_FAKE=DOSSIER (tests), ou
FINGERPRINT_SIMULATOR=1 dans /etc/winlux/signin.conf (VM sans lecteur, que
seul root peut écrire) ; « winlux-signin simulate match » pose alors un bon
doigt, « nomatch » un mauvais.

Côté bureau, winlux-lock et les Paramètres chargent ce fichier comme module
et appellent le relais (fonctions client : status, pin_check, …).
"""
import base64
import hashlib
import hmac
import json
import os
import re
import secrets
import signal
import socket
import sys
import time

ROOT = os.environ.get("WINLUX_ROOT_PREFIX", "")
STORE = ROOT + "/var/lib/winlux/signin"
CONF = ROOT + "/etc/winlux/signin.conf"
NAME = re.compile(r"^[a-z_][a-z0-9_.-]{0,31}$")
PIN = re.compile(r"^[0-9]{4,32}$")
MAX_PIN_TRIES = 5
# scrypt : environ 16 Mo et un dixième de seconde par essai.
SCRYPT = {"n": 2 ** 14, "r": 8, "p": 1}
VERIFY_SECONDS = 60
ENROLL_SECONDS = 180
FINGERS = ("right-index-finger", "left-index-finger", "right-middle-finger", "left-middle-finger",
           "right-thumb", "left-thumb", "right-ring-finger", "left-ring-finger",
           "right-little-finger", "left-little-finger")
FPRINT = "net.reactivated.Fprint"


# ── Langue (messages montrés par l'écran de connexion et les Paramètres) ────

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


_COL = {"en": 0, "es": 1, "de": 2}
# Texte français -> (anglais, espagnol, allemand).
_TR = {
    "Code PIN": ("PIN", "PIN", "PIN"),
    "Le code PIN est incorrect. Réessayez.": (
        "The PIN is incorrect. Try again.", "El PIN no es correcto. Inténtelo de nuevo.",
        "Die PIN ist falsch. Versuchen Sie es erneut."),
    "Le code PIN est bloqué après trop d’essais. Connectez-vous avec votre mot de passe.": (
        "The PIN is blocked after too many attempts. Sign in with your password.",
        "El PIN está bloqueado tras demasiados intentos. Inicie sesión con su contraseña.",
        "Die PIN ist nach zu vielen Versuchen gesperrt. Melden Sie sich mit Ihrem Kennwort an."),
    "Le code PIN doit compter de 4 à 32 chiffres.": (
        "The PIN must be 4 to 32 digits long.", "El PIN debe tener entre 4 y 32 dígitos.",
        "Die PIN muss 4 bis 32 Ziffern lang sein."),
    "Ce code PIN est trop simple (chiffres répétés ou qui se suivent).": (
        "This PIN is too simple (repeated or consecutive digits).",
        "Este PIN es demasiado sencillo (dígitos repetidos o consecutivos).",
        "Diese PIN ist zu einfach (wiederholte oder aufeinanderfolgende Ziffern)."),
    "Les deux codes PIN sont différents.": (
        "The two PINs don’t match.", "Los dos PIN no coinciden.", "Die beiden PINs stimmen nicht überein."),
    "Le mot de passe est incorrect.": (
        "The password is incorrect.", "La contraseña no es correcta.", "Das Kennwort ist falsch."),
    "Ajoutez d’abord un mot de passe à votre compte.": (
        "Add a password to your account first.", "Primero añada una contraseña a su cuenta.",
        "Fügen Sie Ihrem Konto zuerst ein Kennwort hinzu."),
    "Aucun lecteur d’empreintes n’est disponible.": (
        "No fingerprint reader is available.", "No hay ningún lector de huellas disponible.",
        "Kein Fingerabdruckleser verfügbar."),
    "Le lecteur d’empreintes est occupé. Réessayez.": (
        "The fingerprint reader is busy. Try again.", "El lector de huellas está ocupado. Inténtelo de nuevo.",
        "Der Fingerabdruckleser ist belegt. Versuchen Sie es erneut."),
    "Le lecteur a été débranché.": (
        "The reader was disconnected.", "El lector se ha desconectado.", "Der Leser wurde getrennt."),
    "Temps écoulé. Réessayez.": ("Time ran out. Try again.", "Se agotó el tiempo. Inténtelo de nuevo.",
                                 "Die Zeit ist abgelaufen. Versuchen Sie es erneut."),
    "Ce doigt est déjà enregistré.": ("This finger is already registered.", "Este dedo ya está registrado.",
                                      "Dieser Finger ist bereits registriert."),
    "Le lecteur est plein : supprimez des empreintes d’abord.": (
        "The reader is full: remove fingerprints first.", "El lector está lleno: elimine huellas primero.",
        "Der Leser ist voll: Entfernen Sie zuerst Fingerabdrücke."),
    "Tous les doigts sont déjà enregistrés.": ("All fingers are already registered.",
                                               "Todos los dedos ya están registrados.",
                                               "Alle Finger sind bereits registriert."),
    "L’opération a échoué.": ("The operation failed.", "La operación ha fallado.", "Der Vorgang ist fehlgeschlagen."),
    "Le service système ne répond pas.": ("The system service isn’t responding.",
                                         "El servicio del sistema no responde.",
                                         "Der Systemdienst antwortet nicht."),
    "Empreinte non reconnue. Réessayez.": ("Fingerprint not recognised. Try again.",
                                          "Huella no reconocida. Inténtelo de nuevo.",
                                          "Fingerabdruck nicht erkannt. Versuchen Sie es erneut."),
    "Reposez votre doigt sur le lecteur.": ("Place your finger on the reader again.",
                                           "Vuelva a poner el dedo en el lector.",
                                           "Legen Sie Ihren Finger erneut auf den Leser."),
}

MESSAGES = {
    "pin-wrong": "Le code PIN est incorrect. Réessayez.",
    "pin-locked": "Le code PIN est bloqué après trop d’essais. Connectez-vous avec votre mot de passe.",
    "pin-format": "Le code PIN doit compter de 4 à 32 chiffres.",
    "pin-simple": "Ce code PIN est trop simple (chiffres répétés ou qui se suivent).",
    "pin-mismatch": "Les deux codes PIN sont différents.",
    "password": "Le mot de passe est incorrect.",
    "no-password": "Ajoutez d’abord un mot de passe à votre compte.",
    "no-reader": "Aucun lecteur d’empreintes n’est disponible.",
    "busy": "Le lecteur d’empreintes est occupé. Réessayez.",
    "disconnected": "Le lecteur a été débranché.",
    "timeout": "Temps écoulé. Réessayez.",
    "duplicate": "Ce doigt est déjà enregistré.",
    "data-full": "Le lecteur est plein : supprimez des empreintes d’abord.",
    "all-fingers": "Tous les doigts sont déjà enregistrés.",
    "no-match": "Empreinte non reconnue. Réessayez.",
    "retry": "Reposez votre doigt sur le lecteur.",
    "relay": "Le service système ne répond pas.",
}


def _t(text):
    lang = _interface_lang()
    row = _TR.get(text)
    return row[_COL[lang]] if row and lang in _COL else text


def message(code):
    """Message traduit d'un code d'erreur ou d'événement."""
    return _t(MESSAGES.get(code or "", "L’opération a échoué."))


# ── Code PIN (root) ─────────────────────────────────────────────────────────

def pin_problem(pin):
    """Code d'erreur si ce code PIN est refusé, sinon None."""
    if not isinstance(pin, str) or not PIN.match(pin):
        return "pin-format"
    digits = [int(c) for c in pin]
    steps = {b - a for a, b in zip(digits, digits[1:])}
    if steps in ({0}, {1}, {-1}):
        return "pin-simple"
    return None


def _store_path(user, suffix):
    if not NAME.match(user or ""):
        raise ValueError("compte invalide")
    return os.path.join(STORE, user + suffix)


def _read_pin(user):
    try:
        with open(_store_path(user, ".pin"), encoding="utf-8") as handle:
            data = json.load(handle)
        return data if isinstance(data, dict) and data.get("hash") else None
    except (OSError, ValueError):
        return None


def _write_pin(user, data):
    os.makedirs(STORE, mode=0o700, exist_ok=True)
    os.chmod(STORE, 0o700)
    path = _store_path(user, ".pin")
    tmp = f"{path}.{os.getpid()}"
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as out:
        json.dump(data, out)
    os.rename(tmp, path)


def _hash(pin, salt, n, r, p):
    return hashlib.scrypt(pin.encode(), salt=salt, n=n, r=r, p=p, maxmem=64 * 1024 * 1024, dklen=32)


def pin_set(user, pin):
    problem = pin_problem(pin)
    if problem:
        return {"ok": False, "error": problem}
    salt = secrets.token_bytes(16)
    _write_pin(user, {"v": 1, **SCRYPT, "salt": base64.b64encode(salt).decode(),
                      "hash": base64.b64encode(_hash(pin, salt, **SCRYPT)).decode(), "tries": 0})
    return {"ok": True}


def pin_check(user, pin):
    data = _read_pin(user)
    if data is None:
        return {"ok": False, "error": "pin-wrong"}
    tries = int(data.get("tries", 0))
    if tries >= MAX_PIN_TRIES:
        return {"ok": False, "error": "pin-locked", "locked": True}
    try:
        salt = base64.b64decode(data["salt"])
        good = hmac.compare_digest(_hash(pin if isinstance(pin, str) else "", salt,
                                         int(data["n"]), int(data["r"]), int(data["p"])),
                                   base64.b64decode(data["hash"]))
    except (KeyError, ValueError, TypeError):
        good = False
    data["tries"] = 0 if good else tries + 1
    _write_pin(user, data)
    if good:
        return {"ok": True}
    locked = data["tries"] >= MAX_PIN_TRIES
    return {"ok": False, "error": "pin-locked" if locked else "pin-wrong", "locked": locked,
            "left": max(0, MAX_PIN_TRIES - data["tries"])}


def pin_unblock(user):
    data = _read_pin(user)
    if data is not None and data.get("tries"):
        data["tries"] = 0
        _write_pin(user, data)
    return {"ok": True}


def pin_remove(user):
    try:
        os.unlink(_store_path(user, ".pin"))
    except FileNotFoundError:
        pass
    return {"ok": True}


# ── Empreinte : lecteur simulé (tests, VM) ──────────────────────────────────

def _conf():
    out = {}
    try:
        with open(CONF, encoding="utf-8") as conf:
            for line in conf:
                key, _, value = line.strip().partition("=")
                out[key] = value
    except OSError:
        pass
    return out


def simulator_dir():
    fake = os.environ.get("WINLUX_FPRINT_FAKE")
    if fake:
        return fake
    if _conf().get("FINGERPRINT_SIMULATOR") == "1":
        return os.path.join(STORE, "simulator")
    return None


class FakeReader:
    """Lecteur simulé : un doigt « posé » est un fichier touch (match/nomatch).
    Un fichier « auto » répond tout de suite par son contenu (tests)."""

    def __init__(self, folder):
        self.folder = folder
        self.stages = 5

    def present(self):
        return os.path.isdir(self.folder)

    def _fingers_file(self, user):
        return os.path.join(self.folder, user + ".fingers")

    def fingers(self, user):
        try:
            with open(self._fingers_file(user), encoding="utf-8") as handle:
                return [line.strip() for line in handle if line.strip()]
        except OSError:
            return []

    def _touch(self, deadline, stop):
        auto = os.path.join(self.folder, "auto")
        touch = os.path.join(self.folder, "touch")
        while time.monotonic() < deadline and not stop():
            for path in (touch, auto):
                try:
                    with open(path, encoding="utf-8") as handle:
                        value = handle.read().strip() or "match"
                except OSError:
                    continue
                if path == touch:
                    try:
                        os.unlink(touch)
                    except OSError:
                        pass
                return value
            time.sleep(0.1)
        return None

    def verify(self, user, emit, stop, seconds):
        if not self.fingers(user):
            return {"ok": False, "error": "no-match"}
        value = self._touch(time.monotonic() + seconds, stop)
        if value is None:
            return {"ok": False, "error": "timeout"}
        return {"ok": True} if value == "match" else {"ok": False, "error": "no-match"}

    def enroll(self, user, finger, emit, stop, seconds):
        deadline = time.monotonic() + seconds
        for stage in range(1, self.stages + 1):
            if self._touch(deadline, stop) is None:
                return {"ok": False, "error": "timeout"}
            emit({"event": "stage", "done": stage, "total": self.stages})
        with open(self._fingers_file(user), "a", encoding="utf-8") as handle:
            handle.write(finger + "\n")
        return {"ok": True, "finger": finger}

    def delete(self, user):
        try:
            os.unlink(self._fingers_file(user))
        except FileNotFoundError:
            pass
        return {"ok": True}


# ── Empreinte : fprintd sur le bus système ──────────────────────────────────

class FprintdReader:
    def __init__(self):
        self.bus = None
        self.device = None
        self.stages = 5
        try:
            import gi  # noqa: F401
            from gi.repository import Gio, GLib
            self.Gio, self.GLib = Gio, GLib
            self.bus = Gio.bus_get_sync(Gio.BusType.SYSTEM, None)
            answer = self.bus.call_sync(FPRINT, "/net/reactivated/Fprint/Manager", FPRINT + ".Manager",
                                        "GetDefaultDevice", None, GLib.VariantType.new("(o)"),
                                        Gio.DBusCallFlags.NONE, 15000, None)
            self.device = answer.unpack()[0]
            stages = self._property("num-enroll-stages")
            if isinstance(stages, int) and stages > 0:
                self.stages = stages
        except Exception:
            # fprintd absent (Paramètres > Matériel ne l'a pas installé),
            # bus système absent, ou aucun lecteur : rien à proposer.
            self.device = None

    def present(self):
        return self.device is not None

    def _call(self, method, args=None, reply=None, timeout=15000):
        GLib = self.GLib
        return self.bus.call_sync(FPRINT, self.device, FPRINT + ".Device", method, args,
                                  GLib.VariantType.new(reply) if reply else None,
                                  self.Gio.DBusCallFlags.NONE, timeout, None)

    def _property(self, name):
        GLib = self.GLib
        answer = self.bus.call_sync(FPRINT, self.device, "org.freedesktop.DBus.Properties", "Get",
                                    GLib.Variant("(ss)", (FPRINT + ".Device", name)),
                                    GLib.VariantType.new("(v)"), self.Gio.DBusCallFlags.NONE, 5000, None)
        return answer.unpack()[0]

    def fingers(self, user):
        try:
            return list(self._call("ListEnrolledFingers", self.GLib.Variant("(s)", (user,)), "(as)").unpack()[0])
        except Exception:
            return []

    @staticmethod
    def _error(error):
        text = str(error)
        if "AlreadyInUse" in text or "ClaimDevice" in text:
            return "busy"
        if "NoSuchDevice" in text or "ServiceUnknown" in text:
            return "no-reader"
        return "failed"

    def _session(self, user, start, stop_method, signal_name, on_status, stop, seconds):
        """Réserve le lecteur, lance l'opération et attend son signal final."""
        GLib = self.GLib
        loop = GLib.MainLoop()
        result = {}

        def finish(answer):
            if not result:
                result.update(answer)
            loop.quit()

        def status(_conn, _sender, _path, _iface, _signal, params):
            answer = on_status(*params.unpack())
            if answer is not None:
                finish(answer)

        def poll():
            if stop():
                finish({"ok": False, "error": "cancelled"})
                return False
            return True
        try:
            self._call("Claim", GLib.Variant("(s)", (user,)))
        except Exception as error:
            return {"ok": False, "error": self._error(error)}
        watch = self.bus.signal_subscribe(FPRINT, FPRINT + ".Device", signal_name, self.device, None,
                                          self.Gio.DBusSignalFlags.NONE, status)
        started = False
        try:
            start()
            started = True
            GLib.timeout_add_seconds(seconds, lambda: finish({"ok": False, "error": "timeout"}) and False)
            GLib.timeout_add(200, poll)
            for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
                GLib.unix_signal_add(GLib.PRIORITY_HIGH, sig, lambda: finish({"ok": False, "error": "cancelled"}) and False)
            loop.run()
        except Exception as error:
            result.setdefault("ok", False)
            result.setdefault("error", self._error(error))
        finally:
            self.bus.signal_unsubscribe(watch)
            if started:
                try:
                    self._call(stop_method)
                except Exception:
                    pass
            try:
                self._call("Release")
            except Exception:
                pass
        return result

    def verify(self, user, emit, stop, seconds):
        if not self.fingers(user):
            return {"ok": False, "error": "no-match"}

        def on_status(outcome, done):
            if outcome == "verify-match":
                return {"ok": True}
            if outcome == "verify-no-match":
                return {"ok": False, "error": "no-match"}
            if outcome == "verify-disconnected":
                return {"ok": False, "error": "disconnected"}
            if done:
                return {"ok": False, "error": "failed"}
            emit({"event": "retry"})
            return None
        return self._session(user, lambda: self._call("VerifyStart", self.GLib.Variant("(s)", ("any",))),
                             "VerifyStop", "VerifyStatus", on_status, stop, seconds)

    def enroll(self, user, finger, emit, stop, seconds):
        count = [0]

        def on_status(outcome, done):
            if outcome == "enroll-completed":
                return {"ok": True, "finger": finger}
            if outcome == "enroll-stage-passed":
                count[0] += 1
                emit({"event": "stage", "done": min(count[0], self.stages), "total": self.stages})
                return None
            errors = {"enroll-duplicate": "duplicate", "enroll-data-full": "data-full",
                      "enroll-disconnected": "disconnected"}
            if outcome in errors:
                return {"ok": False, "error": errors[outcome]}
            if done:
                return {"ok": False, "error": "failed"}
            emit({"event": "retry"})
            return None
        return self._session(user, lambda: self._call("EnrollStart", self.GLib.Variant("(s)", (finger,))),
                             "EnrollStop", "EnrollStatus", on_status, stop, seconds)

    def delete(self, user):
        GLib = self.GLib
        try:
            self._call("Claim", GLib.Variant("(s)", (user,)))
        except Exception as error:
            return {"ok": False, "error": self._error(error)}
        try:
            try:
                self._call("DeleteEnrolledFingers2")
            except Exception:
                self._call("DeleteEnrolledFingers", GLib.Variant("(s)", (user,)))
            return {"ok": True}
        except Exception as error:
            return {"ok": False, "error": self._error(error)}
        finally:
            try:
                self._call("Release")
            except Exception:
                pass


def reader():
    folder = simulator_dir()
    return FakeReader(folder) if folder else FprintdReader()


# ── Commandes root ──────────────────────────────────────────────────────────

def emit(event):
    sys.stdout.write(json.dumps(event) + "\n")
    sys.stdout.flush()


STOPPED = []


def stop_requested():
    return bool(STOPPED)


def status(user):
    device = reader()
    present = device.present()
    data = _read_pin(user)
    return {"ok": True, "pin": data is not None,
            "pin_locked": bool(data and int(data.get("tries", 0)) >= MAX_PIN_TRIES),
            "reader": present, "fingers": len(device.fingers(user)) if present else 0}


def fp_verify(user):
    device = reader()
    if not device.present():
        return {"ok": False, "error": "no-reader"}
    return device.verify(user, emit, stop_requested, VERIFY_SECONDS)


def fp_enroll(user):
    device = reader()
    if not device.present():
        return {"ok": False, "error": "no-reader"}
    taken = set(device.fingers(user))
    free = [finger for finger in FINGERS if finger not in taken]
    if not free:
        return {"ok": False, "error": "all-fingers"}
    emit({"event": "start", "total": device.stages})
    return device.enroll(user, free[0], emit, stop_requested, ENROLL_SECONDS)


def fp_delete(user):
    device = reader()
    if not device.present():
        return {"ok": True}
    return device.delete(user)


def simulate(word):
    folder = os.environ.get("WINLUX_FPRINT_FAKE") or os.path.join(STORE, "simulator")
    if word in ("on", "off"):
        lines = []
        try:
            with open(CONF, encoding="utf-8") as conf:
                lines = [line.rstrip("\n") for line in conf if not line.startswith("FINGERPRINT_SIMULATOR=")]
        except OSError:
            pass
        if word == "on":
            lines.append("FINGERPRINT_SIMULATOR=1")
            os.makedirs(folder, mode=0o700, exist_ok=True)
        os.makedirs(os.path.dirname(CONF), exist_ok=True)
        with open(CONF, "w", encoding="utf-8") as conf:
            conf.write("".join(line + "\n" for line in lines))
        return {"ok": True}
    if word not in ("match", "nomatch"):
        return {"ok": False, "error": "failed"}
    if not os.path.isdir(folder):
        return {"ok": False, "error": "no-reader"}
    with open(os.path.join(folder, "touch"), "w", encoding="utf-8") as touch:
        touch.write(word + "\n")
    return {"ok": True}


def run_root(command, user):
    if command == "simulate":
        return simulate(user)
    if not NAME.match(user or ""):
        return {"ok": False, "error": "failed"}
    if command == "status":
        return status(user)
    if command == "pin-set":
        return pin_set(user, sys.stdin.readline().rstrip("\n"))
    if command == "pin-check":
        return pin_check(user, sys.stdin.readline().rstrip("\n"))
    if command == "pin-remove":
        return pin_remove(user)
    if command == "pin-unblock":
        return pin_unblock(user)
    if command == "fp-verify":
        return fp_verify(user)
    if command == "fp-enroll":
        return fp_enroll(user)
    if command == "fp-delete":
        return fp_delete(user)
    if command == "forget":
        pin_remove(user)
        return fp_delete(user)
    return None


# ── Client (écran de connexion, Paramètres) ─────────────────────────────────

def relay_socket():
    return os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")


class Request:
    """Une demande au relais root ; cancel() (autre fil) l'interrompt, et le
    relais relâche alors le lecteur d'empreintes."""

    def __init__(self, payload, socket_path=None):
        self.payload = payload
        self.socket_path = socket_path or relay_socket()
        self.client = None
        self.cancelled = False

    def run(self, on_event=None, timeout=10):
        payload = {"op": "signin", **self.payload}
        try:
            self.client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self.client.settimeout(timeout)
            self.client.connect(self.socket_path)
            self.client.sendall((json.dumps(payload) + "\n").encode())
            reader_file = self.client.makefile("r", encoding="utf-8")
            for line in reader_file:
                try:
                    answer = json.loads(line)
                except ValueError:
                    continue
                if not isinstance(answer, dict):
                    continue
                if "ok" in answer or "error" in answer or "auth" in answer:
                    answer.setdefault("ok", False)
                    return answer
                if on_event is not None:
                    on_event(answer)
        except (OSError, ValueError):
            pass
        finally:
            self.close()
        return {"ok": False, "error": "cancelled" if self.cancelled else "relay"}

    def close(self):
        client, self.client = self.client, None
        if client is not None:
            try:
                client.close()
            except OSError:
                pass

    def cancel(self):
        self.cancelled = True
        client = self.client
        if client is not None:
            try:
                client.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass


def ask(action, socket_path=None, timeout=10, on_event=None, **fields):
    return Request({"action": action, **fields}, socket_path).run(on_event, timeout)


def client_status(user, socket_path=None):
    """État des options de connexion d'un compte ; None sans relais."""
    answer = ask("status", socket_path, timeout=20, user=user)
    return answer if answer.get("ok") else None


def main(argv):
    if len(argv) == 3 and os.geteuid() == 0 or (len(argv) == 3 and ROOT):
        for sig in (signal.SIGTERM, signal.SIGHUP):
            signal.signal(sig, lambda *_: STOPPED.append(True))
        answer = run_root(argv[1], argv[2])
        if answer is not None:
            emit(answer)
            return 0 if answer.get("ok") else 1
    elif len(argv) == 3:
        print("winlux-signin : à lancer par le relais root (winlux-root)", file=sys.stderr)
        return 1
    print(__doc__.strip(), file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

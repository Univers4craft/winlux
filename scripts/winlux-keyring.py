#!/usr/bin/python3
"""Trousseau de secrets du compte ouvert à la connexion (#1488).

Les logiciels Linux (Claude, navigateurs, clients mail…) rangent leurs
jetons par libsecret, dans la collection « default » du service
org.freedesktop.secrets (gnome-keyring). Ailleurs, pam_gnome_keyring crée et
ouvre le trousseau « login » avec le mot de passe de session ; la session
Winlux n'a ni PAM de connexion ni logind : sans ce relais, gcr-prompter
demandait de créer « Default Keyring » en boucle et capturait la souris.

    winlux-keyring unlock    le mot de passe du compte est lu sur l'entrée
                             standard (jamais en argument : visible dans
                             /proc) ; vide pour un compte sans mot de passe.

Le trousseau « login » est créé s'il manque, avec ce mot de passe (vide :
trousseau non chiffré, comme un compte sans mot de passe sous GNOME), puis
déverrouillé et désigné par défaut. Tout passe par l'interface de
gnome-keyring prévue pour la connexion, sans fenêtre. Sans gnome-keyring
installé, rien n'est fait. Code de sortie : 0 ouvert ou rien à faire, 1 échec
(mot de passe différent de celui du trousseau, par exemple).
"""
import os
import sys

SERVICE = "org.freedesktop.secrets"
ROOT = "/org/freedesktop/secrets"
LOGIN = ROOT + "/collection/login"
SECRET = "org.freedesktop.Secret.Service"
INTERNAL = "org.gnome.keyring.InternalUnsupportedGuiltRiddenInterface"
SERVICE_FILES = [os.path.join(d, "dbus-1/services/org.freedesktop.secrets.service")
                 for d in os.environ.get("XDG_DATA_DIRS", "/usr/local/share:/usr/share").split(":") if d]


def available():
    return any(os.path.exists(path) for path in SERVICE_FILES)


def unlock(password, bus=None):
    from gi.repository import Gio, GLib

    bus = bus or Gio.bus_get_sync(Gio.BusType.SESSION, None)

    def call(path, iface, method, args, reply):
        return bus.call_sync(SERVICE, path, iface, method, args, GLib.VariantType(reply),
                             Gio.DBusCallFlags.NONE, 30000, None).unpack()

    # Session « plain » : le secret ne quitte pas le bus de session du compte.
    _, session = call(ROOT, SECRET, "OpenSession", GLib.Variant("(sv)", ("plain", GLib.Variant("s", ""))), "(vo)")
    secret = (session, b"", password.encode(), "text/plain")
    try:
        collections = call(ROOT, "org.freedesktop.DBus.Properties", "Get",
                           GLib.Variant("(ss)", (SECRET, "Collections")), "(v)")[0]
        if LOGIN in collections:
            call(ROOT, INTERNAL, "UnlockWithMasterPassword",
                 GLib.Variant("(o(oayays))", (LOGIN, secret)), "()")
            collection = LOGIN
        else:
            label = {"org.freedesktop.Secret.Collection.Label": GLib.Variant("s", "login")}
            collection = call(ROOT, INTERNAL, "CreateWithMasterPassword",
                              GLib.Variant("(a{sv}(oayays))", (label, secret)), "(o)")[0]
        default = call(ROOT, SECRET, "ReadAlias", GLib.Variant("(s)", ("default",)), "(o)")[0]
        if default == "/":
            call(ROOT, SECRET, "SetAlias", GLib.Variant("(so)", ("default", collection)), "()")
        return collection
    finally:
        try:
            call(session, "org.freedesktop.Secret.Session", "Close", None, "()")
        except GLib.Error:
            pass


def main(argv):
    if argv[1:] != ["unlock"]:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    password = sys.stdin.read()
    if not available():
        return 0
    try:
        print(unlock(password))
    except Exception as error:  # noqa: BLE001 - un trousseau fermé ne bloque jamais la session
        print(f"Trousseau non ouvert : {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

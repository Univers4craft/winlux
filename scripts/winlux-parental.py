#!/usr/bin/python3
"""Contrôle parental de Winlux (Paramètres › Comptes › Contrôle parental, #661).

Entièrement local : comptes locaux, aucun service en ligne, rien ne quitte la
machine. Un administrateur surveille un compte standard :

* temps d'écran par jour de la semaine, et plage horaire autorisée ;
* avertissement 5 minutes puis 1 minute avant la fin, puis écran de blocage
  (« Temps d'écran terminé ») ; un adulte peut accorder du temps sur place
  avec son mot de passe, ou depuis ses Paramètres ;
* applications bloquées (programmes Windows .exe et applications Linux) ;
* sites bloqués, ou seulement les sites autorisés, et recherches filtrées
  (SafeSearch de Google, Bing, DuckDuckGo et le mode restreint de YouTube),
  par un DNS filtrant propre à ce compte ;
* compte rendu d'activité : temps par jour, applications utilisées,
  tentatives bloquées.

Le compte surveillé ne peut ni lire ni modifier ses restrictions :

* configuration dans /etc/winlux/parental (root, 0700, fichiers 0600) et
  activité dans /var/lib/winlux/parental (root, 0700) ;
* tout passe par le relais root (winlux-root) : lire la configuration
  demande un compte administrateur, la modifier demande en plus la
  confirmation du Contrôle de compte d'utilisateur ; le compte surveillé ne
  voit que son temps restant (`status`) ;
* le service tourne en root (lancé par l'init, pas de systemd) ; l'écran de
  blocage aussi : le compte ne peut pas le tuer, et s'il le ferme par X11
  (xkill) l'écran revient et ses programmes sont suspendus ;
* le relais refuse de déverrouiller la session ou d'y basculer hors du temps
  permis, et réserve le changement de fuseau horaire à un administrateur
  pour un compte surveillé (l'horloge elle-même est déjà réservée) ;
* le DNS du compte est redirigé par nftables (meta skuid) vers ce service ;
  DNS chiffré (port 853) et résolveurs DoH connus refusés pour ce compte.

  winlux-parental status                     (relais) temps restant du compte appelant
  winlux-parental list                       (admin) comptes et résumé du jour
  winlux-parental get COMPTE                 (admin) configuration (JSON)
  winlux-parental set COMPTE                 (admin) configuration JSON sur l'entrée standard
  winlux-parental enable|disable COMPTE      (admin) active ou suspend la surveillance
  winlux-parental grant COMPTE MINUTES       (admin) temps accordé maintenant
  winlux-parental report COMPTE [JOURS]      (admin) compte rendu (7 jours par défaut)
  winlux-parental forget COMPTE              (admin) efface configuration et activité
  winlux-parental check-login COMPTE         (root) 0 si le compte peut ouvrir sa session
  winlux-parental daemon                     (root) suivi, blocages, DNS filtrant
  winlux-parental supervise                  (root) lance le service et le relance s'il s'arrête
  winlux-parental screen COMPTE ÉCRAN        (root) écran « Temps d'écran terminé »

Tests : WINLUX_PARENTAL_ROOT (racine factice), WINLUX_PARENTAL_NOW (heure
fixée, secondes depuis 1970), WINLUX_PARENTAL_NFT (faux nft),
WINLUX_PARENTAL_UPSTREAM_PORT (port du faux résolveur amont).
"""
import datetime
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import shutil
import signal
import socket
import struct
import subprocess
import sys
import threading
import time

ROOT = os.environ.get("WINLUX_PARENTAL_ROOT", "")
CONF_DIR = Path(ROOT + "/etc/winlux/parental")
DATA_DIR = Path(ROOT + "/var/lib/winlux/parental")
RUN_DIR = Path(ROOT + "/run/winlux")
SESSIONS = RUN_DIR / "sessions"
# État du service qui doit survivre à sa relance (#2246) : comptes gelés.
# Hors du dossier collant /run/winlux, qu'un compte peut préparer (#633).
STATE_DIR = Path(ROOT + "/run/winlux-parental")
FROZEN_FILE = STATE_DIR / "frozen.json"
ORPHAN_GRACE = 5
USERS_CONF = Path(ROOT + "/etc/winlux/users.conf")
PASSWD = Path(ROOT + "/etc/passwd")
GROUP = Path(ROOT + "/etc/group")
RESOLV = Path(ROOT + "/etc/resolv.conf")
NFT = os.environ.get("WINLUX_PARENTAL_NFT", "nft")
RELAY_SOCKET = os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")
SELF = os.path.abspath(__file__)

DAYS = ("mon", "tue", "wed", "thu", "fri", "sat", "sun")
ADMIN_GROUPS = ("sudo", "wheel", "admin")
NAME = re.compile(r"^[a-z_][a-z0-9_.-]{0,31}$")
CLOCK = re.compile(r"^([01][0-9]|2[0-3]):[0-5][0-9]$")
APP_ID = re.compile(r"^(exe|bin):[^\s/\\:]{1,128}$")
DOMAIN = re.compile(r"^(?=.{1,253}$)([a-z0-9_]([a-z0-9_-]{0,61}[a-z0-9_])?\.)+[a-z][a-z0-9-]{0,62}$")
KEEP_DAYS = 35
WARN_AT = (5, 1)
TABLE = "winlux_parental"
DNS_BASE = 54000
TAMPER_FREEZE = 2
# #815 : empreinte d'un programme (taille, 4 Mo du début, 1 Mo de la fin) pour
# reconnaître une copie renommée d'une application bloquée.
PRINT_HEAD = 4 << 20
PRINT_TAIL = 1 << 20
PRINTS_KEEP = 4000
# #816 : adresses obtenues par le DNS filtrant, seules joignables par le compte.
ADDRESS_TIMEOUT = 3 * 3600
ADDRESS_REFRESH = 2 * 3600
LOCAL_IPV4 = ("127.0.0.0/8", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "169.254.0.0/16", "224.0.0.0/4",
              "255.255.255.255")
LOCAL_IPV6 = ("::1", "fe80::/10", "fc00::/7", "ff00::/8")

# Programmes du bureau lui-même : jamais bloqués, jamais comptés comme application.
PROTECTED = {"xorg", "openbox", "picom", "xfce4-panel", "xfdesktop", "xfconfd", "xfsettingsd", "dbus-daemon",
             "dbus-run-session", "pipewire", "pipewire-pulse", "wireplumber", "pulseaudio", "python3", "python",
             "sh", "bash", "dash", "busybox", "env", "setpriv", "prlimit", "xscreensaver", "at-spi-bus-launcher",
             "at-spi2-registryd", "gvfsd", "polkit-agent-helper-1", "xdg-desktop-portal", "ibus-daemon",
             "notify-send", "thunar"}
# Applications PE du bureau Winlux (Explorateur, Boutique, Gestionnaire des
# tâches, Bloc-notes PE…) : toutes tournent comme ce seul programme, lancé par
# winlux-shell-app. Le bloquer fermait le bureau entier du compte (#2293).
SHELL_EXE = "gui_demo.exe"
SHELL_EXE_PATH = ROOT + "/opt/winlux/tests/" + SHELL_EXE

# Résolveurs DNS chiffrés connus : le compte surveillé ne doit pas contourner
# le DNS filtrant (DoH dans les réglages d'un navigateur). Le domaine
# use-application-dns.net est le signal documenté par Mozilla qui coupe le
# DoH automatique de Firefox.
DOH_HOSTS = ("use-application-dns.net", "cloudflare-dns.com", "one.one.one.one", "dns.google", "dns.google.com",
             "dns.quad9.net", "doh.opendns.com", "dns.nextdns.io", "dns.adguard.com", "dns.adguard-dns.com",
             "doh.cleanbrowsing.org", "dns.mullvad.net", "doh.mullvad.net", "doh.libredns.gr", "dns.controld.com")
DOH_IPV4 = ("1.1.1.1", "1.0.0.1", "8.8.8.8", "8.8.4.4", "9.9.9.9", "149.112.112.112", "94.140.14.14",
            "94.140.15.15", "208.67.222.222", "208.67.220.220", "45.90.28.0/24", "45.90.30.0/24",
            "185.228.168.9", "185.228.169.9", "194.242.2.2", "76.76.2.0/24")
DOH_IPV6 = ("2606:4700:4700::1111", "2606:4700:4700::1001", "2001:4860:4860::8888", "2001:4860:4860::8844",
            "2620:fe::fe", "2620:fe::9", "2a10:50c0::ad1:ff", "2a10:50c0::ad2:ff", "2620:119:35::35",
            "2620:119:53::53", "2a07:a8c0::/32", "2a07:a8c1::/32")

# Recherches filtrées : noms publics documentés par chaque moteur.
# VPN et proxys web grand public (#816) : refusés dès que le filtre web est
# actif, comme les DoH connus. Les adresses que le DNS filtrant n'a pas données
# sont de toute façon injoignables par le compte.
VPN_HOSTS = ("nordvpn.com", "nordvpn.net", "expressvpn.com", "protonvpn.com", "protonvpn.ch", "surfshark.com",
             "cyberghostvpn.com", "privateinternetaccess.com", "windscribe.com", "windscribe.net", "hide.me",
             "tunnelbear.com", "hotspotshield.com", "ipvanish.com", "vyprvpn.com", "purevpn.com", "hola.org",
             "urban-vpn.com", "browsec.com", "zenmate.com", "psiphon.ca", "psiphon3.com", "torproject.org",
             "hidemy.name", "hidester.com", "proxysite.com", "kproxy.com", "croxyproxy.com", "croxyproxy.rocks",
             "4everproxy.com", "vpnbook.com", "betternet.co", "tuxlervpn.com", "planetvpn.com", "privadovpn.com",
             "atlasvpn.com", "mullvad.net", "ivpn.net", "airvpn.org", "warp.plus", "cloudflarewarp.com")

SAFE_TARGETS = (
    (re.compile(r"^(www\.)?google\.(com|[a-z]{2}|com?\.[a-z]{2})$"), "forcesafesearch.google.com"),
    (re.compile(r"^(www\.)?bing\.com$"), "strict.bing.com"),
    (re.compile(r"^(www\.)?duckduckgo\.com$"), "safe.duckduckgo.com"),
    (re.compile(r"^(www\.|m\.)?youtube\.com$|^youtubei?\.googleapis\.com$|^www\.youtube-nocookie\.com$"),
     "restrict.youtube.com"),
)


# ── Langue ──────────────────────────────────────────────────────────────────

def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open(ROOT + "/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="): code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}
# (anglais, espagnol, allemand) ; le français sert de clé.
_TR = {
    "lundi": ("Monday", "lunes", "Montag"),
    "mardi": ("Tuesday", "martes", "Dienstag"),
    "mercredi": ("Wednesday", "miércoles", "Mittwoch"),
    "jeudi": ("Thursday", "jueves", "Donnerstag"),
    "vendredi": ("Friday", "viernes", "Freitag"),
    "samedi": ("Saturday", "sábado", "Samstag"),
    "dimanche": ("Sunday", "domingo", "Sonntag"),
    "Illimité": ("Unlimited", "Ilimitado", "Unbegrenzt"),
    "Toute la journée": ("All day", "Todo el día", "Ganztägig"),
    "Temps d’écran terminé": ("Screen time is up", "Se acabó el tiempo de pantalla", "Bildschirmzeit ist abgelaufen"),
    "Ce n’est pas le moment d’utiliser l’ordinateur": ("This is not a time to use the computer",
                                                       "No es momento de usar el ordenador",
                                                       "Jetzt ist keine Computerzeit"),
    "{name} pourra de nouveau utiliser l’ordinateur aujourd’hui à {time}.": (
        "{name} can use the computer again today at {time}.",
        "{name} podrá volver a usar el ordenador hoy a las {time}.",
        "{name} kann den Computer heute um {time} wieder benutzen."),
    "{name} pourra de nouveau utiliser l’ordinateur demain à {time}.": (
        "{name} can use the computer again tomorrow at {time}.",
        "{name} podrá volver a usar el ordenador mañana a las {time}.",
        "{name} kann den Computer morgen um {time} wieder benutzen."),
    "Les applications restent ouvertes derrière cet écran.": (
        "Apps stay open behind this screen.", "Las aplicaciones siguen abiertas detrás de esta pantalla.",
        "Die Apps bleiben hinter diesem Bildschirm geöffnet."),
    "Vous pouvez de nouveau utiliser l’ordinateur": ("You can use the computer again",
                                                     "Puede volver a usar el ordenador",
                                                     "Sie können den Computer wieder benutzen"),
    "Mot de passe de {name}": ("Password for {name}", "Contraseña de {name}", "Kennwort von {name}"),
    "Reprendre": ("Resume", "Reanudar", "Fortsetzen"),
    "Un adulte accorde du temps": ("An adult grants more time", "Un adulto concede más tiempo",
                                   "Ein Erwachsener gibt mehr Zeit"),
    "Changer d’utilisateur": ("Switch user", "Cambiar de usuario", "Benutzer wechseln"),
    "Administrateur": ("Administrator", "Administrador", "Administrator"),
    "Mot de passe": ("Password", "Contraseña", "Kennwort"),
    "Accorder": ("Grant", "Conceder", "Gewähren"),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "Ouvrir la session": ("Sign in", "Iniciar sesión", "Anmelden"),
    "Compte": ("Account", "Cuenta", "Konto"),
    "Comptes": ("Accounts", "Cuentas", "Konten"),
    "Le mot de passe est incorrect.": ("The password is incorrect.", "La contraseña no es correcta.",
                                       "Das Kennwort ist falsch."),
    "Temps d’écran : il reste {minutes} minutes": ("Screen time: {minutes} minutes left",
                                                   "Tiempo de pantalla: quedan {minutes} minutos",
                                                   "Bildschirmzeit: noch {minutes} Minuten"),
    "Temps d’écran : il reste 1 minute": ("Screen time: 1 minute left", "Tiempo de pantalla: queda 1 minuto",
                                          "Bildschirmzeit: noch 1 Minute"),
    "Enregistrez votre travail : l’ordinateur sera bientôt bloqué.": (
        "Save your work: the computer will be locked soon.",
        "Guarde su trabajo: el ordenador se bloqueará pronto.",
        "Speichern Sie Ihre Arbeit: Der Computer wird bald gesperrt."),
    "Contrôle parental": ("Parental controls", "Control parental", "Jugendschutz"),
    "« {name} » est bloquée par le contrôle parental.": ("“{name}” is blocked by parental controls.",
                                                         "«{name}» está bloqueada por el control parental.",
                                                         "„{name}“ ist durch den Jugendschutz gesperrt."),
    "Le site {name} est bloqué par le contrôle parental.": ("The site {name} is blocked by parental controls.",
                                                            "El sitio {name} está bloqueado por el control parental.",
                                                            "Die Website {name} ist durch den Jugendschutz gesperrt."),
    # Paramètres
    "Un administrateur fixe pour un compte standard un temps d’écran, des horaires, des applications et des sites bloqués, et consulte son activité. Tout reste sur ce PC : aucun compte en ligne, aucun envoi de données.": (
        "An administrator sets screen time, hours, blocked apps and sites for a standard account, and reviews its activity. Everything stays on this PC: no online account, no data sent.",
        "Un administrador fija para una cuenta estándar un tiempo de pantalla, horarios, aplicaciones y sitios bloqueados, y consulta su actividad. Todo se queda en este PC: sin cuenta en línea ni envío de datos.",
        "Ein Administrator legt für ein Standardkonto Bildschirmzeit, Zeiten sowie gesperrte Apps und Websites fest und sieht die Aktivität ein. Alles bleibt auf diesem PC: kein Onlinekonto, keine Datenübertragung."),
    "Seul un administrateur peut gérer le contrôle parental de ce PC.": (
        "Only an administrator can manage parental controls on this PC.",
        "Solo un administrador puede gestionar el control parental de este PC.",
        "Nur ein Administrator kann den Jugendschutz dieses PCs verwalten."),
    "Gérer en tant qu’administrateur…": ("Manage as administrator…", "Gestionar como administrador…",
                                         "Als Administrator verwalten…"),
    "Votre compte est géré par le contrôle parental.": ("Your account is managed by parental controls.",
                                                        "Su cuenta está gestionada por el control parental.",
                                                        "Ihr Konto wird durch den Jugendschutz verwaltet."),
    "Temps restant aujourd’hui : {time}": ("Time left today: {time}", "Tiempo restante hoy: {time}",
                                           "Heute verbleibend: {time}"),
    "Pas de limite de temps aujourd’hui.": ("No time limit today.", "Sin límite de tiempo hoy.",
                                            "Heute keine Zeitbegrenzung."),
    "Le service du contrôle parental ne répond pas.": ("The parental controls service is not responding.",
                                                       "El servicio de control parental no responde.",
                                                       "Der Jugendschutzdienst antwortet nicht."),
    "Réessayer": ("Try again", "Reintentar", "Erneut versuchen"),
    "Comptes de la famille": ("Family accounts", "Cuentas de la familia", "Konten der Familie"),
    "Aucun compte standard sur ce PC. Créez d’abord un compte pour l’enfant dans Comptes.": (
        "No standard account on this PC. First create an account for the child in Accounts.",
        "No hay ninguna cuenta estándar en este PC. Cree primero una cuenta para el menor en Cuentas.",
        "Kein Standardkonto auf diesem PC. Legen Sie zuerst unter Konten ein Konto für das Kind an."),
    "Un compte administrateur ne peut pas être surveillé : rendez-le standard dans Comptes.": (
        "An administrator account cannot be supervised: make it standard in Accounts.",
        "Una cuenta de administrador no se puede supervisar: conviértala en estándar en Cuentas.",
        "Ein Administratorkonto kann nicht beaufsichtigt werden: Machen Sie es unter Konten zum Standardkonto."),
    "Surveillé · aujourd’hui {used}": ("Supervised · today {used}", "Supervisada · hoy {used}",
                                       "Beaufsichtigt · heute {used}"),
    "Surveillé · aujourd’hui {used} sur {limit}": ("Supervised · today {used} of {limit}",
                                                   "Supervisada · hoy {used} de {limit}",
                                                   "Beaufsichtigt · heute {used} von {limit}"),
    "Non surveillé": ("Not supervised", "Sin supervisar", "Nicht beaufsichtigt"),
    "Surveillance suspendue": ("Supervision paused", "Supervisión en pausa", "Aufsicht pausiert"),
    "Activer le contrôle parental": ("Turn on parental controls", "Activar el control parental",
                                     "Jugendschutz aktivieren"),
    "Gérer": ("Manage", "Gestionar", "Verwalten"),
    "‹ Tous les comptes": ("‹ All accounts", "‹ Todas las cuentas", "‹ Alle Konten"),
    "Temps d’écran": ("Screen time", "Tiempo de pantalla", "Bildschirmzeit"),
    "Durée maximale par jour et heures où l’ordinateur peut servir. Un avertissement arrive 5 minutes avant la fin.": (
        "Maximum time per day and the hours the computer can be used. A warning comes 5 minutes before the end.",
        "Tiempo máximo por día y horas en que se puede usar el ordenador. Un aviso llega 5 minutos antes del final.",
        "Höchstdauer pro Tag und Uhrzeiten, zu denen der Computer genutzt werden darf. 5 Minuten vor Ende kommt eine Warnung."),
    "de": ("from", "de", "von"),
    "à": ("to", "a", "bis"),
    "Appliquer lundi à tous les jours": ("Apply Monday to every day", "Aplicar el lunes a todos los días",
                                         "Montag auf alle Tage anwenden"),
    "Accorder du temps maintenant": ("Grant time now", "Conceder tiempo ahora", "Jetzt Zeit gewähren"),
    "Temps accordé jusqu’à {time}.": ("Time granted until {time}.", "Tiempo concedido hasta las {time}.",
                                      "Zeit gewährt bis {time}."),
    "Applications": ("Apps", "Aplicaciones", "Apps"),
    "Une application bloquée se ferme aussitôt ouverte, et la tentative apparaît dans l’activité.": (
        "A blocked app closes as soon as it opens, and the attempt shows up in the activity.",
        "Una aplicación bloqueada se cierra en cuanto se abre, y el intento aparece en la actividad.",
        "Eine gesperrte App wird sofort nach dem Öffnen geschlossen, und der Versuch erscheint in der Aktivität."),
    "Aucune application bloquée.": ("No blocked apps.", "Ninguna aplicación bloqueada.", "Keine gesperrten Apps."),
    "Débloquer": ("Unblock", "Desbloquear", "Entsperren"),
    "Bloquer": ("Block", "Bloquear", "Sperren"),
    "Choisir une application…": ("Choose an app…", "Elegir una aplicación…", "App auswählen…"),
    "Sites web": ("Websites", "Sitios web", "Websites"),
    "S’applique à tous les navigateurs de ce compte.": ("Applies to every browser of this account.",
                                                        "Se aplica a todos los navegadores de esta cuenta.",
                                                        "Gilt für alle Browser dieses Kontos."),
    "Filtre": ("Filter", "Filtro", "Filter"),
    "Aucun filtre": ("No filter", "Sin filtro", "Kein Filter"),
    "Bloquer les sites de la liste": ("Block the sites in the list", "Bloquear los sitios de la lista",
                                      "Websites der Liste sperren"),
    "Autoriser seulement les sites de la liste": ("Allow only the sites in the list",
                                                  "Permitir solo los sitios de la lista",
                                                  "Nur Websites der Liste erlauben"),
    "Filtrer les recherches pour adultes": ("Filter adult search results", "Filtrar búsquedas para adultos",
                                            "Suchergebnisse für Erwachsene filtern"),
    "Google, Bing et DuckDuckGo en recherche protégée, YouTube en mode restreint.": (
        "Google, Bing and DuckDuckGo in safe search, YouTube in restricted mode.",
        "Google, Bing y DuckDuckGo en búsqueda segura, YouTube en modo restringido.",
        "Google, Bing und DuckDuckGo mit sicherer Suche, YouTube im eingeschränkten Modus."),
    "Sites bloqués": ("Blocked sites", "Sitios bloqueados", "Gesperrte Websites"),
    "Sites autorisés": ("Allowed sites", "Sitios permitidos", "Erlaubte Websites"),
    "Adresse (par exemple exemple.com)": ("Address (for example example.com)", "Dirección (por ejemplo ejemplo.com)",
                                          "Adresse (zum Beispiel beispiel.de)"),
    "Ajouter": ("Add", "Añadir", "Hinzufügen"),
    "Retirer": ("Remove", "Quitar", "Entfernen"),
    "Liste vide.": ("Empty list.", "Lista vacía.", "Leere Liste."),
    "Adresse non valable : {name}": ("Invalid address: {name}", "Dirección no válida: {name}",
                                      "Ungültige Adresse: {name}"),
    "Application introuvable : {name}. Rien n’a été bloqué ; choisissez-la dans la liste.": (
        "App not found: {name}. Nothing was blocked; choose it from the list.",
        "Aplicación no encontrada: {name}. No se ha bloqueado nada; elíjala de la lista.",
        "App nicht gefunden: {name}. Es wurde nichts gesperrt; wählen Sie sie aus der Liste."),
    "Rien n’a été bloqué. Vouliez-vous dire {names} ? Choisissez-la puis cliquez sur Bloquer.": (
        "Nothing was blocked. Did you mean {names}? Choose it, then select Block.",
        "No se ha bloqueado nada. ¿Quería decir {names}? Elíjala y haga clic en Bloquear.",
        "Es wurde nichts gesperrt. Meinten Sie {names}? Wählen Sie sie aus und klicken Sie auf Sperren."),
    "Rien n’a été bloqué. Vouliez-vous dire {names} ? Elle est sélectionnée : cliquez à nouveau sur Bloquer.": (
        "Nothing was blocked. Did you mean {names}? It is selected: select Block again.",
        "No se ha bloqueado nada. ¿Quería decir {names}? Está seleccionada: vuelva a hacer clic en Bloquear.",
        "Es wurde nichts gesperrt. Meinten Sie {names}? Sie ist ausgewählt: Klicken Sie erneut auf Sperren."),
    " ou ": (" or ", " o ", " oder "),
    "Activité des 7 derniers jours": ("Activity over the last 7 days", "Actividad de los últimos 7 días",
                                      "Aktivität der letzten 7 Tage"),
    "Applications les plus utilisées": ("Most used apps", "Aplicaciones más usadas", "Meistgenutzte Apps"),
    "Tentatives bloquées": ("Blocked attempts", "Intentos bloqueados", "Gesperrte Versuche"),
    "Aucune activité enregistrée.": ("No activity recorded.", "No hay actividad registrada.",
                                     "Keine Aktivität aufgezeichnet."),
    "{count} fois": ("{count} times", "{count} veces", "{count}-mal"),
    "Effacer l’activité et les réglages": ("Clear activity and settings", "Borrar actividad y ajustes",
                                           "Aktivität und Einstellungen löschen"),
    "Enregistré.": ("Saved.", "Guardado.", "Gespeichert."),
    "Modification annulée : confirmation d’un administrateur refusée.": (
        "Change cancelled: administrator confirmation refused.",
        "Cambio cancelado: confirmación de administrador rechazada.",
        "Änderung abgebrochen: Administratorbestätigung verweigert."),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


DAY_NAMES = {"mon": "lundi", "tue": "mardi", "wed": "mercredi", "thu": "jeudi", "fri": "vendredi",
             "sat": "samedi", "sun": "dimanche"}


def duration(minutes):
    """45 min, 1 h, 2 h 30."""
    minutes = max(0, int(minutes))
    if minutes < 60:
        return f"{minutes} min"
    hours, rest = divmod(minutes, 60)
    return f"{hours} h {rest:02d}" if rest else f"{hours} h"


# ── Comptes ─────────────────────────────────────────────────────────────────

def read_lines(path):
    try:
        return Path(path).read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return []


def users_conf():
    out = {}
    for line in read_lines(USERS_CONF):
        parts = line.split(":", 2)
        if len(parts) == 3 and parts[0]:
            out[parts[0]] = (parts[1], parts[2])
    return out


def passwd():
    """{nom: (uid, gid, nom complet, dossier)} des comptes du bureau (UID ≥ 1000)."""
    out = {}
    if ROOT or PASSWD.exists():
        lines = read_lines(PASSWD)
    else:
        lines = []
    for line in lines:
        parts = line.split(":")
        if len(parts) >= 7 and parts[2].isdigit() and int(parts[2]) >= 1000 and parts[0] != "winlux-web":
            out[parts[0]] = (int(parts[2]), int(parts[3]) if parts[3].isdigit() else int(parts[2]),
                             parts[4].split(",")[0], parts[5])
    return out


def is_admin(name):
    if name == "root":
        return True
    role = users_conf().get(name, ("", ""))[0]
    if role in ("root", "admin", "administrateur"):
        return True
    for line in read_lines(GROUP):
        parts = line.split(":")
        if len(parts) >= 4 and parts[0] in ADMIN_GROUPS and name in parts[3].split(","):
            return True
    return False


def desktop_accounts():
    """Comptes de l'écran de connexion : [{name, fullname, uid, admin}]."""
    conf = users_conf()
    out = []
    for name, (uid, _gid, fullname, _home) in sorted(passwd().items()):
        if name in conf:
            out.append({"name": name, "fullname": fullname, "uid": uid, "admin": is_admin(name)})
    return out


def uid_of(name):
    entry = passwd().get(name)
    if entry:
        return entry[0]
    try:
        return pwd.getpwnam(name).pw_uid
    except KeyError:
        return None


# ── Configuration (root seulement) ──────────────────────────────────────────

def private_dir(path):
    """Dossier root 0700 : refusé s'il est un lien ou appartient à un compte."""
    path.mkdir(parents=True, exist_ok=True)
    st = os.lstat(path)
    if os.path.islink(path) or (not ROOT and st.st_uid != 0):
        raise PermissionError(f"{path} n'appartient pas à root")
    os.chmod(path, 0o700)
    return path


def write_private(path, data):
    for top in (CONF_DIR, DATA_DIR):
        if path.is_relative_to(top):
            private_dir(top)
    private_dir(path.parent)
    tmp = path.with_name(f".{path.name}.{os.getpid()}.{threading.get_ident()}")
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as out:
        json.dump(data, out, ensure_ascii=False, indent=1, sort_keys=True)
    os.replace(tmp, path)


def read_json(path):
    try:
        if os.path.islink(path):
            return None
        with open(path, encoding="utf-8") as handle:
            data = json.load(handle)
        return data if isinstance(data, dict) else None
    except (OSError, ValueError):
        return None


def default_config():
    return {"enabled": True,
            "days": {day: {"limit": None, "from": None, "to": None} for day in DAYS},
            "apps": [],
            "web": {"mode": "off", "blocked": [], "allowed": [], "safesearch": False}}


def clean_domain(text):
    text = str(text).strip().lower()
    text = re.sub(r"^[a-z]+://", "", text).split("/")[0].split(":")[0].strip(".")
    if text.startswith("www.") and text.count(".") >= 2:
        text = text[4:]
    try:
        text = text.encode("idna").decode("ascii")
    except UnicodeError:
        return None
    return text if DOMAIN.match(text) else None


def validate(data):
    """Configuration nettoyée ; ValueError avec un message lisible sinon."""
    if not isinstance(data, dict):
        raise ValueError("configuration invalide")
    cfg = default_config()
    cfg["enabled"] = bool(data.get("enabled", True))
    days = data.get("days") or {}
    if not isinstance(days, dict):
        raise ValueError("jours invalides")
    for day in DAYS:
        rule = days.get(day) or {}
        if not isinstance(rule, dict):
            raise ValueError(f"jour invalide : {day}")
        limit = rule.get("limit")
        if limit is not None:
            if isinstance(limit, bool) or not isinstance(limit, int) or not 0 <= limit <= 1440:
                raise ValueError(f"durée invalide : {day}")
        start, end = rule.get("from"), rule.get("to")
        if (start is None) != (end is None):
            raise ValueError(f"plage incomplète : {day}")
        if start is not None:
            if not (isinstance(start, str) and CLOCK.match(start) and isinstance(end, str) and CLOCK.match(end)) \
                    or clock(start) >= clock(end):
                raise ValueError(f"plage invalide : {day}")
        cfg["days"][day] = {"limit": limit, "from": start, "to": end}
    apps = data.get("apps") or []
    if not isinstance(apps, list) or len(apps) > 200:
        raise ValueError("applications invalides")
    seen = set()
    for app in apps:
        if not isinstance(app, dict) or not isinstance(app.get("id"), str):
            raise ValueError("application invalide")
        ident = app["id"].lower() if app["id"].startswith("exe:") else app["id"]
        if not APP_ID.match(ident) or protected(ident):
            raise ValueError(f"application invalide : {app['id']}")
        name = str(app.get("name") or ident.split(":", 1)[1])[:80]
        if ident not in seen:
            seen.add(ident)
            cfg["apps"].append({"id": ident, "name": name})
    web = data.get("web") or {}
    if not isinstance(web, dict) or web.get("mode", "off") not in ("off", "block", "allow"):
        raise ValueError("filtre web invalide")
    cfg["web"]["mode"] = web.get("mode", "off")
    cfg["web"]["safesearch"] = bool(web.get("safesearch", False))
    for key in ("blocked", "allowed"):
        items = web.get(key) or []
        if not isinstance(items, list) or len(items) > 1000:
            raise ValueError("liste de sites invalide")
        out = []
        for item in items:
            domain = clean_domain(item)
            if domain is None:
                raise ValueError(f"adresse invalide : {item}")
            if domain not in out:
                out.append(domain)
        cfg["web"][key] = out
    return cfg


def config_path(user):
    return CONF_DIR / f"{user}.json"


def load_config(user):
    if not NAME.match(user or ""):
        return None
    data = read_json(config_path(user))
    if data is None:
        return None
    apps = data.get("apps") if isinstance(data, dict) else None
    if isinstance(apps, list):
        # Programme du bureau bloqué avant d'être protégé (exe:gui_demo.exe,
        # #2293) : retiré, sans perdre le reste des règles du compte.
        data["apps"] = [app for app in apps if not (isinstance(app, dict) and isinstance(app.get("id"), str)
                                                    and protected(app["id"].lower()))]
    try:
        return validate(data)
    except ValueError:
        return None


def supervised_accounts():
    """{nom: configuration} des comptes surveillés existants, non administrateurs."""
    out = {}
    try:
        names = sorted(p.stem for p in CONF_DIR.glob("*.json"))
    except OSError:
        return out
    accounts = passwd()
    for name in names:
        cfg = load_config(name)
        if cfg and cfg["enabled"] and name in accounts and not is_admin(name):
            out[name] = cfg
    return out


# ── Activité ────────────────────────────────────────────────────────────────

def now_time():
    fixed = os.environ.get("WINLUX_PARENTAL_NOW")
    return datetime.datetime.fromtimestamp(float(fixed)) if fixed else datetime.datetime.now()


def clock(text):
    return int(text[:2]) * 60 + int(text[3:5])


def usage_path(user, day):
    return DATA_DIR / user / f"{day.isoformat()}.json"


def load_usage(user, day):
    data = read_json(usage_path(user, day)) or {}
    usage = {"minutes": 0, "apps": {}, "names": {}, "blocked_apps": {}, "blocked_sites": {}, "grant_until": 0,
             "grants": []}
    for key, value in data.items():
        if key == "grant_until" and isinstance(value, (int, float)) and not isinstance(value, bool):
            usage[key] = float(value)
        elif key in usage and isinstance(value, type(usage[key])) and not isinstance(value, bool):
            usage[key] = value
    return usage


def save_usage(user, day, usage):
    write_private(usage_path(user, day), usage)


LOCK = threading.Lock()


def update_usage(user, change, moment=None):
    moment = moment or now_time()
    with LOCK:
        usage = load_usage(user, moment.date())
        change(usage)
        save_usage(user, moment.date(), usage)
        return usage


def prune(user, moment=None):
    moment = moment or now_time()
    limit = (moment.date() - datetime.timedelta(days=KEEP_DAYS)).isoformat()
    try:
        for path in (DATA_DIR / user).glob("*.json"):
            if path.stem < limit:
                path.unlink()
    except OSError:
        pass


def allowance(cfg, usage, moment):
    """Temps permis maintenant : allowed, reason (limit | schedule), remaining
    (minutes, None si illimité), until (heure de reprise) et until_day."""
    if not cfg or not cfg.get("enabled"):
        return {"supervised": False, "allowed": True, "remaining": None}
    day = DAYS[moment.weekday()]
    rule = cfg["days"][day]
    minute = moment.hour * 60 + moment.minute
    used = int(usage.get("minutes", 0))
    out = {"supervised": True, "used": used, "limit": rule["limit"], "reason": None, "remaining": None,
           "until": None, "until_day": None, "granted": False}
    grant_until = float(usage.get("grant_until", 0) or 0)
    if grant_until > moment.timestamp():
        left = int((grant_until - moment.timestamp() + 59) // 60)
        out.update(allowed=True, remaining=left, granted=True,
                   grant_until=datetime.datetime.fromtimestamp(grant_until).strftime("%H:%M"))
        return out
    tomorrow = cfg["days"][DAYS[(moment.weekday() + 1) % 7]]
    next_start = tomorrow["from"] or "00:00"
    if rule["from"] is not None:
        start, end = clock(rule["from"]), clock(rule["to"])
        if minute < start:
            out.update(allowed=False, reason="schedule", until=rule["from"], until_day="today")
            return out
        if minute >= end:
            out.update(allowed=False, reason="schedule", until=next_start, until_day="tomorrow")
            return out
        out["remaining"] = end - minute
    if rule["limit"] is not None:
        left = rule["limit"] - used
        if left <= 0:
            out.update(allowed=False, reason="limit", until=next_start, until_day="tomorrow")
            return out
        out["remaining"] = left if out["remaining"] is None else min(out["remaining"], left)
    out["allowed"] = True
    return out


def state_of(user, moment=None):
    moment = moment or now_time()
    cfg = load_config(user)
    if cfg is None or not cfg["enabled"] or is_admin(user):
        return {"supervised": False, "allowed": True, "remaining": None}
    return allowance(cfg, load_usage(user, moment.date()), moment)


def grant(user, minutes, by="", moment=None):
    moment = moment or now_time()

    def change(usage):
        start = max(moment.timestamp(), float(usage.get("grant_until", 0) or 0))
        usage["grant_until"] = start + minutes * 60
        usage["grants"].append({"at": int(moment.timestamp()), "minutes": minutes, "by": by})
    usage = update_usage(user, change, moment)
    return datetime.datetime.fromtimestamp(usage["grant_until"]).strftime("%H:%M")


def report(user, days=7, moment=None):
    moment = moment or now_time()
    per_day, apps, names, blocked_apps, blocked_sites, grants = [], {}, {}, {}, {}, []
    for back in range(days - 1, -1, -1):
        day = moment.date() - datetime.timedelta(days=back)
        usage = load_usage(user, day)
        per_day.append({"date": day.isoformat(), "day": DAYS[day.weekday()], "minutes": usage["minutes"]})
        for key, value in usage["apps"].items():
            apps[key] = apps.get(key, 0) + int(value)
        names.update(usage["names"])
        for key, value in usage["blocked_apps"].items():
            blocked_apps[key] = blocked_apps.get(key, 0) + int(value)
        for key, value in usage["blocked_sites"].items():
            blocked_sites[key] = blocked_sites.get(key, 0) + int(value)
        grants += usage["grants"]
    top = sorted(apps.items(), key=lambda item: -item[1])[:15]
    return {"days": per_day,
            "apps": [{"id": key, "name": display_name(key, names), "minutes": value} for key, value in top],
            "blocked_apps": [{"id": key, "name": display_name(key, names), "count": value}
                             for key, value in sorted(blocked_apps.items(), key=lambda item: -item[1])[:15]],
            "blocked_sites": [{"site": key, "count": value}
                              for key, value in sorted(blocked_sites.items(), key=lambda item: -item[1])[:15]],
            "grants": grants[-10:],
            "today": state_of(user, moment)}


# ── Applications ────────────────────────────────────────────────────────────

def protected(ident):
    kind, _, name = ident.partition(":")
    name = name.lower()
    if kind == "exe":
        return name == SHELL_EXE
    return kind == "bin" and (name in PROTECTED or name.startswith("winlux") or name.startswith("xfce4-")
                              or name.startswith("python3"))


def shell_process(pid, exe, cmdline):
    """Vrai si ce processus fait tourner le gui_demo.exe installé par Winlux
    (fichier de root), pas une copie qui en aurait pris le nom (#2293)."""
    path = app_file(pid, exe, cmdline)
    try:
        return bool(path) and os.path.samefile(path, SHELL_EXE_PATH)
    except (OSError, ValueError):
        return False


def app_of(exe, cmdline):
    """Identifiant de l'application d'un processus : exe:jeu.exe ou bin:firefox."""
    base = os.path.basename(exe or "")
    for arg in cmdline[1:] if base == "winlux" or base.startswith("winlux") else ():
        if arg.lower().endswith(".exe"):
            return "exe:" + re.split(r"[\\/]", arg)[-1].lower()
    if base.startswith("python") or base in ("sh", "bash", "dash", "perl"):
        for arg in cmdline[1:]:
            if not arg.startswith("-"):
                return "bin:" + os.path.basename(arg)
        return "bin:" + base
    if base.endswith(" (deleted)"):
        base = base[:-10]
    return "bin:" + base if base else None


def app_file(pid, exe, cmdline):
    """Fichier qui fait l'application d'un processus (#815) : le .exe lancé par
    winlux, le script d'un interpréteur, sinon le programme lui-même."""
    base = os.path.basename(exe or "")
    wanted = None
    if base.startswith("winlux"):
        wanted = next((arg for arg in cmdline[1:] if arg.lower().endswith(".exe")), None)
    elif base.startswith("python") or base in ("sh", "bash", "dash", "perl"):
        wanted = next((arg for arg in cmdline[1:] if not arg.startswith("-")), None)
    if wanted is None:
        return f"/proc/{pid}/exe"
    if "\\" not in wanted and not re.match(r"^[A-Za-z]:", wanted):
        if not wanted.startswith("/"):
            try:
                wanted = os.path.join(os.readlink(f"/proc/{pid}/cwd"), wanted)
            except OSError:
                return None
        return wanted
    # Chemin Windows : le fichier projeté en mémoire qui porte ce nom.
    name = re.split(r"[\\/]", wanted)[-1].lower()
    try:
        with open(f"/proc/{pid}/maps", encoding="utf-8", errors="replace") as handle:
            for line in handle:
                path = line.rstrip("\n").split(None, 5)[-1] if line.count(" ") >= 5 else ""
                if path.startswith("/") and os.path.basename(path).lower() == name:
                    return path
    except OSError:
        pass
    return None


def file_sample(path):
    """(clé du fichier, début, fin) d'un fichier ordinaire, sinon None."""
    # Chemin donné par le compte surveillé, ouvert en root : O_PATH d'abord, qui
    # n'ouvre pas un périphérique, puis lecture seulement d'un fichier ordinaire.
    try:
        fd = os.open(path, os.O_PATH | os.O_CLOEXEC)
    except (OSError, ValueError):
        return None
    try:
        st = os.fstat(fd)
        if not stat_regular(st.st_mode):
            return None
        with open(f"/proc/self/fd/{fd}", "rb") as handle:
            head = handle.read(PRINT_HEAD)
            tail = b""
            if st.st_size > PRINT_HEAD:
                handle.seek(max(PRINT_HEAD, st.st_size - PRINT_TAIL))
                tail = handle.read(PRINT_TAIL)
        return (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns), head, tail
    except OSError:
        return None
    finally:
        os.close(fd)


def stat_regular(mode):
    return (mode & 0o170000) == 0o100000


def fingerprint(key, head, tail):
    digest = hashlib.sha256(str(key[2]).encode() + b"\0" + head + b"\0" + tail)
    return digest.hexdigest()[:40]


ORIGINAL_NAME = "OriginalFilename".encode("utf-16-le") + b"\0\0"


def pe_original_name(data):
    """Nom d'origine inscrit dans la ressource VERSIONINFO d'un .exe (« jeu.exe »),
    qui ne change pas quand on renomme le fichier."""
    if data[:2] != b"MZ":
        return None
    start = 0
    while True:
        index = data.find(ORIGINAL_NAME, start)
        if index < 0:
            return None
        start = index + 2
        if index % 2:
            continue
        offset = (index + len(ORIGINAL_NAME) + 3) & ~3
        end = offset
        while end + 1 < len(data) and data[end:end + 2] != b"\0\0" and end - offset < 520:
            end += 2
        try:
            value = data[offset:end].decode("utf-16-le").strip()
        except UnicodeDecodeError:
            continue
        value = re.split(r"[\\/]", value)[-1].lower()
        if value.endswith(".exe") and 4 < len(value) <= 128:
            return value


def proc_uid(pid):
    try:
        return os.stat(f"/proc/{pid}").st_uid
    except OSError:
        return None


def proc_info(pid):
    try:
        exe = os.readlink(f"/proc/{pid}/exe")
    except OSError:
        exe = ""
    try:
        with open(f"/proc/{pid}/cmdline", "rb") as handle:
            cmdline = [part.decode("utf-8", "replace") for part in handle.read().split(b"\0") if part]
    except OSError:
        cmdline = []
    return exe, cmdline


def proc_env(pid):
    try:
        with open(f"/proc/{pid}/environ", "rb") as handle:
            pairs = handle.read().split(b"\0")
    except OSError:
        return {}
    out = {}
    for pair in pairs:
        key, sep, value = pair.partition(b"=")
        if sep:
            out[key.decode("utf-8", "replace")] = value.decode("utf-8", "replace")
    return out


def pids_of(uids):
    """{pid: uid} des processus de ces comptes."""
    out = {}
    try:
        entries = os.listdir("/proc")
    except OSError:
        return out
    for entry in entries:
        if entry.isdigit():
            uid = proc_uid(entry)
            if uid in uids:
                out[int(entry)] = uid
    return out


def stale_screens():
    """PID des écrans de blocage (« winlux-parental screen ») de root qui ne
    sont pas les enfants de ce service."""
    owner = 0 if not ROOT else os.getuid()
    found = []
    for pid in pids_of({owner}):
        if pid == os.getpid():
            continue
        _exe, cmdline = proc_info(pid)
        if len(cmdline) >= 4 and os.path.realpath(cmdline[-4]) == os.path.realpath(SELF) and cmdline[-3] == "screen":
            try:
                parent = int(Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[1])
            except (OSError, ValueError, IndexError):
                continue
            if parent != os.getpid():
                found.append(pid)
    return found


def shown_name(ident, stored, names):
    """Nom affiché d'une application bloquée. Pour un programme Linux, le nom du
    lanceur du menu (#2248) l'emporte sur celui enregistré au moment du blocage :
    un blocage fait avant #2248 gardait « Text Editor Settings » pour Mousepad (#2351)."""
    return names.get(ident, stored) if ident.startswith("bin:") else stored


def blocked_ids(cfg):
    return {app["id"]: app["name"] for app in cfg.get("apps", [])}


def desktop_entries():
    """{bin:programme: (nom, visible)} d'après les lanceurs des applications.

    Plusieurs lanceurs peuvent lancer le même programme (Mousepad et ses
    réglages, « mousepad --preferences ») : celui qui figure au menu passe
    avant un lanceur caché (NoDisplay, Hidden) ou secondaire, quel que soit
    l'ordre des fichiers (#2248). Spécification Desktop Entry de freedesktop.org."""
    found = {}
    for folder in (Path(ROOT + "/usr/share/applications"), Path(ROOT + "/usr/local/share/applications")):
        try:
            files = sorted(folder.glob("*.desktop"))
        except OSError:
            continue
        for path in files:
            values = {}
            for line in read_lines(path):
                if line.startswith("[") and line.strip() != "[Desktop Entry]":
                    break
                key, sep, value = line.partition("=")
                if sep and key.strip() not in values:
                    values[key.strip()] = value.strip()
            name, command = values.get("Name"), values.get("Exec", "")
            words = [w for w in command.split() if w != "env" and "=" not in w]
            if not name or not words or values.get("Type", "Application") != "Application":
                continue
            ident = "bin:" + os.path.basename(words[0])
            visible = values.get("NoDisplay", "").lower() != "true" and values.get("Hidden", "").lower() != "true"
            # Le lanceur principal : au menu, et sans option qui le spécialise.
            rank = (visible, not any(w.startswith("-") for w in words[1:]))
            if ident not in found or rank > found[ident][2]:
                found[ident] = (name, visible, rank)
    return {ident: (name, visible) for ident, (name, visible, _rank) in found.items()}


def desktop_names():
    """Nom affiché des applications Linux d'après leurs lanceurs (bin:firefox → Firefox)."""
    return {ident: name for ident, (name, _visible) in desktop_entries().items()}


def desktop_choices():
    """Applications proposées au blocage : celles du menu, sous leur nom."""
    return {ident: name for ident, (name, visible) in desktop_entries().items() if visible and not protected(ident)}


PROGRAM_DIRS = ("/usr/bin", "/usr/local/bin", "/bin", "/usr/games")


def typed_app(text, choices):
    """Identifiant de l'application tapée dans « Choisir une application », ou None.

    Un texte qui n'est ni une application de la liste (nom ou programme), ni un
    .exe, ni un programme installé ne bloque rien : le panneau le refuse au lieu
    d'enregistrer en silence un programme inexistant (« mouse » → bin:mouse, #2350)."""
    text = text.strip()
    if not text:
        return None
    folded = text.casefold()
    for ident, name in choices.items():
        program = ident.split(":", 1)[1]
        if folded in (name.casefold(), program.casefold(), f"{name} ({program})".casefold()):
            return ident
    if folded.endswith(".exe"):
        return "exe:" + re.split(r"[\\/]", text)[-1].lower()
    program = os.path.basename(text)
    if program and any(os.access(f"{ROOT}{folder}/{program}", os.X_OK) and Path(f"{ROOT}{folder}/{program}").is_file()
                       for folder in PROGRAM_DIRS):
        return "bin:" + program
    return None


def typed_suggestions(text, choices, limit=3):
    """Applications de la liste dont le nom ou le programme contient le texte tapé."""
    folded = text.strip().casefold()
    if not folded:
        return []
    found = [ident for ident, name in sorted(choices.items(), key=lambda item: item[1].casefold())
             if folded in name.casefold() or folded in ident.split(":", 1)[1].casefold()]
    return found[:limit]


def display_name(ident, names):
    if ident in names:
        return names[ident]
    kind, _, value = ident.partition(":")
    return value[:-4] if kind == "exe" and value.endswith(".exe") else value


# ── DNS filtrant ────────────────────────────────────────────────────────────

def under(name, domains):
    return any(name == domain or name.endswith("." + domain) for domain in domains)


def dns_decision(name, web):
    """block | forward | safe:CIBLE pour un nom demandé par le compte surveillé."""
    name = name.lower().rstrip(".")
    if under(name, DOH_HOSTS) or under(name, VPN_HOSTS):
        return "block"
    mode = web.get("mode", "off")
    if mode == "block" and under(name, web.get("blocked", [])):
        return "block"
    if mode == "allow" and not under(name, web.get("allowed", [])):
        return "block"
    if web.get("safesearch"):
        for pattern, target in SAFE_TARGETS:
            if pattern.match(name):
                return "safe:" + target
    return "forward"


def read_name(packet, offset):
    """Nom DNS à cette position (pointeurs suivis) et position après lui."""
    labels, end, jumps = [], None, 0
    while True:
        if offset >= len(packet):
            raise ValueError("nom tronqué")
        length = packet[offset]
        if length & 0xC0 == 0xC0:
            if offset + 1 >= len(packet) or jumps > 16:
                raise ValueError("pointeur invalide")
            if end is None:
                end = offset + 2
            offset = ((length & 0x3F) << 8) | packet[offset + 1]
            jumps += 1
            continue
        if length == 0:
            return ".".join(labels), (end if end is not None else offset + 1)
        labels.append(packet[offset + 1:offset + 1 + length].decode("ascii", "replace"))
        offset += 1 + length


def parse_query(packet):
    """(identifiant, nom, type, fin de la question) d'une requête DNS."""
    if len(packet) < 12:
        raise ValueError("requête trop courte")
    ident, _flags, qdcount = struct.unpack("!HHH", packet[:6])
    if qdcount != 1:
        raise ValueError("une seule question attendue")
    name, offset = read_name(packet, 12)
    if offset + 4 > len(packet):
        raise ValueError("question tronquée")
    qtype, = struct.unpack("!H", packet[offset:offset + 2])
    return ident, name, qtype, offset + 4


def reply_with(packet, end, rcode, answers=b"", count=0):
    flags = 0x8000 | (packet[2] & 0x01) << 8 | 0x0080 | rcode
    return packet[:2] + struct.pack("!HHHHH", flags, 1, count, 0, 0) + packet[12:end] + answers


def encode_name(name):
    out = b""
    for label in name.rstrip(".").split("."):
        raw = label.encode("ascii")
        out += bytes([len(raw)]) + raw
    return out + b"\0"


def build_query(name, qtype, ident=None):
    ident = ident if ident is not None else int.from_bytes(os.urandom(2), "big")
    return struct.pack("!HHHHHH", ident, 0x0100, 1, 0, 0, 0) + encode_name(name) + struct.pack("!HH", qtype, 1)


def answer_records(packet):
    """[(type, ttl, rdata)] des réponses A/AAAA d'un paquet DNS."""
    _ident, _flags, qd, an = struct.unpack("!HHHH", packet[:8])
    offset = 12
    for _ in range(qd):
        _name, offset = read_name(packet, offset)
        offset += 4
    out = []
    for _ in range(an):
        _name, offset = read_name(packet, offset)
        rtype, _rclass, ttl, length = struct.unpack("!HHIH", packet[offset:offset + 10])
        offset += 10
        data = packet[offset:offset + length]
        offset += length
        if rtype in (1, 28):
            out.append((rtype, ttl, data))
    return out


def upstreams():
    out = []
    for line in read_lines(RESOLV):
        parts = line.split()
        if len(parts) >= 2 and parts[0] == "nameserver":
            address = parts[1].split("%")[0]
            if not (address.startswith("127.") or address in ("::1", "0.0.0.0", "::")):
                out.append(address)
    return out


def ask_upstream(packet, tcp=False):
    port = int(os.environ.get("WINLUX_PARENTAL_UPSTREAM_PORT", "53"))
    for server in upstreams():
        family = socket.AF_INET6 if ":" in server else socket.AF_INET
        try:
            with socket.socket(family, socket.SOCK_STREAM if tcp else socket.SOCK_DGRAM) as sock:
                sock.settimeout(3)
                if tcp:
                    sock.connect((server, port))
                    sock.sendall(struct.pack("!H", len(packet)) + packet)
                    size = struct.unpack("!H", recv_exact(sock, 2))[0]
                    return recv_exact(sock, size)
                sock.sendto(packet, (server, port))
                return sock.recvfrom(65535)[0]
        except (OSError, struct.error, ValueError):
            continue
    return None


def recv_exact(sock, size):
    data = b""
    while len(data) < size:
        chunk = sock.recv(size - len(data))
        if not chunk:
            raise ValueError("connexion fermée")
        data += chunk
    return data


class DnsFilter:
    """Résolveur d'un compte surveillé : écoute 127.0.0.1 et ::1 sur son port."""

    def __init__(self, daemon, user, port):
        self.daemon, self.user, self.port = daemon, user, port
        self.sockets = []
        for family, host in ((socket.AF_INET, "127.0.0.1"), (socket.AF_INET6, "::1")):
            for kind in (socket.SOCK_DGRAM, socket.SOCK_STREAM):
                try:
                    sock = socket.socket(family, kind)
                    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                    sock.bind((host, port))
                    if kind == socket.SOCK_STREAM:
                        sock.listen(16)
                except OSError:
                    continue
                self.sockets.append(sock)
                target = self.serve_udp if kind == socket.SOCK_DGRAM else self.serve_tcp
                threading.Thread(target=target, args=(sock,), daemon=True).start()

    def answer(self, packet, tcp=False):
        try:
            _ident, name, qtype, end = parse_query(packet)
        except (ValueError, struct.error):
            return None
        web = self.daemon.web_of(self.user)
        decision = dns_decision(name, web)
        if decision == "block":
            if not under(name.lower(), DOH_HOSTS):
                self.daemon.site_blocked(self.user, name.lower().rstrip("."))
            return reply_with(packet, end, 3)
        if decision.startswith("safe:"):
            if qtype not in (1, 28):
                return reply_with(packet, end, 0)
            response = ask_upstream(build_query(decision[5:], qtype))
            try:
                records = answer_records(response) if response else []
            except (ValueError, struct.error):
                records = []
            records = [record for record in records if record[0] == qtype]
            self.opened(records)
            body = b"".join(b"\xc0\x0c" + struct.pack("!HHIH", rtype, 1, min(ttl, 300), len(data)) + data
                            for rtype, ttl, data in records)
            return reply_with(packet, end, 0, body, len(records))
        response = ask_upstream(packet, tcp) or (tcp and ask_upstream(packet))
        if not response:
            return reply_with(packet, end, 2)
        try:
            self.opened(answer_records(response))
        except (ValueError, struct.error, IndexError):
            pass
        return response

    def opened(self, records):
        """Ouvre au compte les adresses de la réponse avant qu'il ne la reçoive (#816)."""
        resolved = getattr(self.daemon, "resolved", None)
        if resolved and records:
            resolved(self.user, records)

    def serve_udp(self, sock):
        while True:
            try:
                packet, peer = sock.recvfrom(4096)
            except OSError:
                return
            threading.Thread(target=self.udp_reply, args=(sock, packet, peer), daemon=True).start()

    def udp_reply(self, sock, packet, peer):
        response = self.answer(packet)
        if response:
            try:
                sock.sendto(response, peer)
            except OSError:
                pass

    def serve_tcp(self, sock):
        while True:
            try:
                client, _ = sock.accept()
            except OSError:
                return
            threading.Thread(target=self.tcp_reply, args=(client,), daemon=True).start()

    def tcp_reply(self, client):
        with client:
            try:
                client.settimeout(10)
                size = struct.unpack("!H", recv_exact(client, 2))[0]
                response = self.answer(recv_exact(client, size), tcp=True)
                if response:
                    client.sendall(struct.pack("!H", len(response)) + response)
            except (OSError, ValueError, struct.error):
                pass


def dns_port(uid):
    return DNS_BASE + uid - 1000 if 1000 <= uid < 2000 else None


def nft_ruleset(filtered):
    """Règles nftables pour {uid: port} : DNS redirigé, DNS chiffré refusé et,
    hors du réseau local, seules les adresses données par le DNS filtrant (#816)."""
    if not filtered:
        return ""
    lines = [f"table inet {TABLE} {{"]
    for uid in sorted(filtered):
        lines += [f"  set ok4_{uid} {{ type ipv4_addr; flags timeout; }}",
                  f"  set ok6_{uid} {{ type ipv6_addr; flags timeout; }}"]
    lines += ["  chain dns {", "    type nat hook output priority -100; policy accept;"]
    for uid, port in sorted(filtered.items()):
        lines += [f"    meta skuid {uid} udp dport 53 redirect to :{port}",
                  f"    meta skuid {uid} tcp dport 53 redirect to :{port}"]
    lines += ["  }", "  chain out {", "    type filter hook output priority 0; policy accept;"]
    v4, v6 = ", ".join(DOH_IPV4), ", ".join(DOH_IPV6)
    for uid in sorted(filtered):
        lines += [f"    meta skuid {uid} tcp dport 853 reject with tcp reset",
                  f"    meta skuid {uid} udp dport 853 reject",
                  f"    meta skuid {uid} ip daddr {{ {v4} }} tcp dport 443 reject with tcp reset",
                  f"    meta skuid {uid} ip daddr {{ {v4} }} udp dport 443 reject",
                  f"    meta skuid {uid} ip6 daddr {{ {v6} }} tcp dport 443 reject with tcp reset",
                  f"    meta skuid {uid} ip6 daddr {{ {v6} }} udp dport 443 reject"]
    local4, local6 = ", ".join(LOCAL_IPV4), ", ".join(LOCAL_IPV6)
    for uid in sorted(filtered):
        # VPN, proxy, DoH inconnu ou adresse écrite en dur : rien n'est joignable
        # que le DNS filtrant n'ait donné (#816). Le réseau local reste ouvert.
        lines += [f"    meta skuid {uid} ct state established,related accept",
                  f"    meta skuid {uid} oifname \"lo\" accept",
                  f"    meta skuid {uid} ip daddr {{ {local4} }} accept",
                  f"    meta skuid {uid} ip6 daddr {{ {local6} }} accept",
                  f"    meta skuid {uid} ip daddr @ok4_{uid} accept",
                  f"    meta skuid {uid} ip6 daddr @ok6_{uid} accept",
                  f"    meta skuid {uid} meta l4proto tcp reject with tcp reset",
                  f"    meta skuid {uid} reject"]
    lines += ["  }", "}"]
    return "\n".join(lines) + "\n"


def address_script(uid, addresses):
    """Commandes nft qui ouvrent ces adresses {adresse: secondes} au compte."""
    out = []
    for family, prefix in ((socket.AF_INET, "ok4"), (socket.AF_INET6, "ok6")):
        items = [f"{address} timeout {max(60, int(seconds))}s" for address, seconds in sorted(addresses.items())
                 if (":" in address) == (family == socket.AF_INET6)]
        if items:
            out.append(f"add element inet {TABLE} {prefix}_{uid} {{ {', '.join(items)} }}")
    return "\n".join(out) + ("\n" if out else "")


def run_nft(script):
    try:
        result = subprocess.run([NFT, "-f", "-"], input=script, capture_output=True, text=True, timeout=20)
    except (OSError, subprocess.SubprocessError) as error:
        return str(error)
    return result.stderr.strip() if result.returncode else ""


def apply_nft(rules):
    script = f"table inet {TABLE}\ndelete table inet {TABLE}\n" + rules
    try:
        result = subprocess.run([NFT, "-f", "-"], input=script, capture_output=True, text=True, timeout=20)
    except (OSError, subprocess.SubprocessError) as error:
        return str(error)
    return result.stderr.strip() if result.returncode else ""


# ── Sessions ────────────────────────────────────────────────────────────────

def session_displays(user):
    """[(écran, vt)] des sessions ouvertes de ce compte (registre de winlux-sessions)."""
    path = SESSIONS / user
    values = {}
    for line in read_lines(path):
        key, sep, value = line.partition("=")
        if sep:
            values[key] = value
    display = values.get("display", "")
    if not re.fullmatch(r":[0-9]{1,2}", display):
        return []
    return [(display, values.get("vt", ""))]


def active_vt():
    if os.environ.get("WINLUX_TEST_VT"):
        return os.environ["WINLUX_TEST_VT"]
    try:
        return Path("/sys/class/tty/tty0/active").read_text().strip().removeprefix("tty")
    except OSError:
        return ""


def foreground_pid(display):
    """PID de la fenêtre active de cet écran (xprop), None sinon."""
    xprop = shutil.which("xprop")
    if not xprop:
        return None
    env = {"PATH": "/usr/bin:/bin", "DISPLAY": display}
    try:
        out = subprocess.run([xprop, "-root", "_NET_ACTIVE_WINDOW"], capture_output=True, text=True, timeout=3,
                             env=env).stdout
        match = re.search(r"window id # (0x[0-9a-f]+)", out)
        if not match or int(match.group(1), 16) == 0:
            return None
        out = subprocess.run([xprop, "-id", match.group(1), "_NET_WM_PID"], capture_output=True, text=True,
                             timeout=3, env=env).stdout
        match = re.search(r"= ([0-9]+)", out)
        return int(match.group(1)) if match else None
    except (OSError, subprocess.SubprocessError):
        return None


def user_session_env(uid, display):
    """Environnement de session (DISPLAY, bus) d'un processus du compte sur cet écran."""
    for pid in pids_of({uid}):
        env = proc_env(pid)
        if env.get("DISPLAY") == display and env.get("DBUS_SESSION_BUS_ADDRESS"):
            return env
    return None


def gvariant_string(text):
    """Chaîne au format texte de GVariant, telle que gdbus call la lit."""
    escaped = str(text).replace("\\", "\\\\").replace("'", "\\'").replace("\n", "\\n").replace("\r", "\\r") \
        .replace("\t", "\\t")
    return "'" + escaped + "'"


def notify_command(title, body):
    """Commande qui affiche la bulle sur le bus de session courant.

    org.freedesktop.Notifications.Notify (winlux-notifications) par gdbus, de
    GLib, toujours dans l'image ; notify-send n'y est pas (#630, #2245) et ne
    reste qu'un repli."""
    gdbus = shutil.which("gdbus")
    if gdbus:
        return [gdbus, "call", "--session", "--timeout", "5", "--dest", "org.freedesktop.Notifications",
                "--object-path", "/org/freedesktop/Notifications", "--method", "org.freedesktop.Notifications.Notify", "--",
                gvariant_string(_t("Contrôle parental")), "0", gvariant_string("dialog-warning"),
                gvariant_string(title), gvariant_string(body), "@as []", "@a{sv} {}", "-1"]
    tool = shutil.which("notify-send")
    if tool:
        return [tool, "-a", _t("Contrôle parental"), "-i", "dialog-warning", title, body]
    return None


def notify(user, display, title, body):
    """Bulle dans la session du compte, sous son identité, sur son propre bus."""
    entry = passwd().get(user)
    if not entry or ROOT:
        return False
    env = user_session_env(entry[0], display)
    command = notify_command(title, body)
    if env is None or command is None:
        return False
    child_env = {"PATH": "/usr/bin:/bin", "HOME": entry[3], "DISPLAY": display,
                 "DBUS_SESSION_BUS_ADDRESS": env["DBUS_SESSION_BUS_ADDRESS"], "LANG": env.get("LANG", "C.UTF-8")}
    try:
        subprocess.Popen(["setpriv", f"--reuid={entry[0]}", f"--regid={entry[1]}", "--init-groups", *command],
                         env=child_env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, start_new_session=True)
    except OSError:
        return False
    return True


def lock_running(uid, display):
    """L'écran de verrouillage du compte couvre déjà cet écran."""
    for pid in pids_of({uid}):
        _exe, cmdline = proc_info(pid)
        if any(arg.endswith("winlux-lock") for arg in cmdline[:2]) and \
                not any(arg in ("daemon", "status") for arg in cmdline[1:3]):
            if proc_env(pid).get("DISPLAY") == display:
                return True
    return False


def signal_all(uid, sig):
    for _ in range(3):
        pids = pids_of({uid})
        for pid in pids:
            try:
                os.kill(pid, sig)
            except OSError:
                pass
        if sig != signal.SIGSTOP:
            return


# ── Service root ────────────────────────────────────────────────────────────

class Daemon:
    def __init__(self):
        self.configs = {}
        self.filters = {}
        self.rules = None
        self.screens = {}
        self.frozen = set()
        self.warned = set()
        self.killed = set()
        self.names = {}
        self.names_at = None
        self.site_notes = {}
        self.lock = threading.Lock()
        self.filtered = {}
        self.addresses = {}
        self.address_lock = threading.Lock()
        self.prints = {}
        self.samples = {}
        self.seen = {}
        self.orphans = []
        self.orphans_at = 0

    def log(self, text):
        print(time.strftime("%H:%M:%S"), text, flush=True)

    def web_of(self, user):
        with self.lock:
            cfg = self.configs.get(user)
        return cfg["web"] if cfg else {"mode": "off"}

    def site_blocked(self, user, site):
        update_usage(user, lambda u: u["blocked_sites"].__setitem__(site, u["blocked_sites"].get(site, 0) + 1))
        last = self.site_notes.get((user, site), 0)
        if time.monotonic() - last > 60:
            self.site_notes[(user, site)] = time.monotonic()
            for display, _vt in session_displays(user):
                notify(user, display, _t("Contrôle parental"), _t("Le site {name} est bloqué par le contrôle parental.").format(name=site))

    def resolved(self, user, records):
        """Adresses A/AAAA données par le DNS filtrant : joignables par le compte."""
        uid = uid_of(user)
        if uid not in self.filtered:
            return
        now = time.monotonic()
        todo = {}
        with self.address_lock:
            known = self.addresses.setdefault(uid, {})
            for rtype, _ttl, data in records:
                try:
                    address = socket.inet_ntop(socket.AF_INET if rtype == 1 else socket.AF_INET6, data)
                except (OSError, ValueError):
                    continue
                if known.get(address, 0) - now > ADDRESS_REFRESH:
                    continue
                known[address] = now + ADDRESS_TIMEOUT
                todo[address] = ADDRESS_TIMEOUT
            if todo:
                error = run_nft(address_script(uid, todo))
                if error:
                    self.log(f"nftables : {error}")

    def reload(self):
        configs = supervised_accounts()
        with self.lock:
            self.configs = configs
        filtered = {}
        for user, cfg in configs.items():
            uid = uid_of(user)
            port = dns_port(uid) if uid is not None else None
            if port is None:
                continue
            if cfg["web"]["mode"] != "off" or cfg["web"]["safesearch"]:
                if user not in self.filters:
                    self.filters[user] = DnsFilter(self, user, port)
                filtered[uid] = port
        self.filtered = filtered
        rules = nft_ruleset(filtered)
        now = time.monotonic()
        with self.address_lock:
            for uid in list(self.addresses):
                kept = {a: t for a, t in self.addresses[uid].items() if t > now and uid in filtered}
                if kept:
                    self.addresses[uid] = kept
                else:
                    del self.addresses[uid]
            if rules != self.rules:
                error = apply_nft(rules)
                if error:
                    self.log(f"nftables : {error}")
                else:
                    self.rules = rules
                    # Table recréée : les adresses déjà données restent joignables.
                    script = "".join(address_script(uid, {a: t - now for a, t in known.items()})
                                     for uid, known in self.addresses.items())
                    if script:
                        run_nft(script)
        for user, cfg in configs.items():
            self.learn_installed(user, cfg)
        for user in list(self.screens):
            if user[0] not in configs:
                self.close_screen(user)
        for uid in list(self.frozen):
            if uid not in {uid_of(u) for u in configs}:
                self.thaw(uid)

    def app_names(self):
        # Premier appel : charger tout de suite. time.monotonic() compte depuis le
        # démarrage : avec 0 pour départ, le cache restait vide 10 minutes (#2387).
        if self.names_at is None or time.monotonic() - self.names_at > 600:
            self.names, self.names_at = desktop_names(), time.monotonic()
        return self.names

    def enforce_apps(self):
        """Ferme aussitôt les applications bloquées des comptes surveillés."""
        with self.lock:
            configs = dict(self.configs)
        by_uid = {}
        for user, cfg in configs.items():
            uid = uid_of(user)
            if uid is not None and cfg["apps"]:
                by_uid[uid] = (user, blocked_ids(cfg))
        if not by_uid:
            return
        alive = set()
        for pid, uid in pids_of(set(by_uid)).items():
            if uid in self.frozen:
                continue
            exe, cmdline = proc_info(pid)
            ident = app_of(exe, cmdline)
            user, blocked = by_uid[uid]
            match = self.identify(user, pid, exe, cmdline, ident, blocked)
            if match:
                try:
                    os.kill(pid, signal.SIGKILL)
                except OSError:
                    continue
                if match != ident:
                    self.log(f"{user} : copie de {match} lancée sous le nom {ident}, fermée")
                ident = match
                key = (uid, ident)
                alive.add(key)
                if key not in self.killed:
                    self.killed.add(key)
                    name = shown_name(ident, blocked[ident], self.app_names())
                    update_usage(user, lambda u: (u["blocked_apps"].__setitem__(ident, u["blocked_apps"].get(ident, 0) + 1),
                                                  u["names"].__setitem__(ident, name)))
                    for display, _vt in session_displays(user):
                        if not notify(user, display, _t("Contrôle parental"),
                                      _t("« {name} » est bloquée par le contrôle parental.").format(name=name)):
                            self.log(f"{user} : bulle « application bloquée » impossible sur {display}")
        # Une nouvelle tentative, plus tard, compte à nouveau.
        self.killed &= alive | {key for key in self.killed if key[0] not in by_uid}
        live = set(pids_of(set(by_uid))) if self.seen else set()
        for pid in [pid for pid in self.seen if pid not in live]:
            del self.seen[pid]

    def prints_of(self, user):
        """{empreinte: [identifiants]} des programmes vus pour ce compte (root, 0600)."""
        if user not in self.prints:
            data = read_json(DATA_DIR / user / "fingerprints.json") or {}
            self.prints[user] = {k: list(v) for k, v in data.items()
                                 if isinstance(v, list) and all(isinstance(i, str) for i in v)}
        return self.prints[user]

    def remember(self, user, mark, ident):
        prints = self.prints_of(user)
        idents = prints.setdefault(mark, [])
        if ident in idents:
            return
        idents.append(ident)
        while len(prints) > PRINTS_KEEP:
            del prints[next(iter(prints))]
        try:
            write_private(DATA_DIR / user / "fingerprints.json", prints)
        except OSError as error:
            self.log(f"empreintes de {user} : {error}")

    def sample(self, path):
        """(empreinte, nom d'origine du .exe) d'un fichier, mis en cache par inode."""
        try:
            st = os.stat(path)
        except OSError:
            return None
        key = (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns)
        if key not in self.samples:
            got = file_sample(path)
            if got is None:
                return None
            real_key, head, tail = got
            if len(self.samples) > 4096:
                self.samples.clear()
            self.samples[key] = (fingerprint(real_key, head, tail), pe_original_name(head) or pe_original_name(tail))
        return self.samples[key]

    def identify(self, user, pid, exe, cmdline, ident, blocked):
        """Application bloquée que ce processus fait tourner, même renommée (#815)."""
        if ident and protected(ident) and ident.startswith("exe:") and shell_process(pid, exe, cmdline):
            # Le vrai bureau Winlux n'est jamais fermé, même si une copie
            # bloquée (renommée) partage son empreinte (#2293).
            return None
        cache_key = (exe, tuple(cmdline))
        cached = self.seen.get(pid)
        fresh = not cached or cached[0] != cache_key
        if fresh:
            path = app_file(pid, exe, cmdline) if ident else None
            got = self.sample(path) if path else None
            cached = (cache_key,) + (got or (None, None))
            self.seen[pid] = cached
        _key, mark, original = cached
        found = ident if ident in blocked else None
        if mark:
            if found is None:
                found = next((known for known in self.prints_of(user).get(mark, []) if known in blocked), None)
            if found is None and original and ident and ident.startswith("exe:") and "exe:" + original in blocked:
                found = "exe:" + original
            if fresh and ident and not protected(ident):
                self.remember(user, mark, found or ident)
                self.remember(user, mark, ident)
        return found

    def learn_installed(self, user, cfg):
        """Empreinte des applications Linux bloquées, telles qu'installées : une
        copie est reconnue dès son premier lancement."""
        for app in cfg.get("apps", []):
            kind, _, name = app["id"].partition(":")
            if kind != "bin" or protected(app["id"]):
                continue
            path = shutil.which(name, path=ROOT + "/usr/local/bin:" + ROOT + "/usr/bin:" + ROOT + "/usr/games:" + ROOT + "/bin")
            got = self.sample(path) if path else None
            if got:
                self.remember(user, got[0], app["id"])

    def sessions(self):
        """[(compte, uid, écran, active)] des sessions des comptes surveillés."""
        out = []
        current = active_vt()
        with self.lock:
            users = list(self.configs)
        for user in users:
            uid = uid_of(user)
            if uid is None:
                continue
            for display, vt in session_displays(user):
                # Le registre doit correspondre à un vrai bureau du compte sur cet écran.
                if not any(proc_env(pid).get("DISPLAY") == display for pid in pids_of({uid})):
                    continue
                out.append((user, uid, display, not vt or not current or vt == current))
        return out

    def enforce_time(self):
        moment = now_time()
        for user, uid, display, _active in self.sessions():
            state = state_of(user, moment)
            key = (user, display)
            screen = self.screens.get(key)
            if screen is not None and screen["proc"].poll() is not None:
                code = screen["proc"].returncode
                del self.screens[key]
                if code == 0:
                    self.thaw(uid)
                    screen = None
                else:
                    # Écran fermé de force (xkill, plantage) : il revient, et
                    # au deuxième coup les programmes du compte sont suspendus.
                    self.log(f"écran de {user} fermé (code {code})")
                    tamper = screen.get("tamper", 0) + 1
                    if tamper >= TAMPER_FREEZE:
                        self.freeze(uid)
                    self.open_screen(user, display, tamper)
                    continue
            if state["allowed"]:
                remaining = state.get("remaining")
                for threshold in WARN_AT:
                    mark = (user, moment.date(), threshold, state.get("reason"), state.get("grant_until"))
                    if remaining is not None and remaining <= threshold and mark not in self.warned:
                        self.warned.add(mark)
                        title = _t("Temps d’écran : il reste 1 minute") if remaining <= 1 else \
                            _t("Temps d’écran : il reste {minutes} minutes").format(minutes=remaining)
                        if not notify(user, display, title,
                                      _t("Enregistrez votre travail : l’ordinateur sera bientôt bloqué.")):
                            self.log(f"{user} : bulle « {title} » impossible sur {display}")
                        break
                continue
            if screen is None and not lock_running(uid, display):
                self.open_screen(user, display, 0)

    def open_screen(self, user, display, tamper):
        env = {"PATH": "/usr/sbin:/usr/bin:/sbin:/bin", "HOME": "/root", "DISPLAY": display,
               "LANG": os.environ.get("LANG", "C.UTF-8")}
        for key in ("WINLUX_PARENTAL_ROOT", "WINLUX_ROOT_SOCKET", "WINLUX_LANG"):
            if os.environ.get(key):
                env[key] = os.environ[key]
        try:
            proc = subprocess.Popen([sys.executable, "-B", SELF, "screen", user, display], env=env,
                                    stdin=subprocess.DEVNULL, start_new_session=True)
        except OSError as error:
            self.log(f"écran de blocage : {error}")
            return
        self.screens[(user, display)] = {"proc": proc, "tamper": tamper}

    def close_screen(self, key):
        screen = self.screens.pop(key, None)
        if screen:
            try:
                screen["proc"].terminate()
            except OSError:
                pass

    def freeze(self, uid):
        if uid not in self.frozen:
            self.frozen.add(uid)
            self.save_frozen()
            signal_all(uid, signal.SIGSTOP)

    def thaw(self, uid):
        if uid in self.frozen:
            self.frozen.discard(uid)
            signal_all(uid, signal.SIGCONT)
            self.save_frozen()

    def close_orphans(self, force=False):
        """Ferme les écrans de l'instance précédente, une fois le délai passé."""
        if not self.orphans or (not force and time.monotonic() < self.orphans_at):
            return
        for pid in self.orphans:
            try:
                os.kill(pid, signal.SIGKILL)
                self.log(f"écran de blocage laissé par le service précédent fermé (pid {pid})")
            except OSError:
                pass
        self.orphans = []

    def save_frozen(self):
        """Comptes gelés, écrits avant le gel : un service relancé les retrouve."""
        try:
            write_private(FROZEN_FILE, sorted(self.frozen))
        except OSError as error:
            self.log(f"comptes gelés : {error}")

    def recover(self):
        """Au démarrage, reprend l'état laissé par une instance précédente
        (arrêtée, plantée) : ses écrans de blocage sont fermés et les comptes
        qu'elle avait gelés sont dégelés. Les règles s'appliquent ensuite comme
        d'habitude : l'écran revient si le temps n'est pas permis (#2246).

        Les écrans laissés ne sont fermés qu'après ORPHAN_GRACE secondes : le
        nouveau service a d'abord rouvert les siens, l'écran de l'enfant n'est
        jamais découvert pendant la reprise."""
        self.orphans = stale_screens()
        self.orphans_at = time.monotonic() + ORPHAN_GRACE
        try:
            uids = [uid for uid in json.loads(FROZEN_FILE.read_text(encoding="utf-8")) if isinstance(uid, int)]
        except (OSError, ValueError, TypeError):
            uids = []
        for uid in uids:
            if uid >= 1000:
                signal_all(uid, signal.SIGCONT)
                self.log(f"compte {uid} gelé par le service précédent : dégelé")
        self.frozen = set()
        if uids:
            self.save_frozen()

    def count_minute(self):
        """Une minute d'écran pour chaque session surveillée active et déverrouillée."""
        moment = now_time()
        counted = set()
        for user, uid, display, active in self.sessions():
            if not active or user in counted or (user, display) in self.screens or lock_running(uid, display):
                continue
            counted.add(user)
            pid = foreground_pid(display)
            ident = None
            if pid is not None and proc_uid(pid) == uid:
                ident = app_of(*proc_info(pid))
                if ident and protected(ident):
                    ident = None
            names = self.app_names()

            def change(usage, ident=ident):
                usage["minutes"] += 1
                if ident:
                    usage["apps"][ident] = usage["apps"].get(ident, 0) + 1
                    usage["names"].setdefault(ident, display_name(ident, names))
            update_usage(user, change, moment)

    def run(self):
        if os.geteuid() != 0 and not ROOT:
            print("winlux-parental daemon doit tourner en root", file=sys.stderr)
            return 1
        # Un seul service : nom abstrait réservé (lancé deux fois au démarrage
        # si le premier essai des services système a échoué).
        self.singleton = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            self.singleton.bind("\0winlux-parental" + ROOT.replace("/", "-"))
        except OSError:
            return 0
        for directory in (CONF_DIR, DATA_DIR):
            private_dir(directory)
        self.recover()
        signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
        last_reload = last_time = 0
        next_minute = time.monotonic() + 60
        pruned = None
        while True:
            now = time.monotonic()
            try:
                if now - last_reload >= 10:
                    self.reload()
                    last_reload = now
                self.enforce_apps()
                if now - last_time >= 3:
                    self.enforce_time()
                    last_time = now
                self.close_orphans()
                if now >= next_minute:
                    next_minute += 60
                    if next_minute < now:
                        next_minute = now + 60
                    self.count_minute()
                    today = datetime.date.today()
                    if pruned != today:
                        for user in self.configs:
                            prune(user)
                        pruned = today
            except Exception as error:  # le service ne doit jamais s'arrêter
                self.log(f"erreur : {error!r}")
            time.sleep(1)


# ── Supervision ─────────────────────────────────────────────────────────────

SUPERVISE_DELAYS = (1, 2, 5, 10, 30, 60)


def supervise(command, log, rounds=None, sleep=time.sleep, stable=60):
    """Lance le service et le relance après chaque arrêt (#2247) : sans systemd,
    rien d'autre ne le ferait, et un contrôle parental arrêté laisse tout passer.
    Délai croissant si le service s'arrête aussitôt (au plus une minute), remis à
    zéro après une minute de marche. Le verrou d'instance unique du service
    évite les doublons : une relance pendant qu'un autre tourne s'arrête seule."""
    child = {"proc": None}

    def stop(*_):
        proc = child["proc"]
        if proc is not None and proc.poll() is None:
            proc.terminate()
        sys.exit(0)

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    failures = 0
    done = 0
    while rounds is None or done < rounds:
        started = time.monotonic()
        try:
            child["proc"] = subprocess.Popen(command, stdin=subprocess.DEVNULL)
            code = child["proc"].wait()
        except OSError as error:
            code = f"impossible de lancer : {error}"
        done += 1
        failures = 0 if time.monotonic() - started >= stable else failures + 1
        delay = SUPERVISE_DELAYS[min(failures, len(SUPERVISE_DELAYS) - 1)]
        log(f"service arrêté (code {code}), relancé dans {delay} s")
        sleep(delay)
    return 0


# ── Commandes ───────────────────────────────────────────────────────────────

def emit(**values):
    print(json.dumps(values, ensure_ascii=False))
    return 0 if values.get("ok", True) else 1


def fail(error):
    return emit(ok=False, error=error)


def peer():
    """Compte qui appelle : le relais root le nomme (WINLUX_PEER_USER)."""
    if os.geteuid() == 0 or ROOT:
        name = os.environ.get("WINLUX_PEER_USER")
        if name:
            return name
    try:
        return pwd.getpwuid(os.getuid()).pw_name
    except KeyError:
        return ""


def require_root():
    if os.geteuid() != 0 and not ROOT:
        raise PermissionError("droits administrateur requis")


def supervisable(user):
    accounts = {account["name"]: account for account in desktop_accounts()}
    if user not in accounts:
        raise ValueError(f"compte inconnu : {user}")
    if accounts[user]["admin"]:
        raise ValueError("un compte administrateur ne peut pas être surveillé")


def command_list():
    require_root()
    moment = now_time()
    accounts = []
    for account in desktop_accounts():
        cfg = load_config(account["name"])
        entry = dict(account, configured=cfg is not None, enabled=bool(cfg and cfg["enabled"]))
        entry["today"] = state_of(account["name"], moment) if cfg else None
        if cfg:
            entry["today"]["used"] = load_usage(account["name"], moment.date())["minutes"]
        accounts.append(entry)
    return emit(ok=True, accounts=accounts, me=peer())


def main(argv):
    args = argv[1:]
    if not args:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    command, rest = args[0], args[1:]
    try:
        if command == "daemon":
            return Daemon().run()
        if command == "supervise" and not rest:
            if os.geteuid() != 0 and not ROOT:
                print("winlux-parental supervise doit tourner en root", file=sys.stderr)
                return 1

            def log(text):
                print(time.strftime("%Y-%m-%d %H:%M:%S"), "supervision :", text, flush=True)
            return supervise([sys.executable, "-B", SELF, "daemon"], log)
        if command == "status" and not rest:
            user = peer()
            state = state_of(user)
            # Le compte surveillé voit son temps, jamais ses réglages.
            return emit(ok=True, user=user, **{key: state.get(key) for key in
                                               ("supervised", "allowed", "reason", "remaining", "until", "until_day")})
        if command == "list" and not rest:
            return command_list()
        if command == "screen" and len(rest) == 2:
            require_root()
            return screen_main(rest[0], rest[1])
        if not rest or not NAME.match(rest[0]):
            print(__doc__.strip(), file=sys.stderr)
            return 2
        user = rest[0]
        if command == "check-login" and len(rest) == 1:
            require_root()
            state = state_of(user)
            print(json.dumps({key: state.get(key) for key in ("allowed", "reason", "until", "until_day")}))
            return 0 if state["allowed"] else 1
        require_root()
        if command == "get" and len(rest) == 1:
            cfg = load_config(user)
            return emit(ok=True, user=user, configured=cfg is not None, admin=is_admin(user),
                        config=cfg or dict(default_config(), enabled=False))
        if command == "set" and len(rest) == 1:
            supervisable(user)
            cfg = validate(json.loads(sys.stdin.read(1 << 20) or "{}"))
            write_private(config_path(user), cfg)
            return emit(ok=True, user=user, config=cfg)
        if command in ("enable", "disable") and len(rest) == 1:
            supervisable(user)
            cfg = load_config(user) or default_config()
            cfg["enabled"] = command == "enable"
            write_private(config_path(user), cfg)
            return emit(ok=True, user=user, config=cfg)
        if command == "grant" and len(rest) == 2 and rest[1].isdigit() and 1 <= int(rest[1]) <= 720:
            if load_config(user) is None:
                raise ValueError("compte non surveillé")
            until = grant(user, int(rest[1]), peer())
            return emit(ok=True, user=user, until=until)
        if command == "report" and len(rest) in (1, 2):
            days = int(rest[1]) if len(rest) == 2 and rest[1].isdigit() else 7
            return emit(ok=True, user=user, **report(user, max(1, min(days, KEEP_DAYS))))
        if command == "forget" and len(rest) == 1:
            for path in (config_path(user),):
                try:
                    path.unlink()
                except FileNotFoundError:
                    pass
            shutil.rmtree(DATA_DIR / user, ignore_errors=True)
            return emit(ok=True, user=user)
    except PermissionError as error:
        return fail(str(error))
    except ValueError as error:
        return fail(str(error))
    print(__doc__.strip(), file=sys.stderr)
    return 2


# ── Écran de blocage (root, sur l'écran du compte) ──────────────────────────

def relay(payload, timeout=60):
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(timeout)
            client.connect(RELAY_SOCKET)
            client.sendall((json.dumps(payload) + "\n").encode())
            answer = client.makefile("r", encoding="utf-8").readline()
        return json.loads(answer or "{}")
    except (OSError, ValueError):
        return {}


def password_checked(user, password):
    return bool(relay({"op": "check", "user": user, "password": password}).get("ok"))


SCREEN_CSS = b"""
window.parental { background: linear-gradient(160deg, #1d1438, #0d0a1c); }
window.parental label { color: #f4f1ff; }
window.parental .title { font-size: 26pt; font-weight: 300; }
window.parental .detail { font-size: 13pt; color: #cfc6f5; }
window.parental .error { color: #ff99a4; }
window.parental button { background: rgba(255,255,255,0.10); color: #ffffff; border: 1px solid rgba(255,255,255,0.18);
                         border-radius: 6px; padding: 8px 18px; box-shadow: none; text-shadow: none; }
window.parental button:hover { background: rgba(255,255,255,0.18); }
window.parental button.suggested-action { background: #6a3df0; border-color: #b69cff; }
window.parental entry { min-width: 280px; padding: 6px; }
"""


def screen_main(user, display):
    # L'écran tourne en root par-dessus une session qu'il peut geler (SIGSTOP,
    # deuxième fermeture forcée) : il ne doit dépendre d'aucun service de cette
    # session. Sans cela, GTK suit la propriété AT_SPI_BUS de la fenêtre racine
    # jusqu'au bus d'accessibilité du compte, gelé, et attend sa réponse sans
    # fin avant de créer la moindre fenêtre (#2244). NO_AT_BRIDGE=1 est la
    # variable d'at-spi2-atk qui coupe ce pont ; posée avant de charger GTK.
    os.environ["NO_AT_BRIDGE"] = "1"
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GLib, Gtk
    os.environ["DISPLAY"] = display
    accounts = {account["name"]: account for account in desktop_accounts()}
    me = accounts.get(user, {"name": user, "fullname": "", "uid": uid_of(user)})
    shown_name = me.get("fullname") or user
    provider = Gtk.CssProvider()
    provider.load_from_data(SCREEN_CSS)
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), provider,
                                             Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
    window = Gtk.Window(type=Gtk.WindowType.POPUP)
    window.get_style_context().add_class("parental")
    window.set_name("parental-screen")
    root = Gdk.get_default_root_window()
    window.move(0, 0)
    window.set_default_size(root.get_width(), root.get_height())
    window.connect("delete-event", lambda *_: True)
    outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, halign=Gtk.Align.CENTER, valign=Gtk.Align.CENTER, spacing=14)
    window.add(outer)
    title = Gtk.Label(wrap=True, justify=Gtk.Justification.CENTER)
    title.get_style_context().add_class("title")
    detail = Gtk.Label(wrap=True, justify=Gtk.Justification.CENTER, max_width_chars=60)
    detail.get_style_context().add_class("detail")
    error = Gtk.Label()
    error.get_style_context().add_class("error")
    form = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, halign=Gtk.Align.CENTER)
    buttons = Gtk.Box(spacing=10, halign=Gtk.Align.CENTER)
    for widget in (title, detail, form, error, buttons):
        outer.pack_start(widget, False, False, 0)
    state = {"mode": None, "grab_failures": 0}

    def clear(box):
        for child in box.get_children():
            child.destroy()

    def button(label, callback, suggested=False, parent=buttons):
        widget = Gtk.Button(label=label)
        if suggested:
            widget.get_style_context().add_class("suggested-action")
        widget.connect("clicked", lambda *_: callback())
        parent.pack_start(widget, False, False, 0)
        return widget

    def password_entry(placeholder, activate):
        entry = Gtk.Entry(visibility=False, placeholder_text=placeholder, halign=Gtk.Align.CENTER)
        entry.connect("activate", lambda *_: activate())
        form.pack_start(entry, False, False, 0)
        return entry

    def account_combo(names):
        combo = Gtk.ComboBoxText(halign=Gtk.Align.CENTER)
        for name in names:
            account = accounts.get(name, {})
            combo.append(name, account.get("fullname") or name)
        combo.set_active(0)
        form.pack_start(combo, False, False, 0)
        return combo

    def blocked_view():
        current = state_of(user)
        if current["allowed"]:
            return unlock_view()
        state["mode"] = "blocked"
        clear(form); clear(buttons); error.set_text("")
        title.set_text(_t("Temps d’écran terminé") if current["reason"] == "limit"
                       else _t("Ce n’est pas le moment d’utiliser l’ordinateur"))
        text = _t("{name} pourra de nouveau utiliser l’ordinateur aujourd’hui à {time}.") if current.get("until_day") == "today" \
            else _t("{name} pourra de nouveau utiliser l’ordinateur demain à {time}.")
        detail.set_text(text.format(name=shown_name, time=current.get("until") or "00:00") + "\n" +
                        _t("Les applications restent ouvertes derrière cet écran."))
        if any(account["admin"] for account in accounts.values()):
            button(_t("Un adulte accorde du temps"), grant_view, suggested=True)
        if len([name for name in accounts if name != user]) > 0:
            button(_t("Changer d’utilisateur"), switch_view)
        window.show_all()

    def unlock_view():
        state["mode"] = "unlock"
        clear(form); clear(buttons); error.set_text("")
        title.set_text(_t("Vous pouvez de nouveau utiliser l’ordinateur"))
        detail.set_text("")
        entry = password_entry(_t("Mot de passe de {name}").format(name=shown_name), lambda: unlock(entry))
        button(_t("Reprendre"), lambda: unlock(entry), suggested=True)
        window.show_all()
        entry.grab_focus()

    def unlock(entry):
        if password_checked(user, entry.get_text()):
            finish()
        else:
            entry.set_text("")
            error.set_text(_t("Le mot de passe est incorrect."))

    def grant_view():
        state["mode"] = "grant"
        clear(form); clear(buttons); error.set_text("")
        admins = [name for name, account in accounts.items() if account["admin"]]
        combo = account_combo(admins)
        minutes = Gtk.ComboBoxText(halign=Gtk.Align.CENTER)
        for value in (15, 30, 60, 120):
            minutes.append(str(value), "+ " + duration(value))
        minutes.set_active(1)
        form.pack_start(minutes, False, False, 0)
        entry = password_entry(_t("Mot de passe"), lambda: do_grant(combo, minutes, entry))
        button(_t("Accorder"), lambda: do_grant(combo, minutes, entry), suggested=True)
        button(_t("Annuler"), blocked_view)
        window.show_all()
        entry.grab_focus()

    def do_grant(combo, minutes, entry):
        admin = combo.get_active_id()
        if admin and is_admin(admin) and password_checked(admin, entry.get_text()):
            grant(user, int(minutes.get_active_id()), admin)
            finish()
        else:
            entry.set_text("")
            error.set_text(_t("Le mot de passe est incorrect."))

    def switch_view():
        state["mode"] = "switch"
        clear(form); clear(buttons); error.set_text("")
        combo = account_combo([name for name in accounts if name != user])
        entry = password_entry(_t("Mot de passe"), lambda: do_switch(combo, entry))
        button(_t("Ouvrir la session"), lambda: do_switch(combo, entry), suggested=True)
        button(_t("Annuler"), blocked_view)
        window.show_all()
        entry.grab_focus()

    def do_switch(combo, entry):
        name = combo.get_active_id()
        if name and relay({"op": "switch", "user": name, "password": entry.get_text()}, timeout=90).get("ok"):
            blocked_view()
        else:
            entry.set_text("")
            error.set_text(_t("Le mot de passe est incorrect."))

    def finish():
        try:
            Gdk.Display.get_default().get_default_seat().ungrab()
        except Exception:
            pass
        state["done"] = True
        Gtk.main_quit()

    def grab():
        gdk_window = window.get_window()
        if gdk_window is None:
            return True
        gdk_window.raise_()
        seat = Gdk.Display.get_default().get_default_seat()
        status = seat.grab(gdk_window, Gdk.SeatCapabilities.ALL, True, None, None, None)
        if status == Gdk.GrabStatus.SUCCESS:
            state["grab_failures"] = 0
        else:
            state["grab_failures"] += 1
            # Une application plein écran du compte garde le clavier : elle est fermée.
            if state["grab_failures"] % 10 == 0:
                pid = foreground_pid(display)
                if pid is not None and proc_uid(pid) == me.get("uid"):
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except OSError:
                        pass
        return True

    def tick():
        current = state_of(user)
        if state["mode"] == "blocked" and current["allowed"]:
            unlock_view()
        elif state["mode"] == "unlock" and not current["allowed"]:
            blocked_view()
        return True

    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, signal.SIG_IGN)
    blocked_view()
    window.show_all()
    GLib.timeout_add(500, grab)
    GLib.timeout_add_seconds(5, tick)
    Gtk.main()
    return 0 if state.get("done") else 1


# ── Paramètres › Comptes › Contrôle parental ────────────────────────────────

def call(args, stdin=None, timeout=300):
    """Commande en root : directe pour root, sinon par le relais (UAC si besoin)."""
    if os.geteuid() == 0 or not os.path.exists(RELAY_SOCKET) or os.environ.get("WINLUX_PARENTAL_DIRECT") == "1":
        command = [sys.executable, "-B", SELF, *args]
    else:
        command = [shutil.which("winlux-root") or "/usr/bin/winlux-root", "exec", "winlux-parental", *args]
    try:
        result = subprocess.run(command, input=stdin, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.SubprocessError) as error:
        return {"ok": False, "error": str(error)}
    for line in reversed(result.stdout.splitlines()):
        if line.startswith("{"):
            try:
                return json.loads(line)
            except ValueError:
                break
    if result.returncode == 1223 % 256:
        return {"ok": False, "error": _t("Modification annulée : confirmation d’un administrateur refusée."),
                "cancelled": True}
    return {"ok": False, "error": (result.stderr or result.stdout).strip()[-400:]}


def available():
    return not Path(os.environ.get("WINLUX_LIVE_FLAG", "/etc/winlux/live-session")).exists()


def page(host):
    return ParentalPanel(host)


LIMITS = (None, 15, 30, 45, 60, 90, 120, 150, 180, 240, 300, 360, 480)
STARTS = [f"{h:02d}:{m:02d}" for h in range(24) for m in (0, 30)]
ENDS = STARTS[1:] + ["23:59"]


class ParentalPanel:
    """Page des Paramètres : comptes surveillés, réglages d'un compte, activité."""

    def __init__(self, host):
        import gi
        gi.require_version("Gtk", "3.0")
        from gi.repository import GLib, Gtk
        self.Gtk, self.GLib, self.host = Gtk, GLib, host
        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.box.set_name("parental-page")
        host.body.pack_start(self.box, False, False, 0)
        self.user = self.cfg = None
        self.save_source = None
        try:
            self.me = pwd.getpwuid(os.getuid()).pw_name
        except KeyError:
            self.me = os.environ.get("USER", "")
        self.start()

    # Briques (mêmes classes CSS que le reste des Paramètres).

    def clear(self):
        for child in self.box.get_children():
            child.destroy()

    def text(self, value, style=None, parent=None, name=None):
        label = self.Gtk.Label(label=value, xalign=0, wrap=True)
        if style:
            label.get_style_context().add_class(style)
        if name:
            label.set_name(name)
        (parent or self.box).pack_start(label, False, False, 0)
        return label

    def card(self):
        box = self.Gtk.Box(orientation=self.Gtk.Orientation.VERTICAL, spacing=10)
        box.get_style_context().add_class("card")
        self.box.pack_start(box, False, False, 0)
        return box

    def line(self, parent, title, subtitle=None, widgets=()):
        row = self.Gtk.Box(spacing=12)
        texts = self.Gtk.Box(orientation=self.Gtk.Orientation.VERTICAL, spacing=2, valign=self.Gtk.Align.CENTER)
        texts.pack_start(self.Gtk.Label(label=title, xalign=0, wrap=True), False, False, 0)
        if subtitle:
            self.text(subtitle, "subtitle", texts)
        row.pack_start(texts, True, True, 0)
        for widget in widgets:
            widget.set_valign(self.Gtk.Align.CENTER)
            row.pack_start(widget, False, False, 0)
        parent.pack_start(row, False, False, 0)
        return row

    def button(self, label, callback, name=None):
        widget = self.Gtk.Button(label=label)
        if name:
            widget.set_name(name)
        widget.connect("clicked", lambda *_: callback())
        return widget

    def background(self, args, after, stdin=None):
        def worker():
            answer = call(args, stdin)
            self.GLib.idle_add(lambda: after(answer) and False)
        threading.Thread(target=worker, daemon=True).start()

    def show(self):
        self.box.show_all()

    # Accueil de la page.

    def start(self):
        self.clear()
        self.text(_t("Un administrateur fixe pour un compte standard un temps d’écran, des horaires, des applications et des sites bloqués, et consulte son activité. Tout reste sur ce PC : aucun compte en ligne, aucun envoi de données."), "subtitle")
        self.message = self.text("", "error", name="parental-message")
        self.message.set_no_show_all(True)
        # Premier affichage tout de suite (commande locale ou relais, sans confirmation).
        if is_admin(self.me):
            self.overview(call(["list"]))
        else:
            self.not_admin(call(["status"]))
        self.show()

    def load_accounts(self):
        self.background(["list"], self.overview)

    def not_admin(self, answer):
        box = self.card()
        if answer.get("supervised"):
            self.line(box, _t("Votre compte est géré par le contrôle parental."))
            remaining = answer.get("remaining")
            self.text(_t("Temps restant aujourd’hui : {time}").format(time=duration(remaining)) if remaining is not None
                      else _t("Pas de limite de temps aujourd’hui."), "subtitle", box, "parental-remaining")
        else:
            self.line(box, _t("Seul un administrateur peut gérer le contrôle parental de ce PC."),
                      widgets=[self.button(_t("Gérer en tant qu’administrateur…"), self.load_accounts, "parental-elevate")])
        self.show()

    def failed(self, answer):
        self.message.set_text(answer.get("error") or _t("Le service du contrôle parental ne répond pas."))
        self.message.show()
        self.show()

    def overview(self, answer):
        if not answer.get("ok"):
            if not answer.get("cancelled"):
                box = self.card()
                self.line(box, _t("Le service du contrôle parental ne répond pas."),
                          widgets=[self.button(_t("Réessayer"), self.start)])
            return self.failed(answer)
        self.clear()
        self.text(_t("Un administrateur fixe pour un compte standard un temps d’écran, des horaires, des applications et des sites bloqués, et consulte son activité. Tout reste sur ce PC : aucun compte en ligne, aucun envoi de données."), "subtitle")
        self.message = self.text("", "error", name="parental-message")
        self.message.set_no_show_all(True)
        label = self.Gtk.Label(label=_t("Comptes de la famille"), xalign=0)
        label.get_style_context().add_class("section")
        self.box.pack_start(label, False, False, 0)
        standard = [account for account in answer.get("accounts", []) if not account["admin"]]
        if not standard:
            box = self.card()
            self.line(box, _t("Aucun compte standard sur ce PC. Créez d’abord un compte pour l’enfant dans Comptes."),
                      widgets=[self.button(_t("Comptes"), lambda: self.host.open_page(33))])
        for account in standard:
            box = self.card()
            name = account["name"]
            title = f"{account['fullname']} ({name})" if account["fullname"] and account["fullname"] != name else name
            today = account.get("today") or {}
            if account["enabled"]:
                used = duration(today.get("used", 0))
                subtitle = _t("Surveillé · aujourd’hui {used} sur {limit}").format(used=used, limit=duration(today["limit"])) \
                    if today.get("limit") is not None else _t("Surveillé · aujourd’hui {used}").format(used=used)
            else:
                subtitle = _t("Surveillance suspendue") if account["configured"] else _t("Non surveillé")
            switch = self.Gtk.Switch(active=account["enabled"])
            switch.set_name(f"parental-enable-{name}")
            switch.set_tooltip_text(_t("Activer le contrôle parental"))
            switch.connect("notify::active", lambda widget, _p, n=name: self.toggle(n, widget.get_active()))
            widgets = [switch]
            if account["enabled"]:
                widgets.insert(0, self.button(_t("Gérer"), lambda n=name: self.open_account(n), f"parental-manage-{name}"))
            self.line(box, title, subtitle, widgets)
        self.show()

    def toggle(self, name, on):
        self.background(["enable" if on else "disable", name],
                        lambda answer: self.load_accounts() if answer.get("ok") else (self.failed(answer), self.load_accounts()))

    # Réglages d'un compte.

    def open_account(self, name):
        self.user = name

        def loaded(answer):
            if not answer.get("ok"):
                return self.failed(answer)
            self.cfg = answer["config"]
            self.background(["report", name], self.detail)
        self.background(["get", name], loaded)

    def detail(self, report):
        Gtk = self.Gtk
        self.clear()
        back = self.button(_t("‹ Tous les comptes"), self.load_accounts, "parental-back")
        back.set_halign(Gtk.Align.START)
        self.box.pack_start(back, False, False, 0)
        self.message = self.text("", "error", name="parental-message")
        self.message.set_no_show_all(True)
        self.time_section(report.get("today") or {})
        self.apps_section(report)
        self.web_section()
        self.activity_section(report)
        box = self.card()
        self.line(box, _t("Effacer l’activité et les réglages"),
                  widgets=[self.button(_t("Effacer l’activité et les réglages"),
                                       lambda: self.background(["forget", self.user], lambda _a: self.load_accounts()))])
        self.show()

    def section(self, title, hint=None):
        label = self.Gtk.Label(label=title, xalign=0)
        label.get_style_context().add_class("section")
        self.box.pack_start(label, False, False, 0)
        if hint:
            self.text(hint, "subtitle")

    def time_section(self, today):
        Gtk = self.Gtk
        self.section(_t("Temps d’écran"), _t("Durée maximale par jour et heures où l’ordinateur peut servir. Un avertissement arrive 5 minutes avant la fin."))
        box = self.card()
        grid = Gtk.Grid(column_spacing=10, row_spacing=6)
        box.pack_start(grid, False, False, 0)
        for row, day in enumerate(DAYS):
            rule = self.cfg["days"][day]
            grid.attach(Gtk.Label(label=_t(DAY_NAMES[day]), xalign=0), 0, row, 1, 1)
            limit = Gtk.ComboBoxText()
            limit.set_name(f"parental-limit-{day}")
            for value in LIMITS:
                limit.append(str(value), _t("Illimité") if value is None else duration(value))
            if rule["limit"] is not None and rule["limit"] not in LIMITS:
                limit.append(str(rule["limit"]), duration(rule["limit"]))
            limit.set_active_id(str(rule["limit"]))
            limit.connect("changed", lambda w, d=day: self.set_day(d, limit=None if w.get_active_id() == "None" else int(w.get_active_id())))
            grid.attach(limit, 1, row, 1, 1)
            start = Gtk.ComboBoxText()
            start.set_name(f"parental-from-{day}")
            start.append("None", _t("Toute la journée"))
            for value in STARTS:
                start.append(value, _t("de") + " " + value)
            start.set_active_id(str(rule["from"]))
            end = Gtk.ComboBoxText()
            end.set_name(f"parental-to-{day}")
            for value in ENDS:
                end.append(value, _t("à") + " " + value)
            end.set_active_id(rule["to"] or "21:00")
            end.set_sensitive(rule["from"] is not None)
            start.connect("changed", lambda w, d=day, e=end: self.set_range(d, w.get_active_id(), e))
            end.connect("changed", lambda w, d=day, s=start: s.get_active_id() != "None" and self.set_range(d, s.get_active_id(), w))
            grid.attach(start, 2, row, 1, 1)
            grid.attach(end, 3, row, 1, 1)
        buttons = Gtk.Box(spacing=8)
        buttons.pack_start(self.button(_t("Appliquer lundi à tous les jours"), self.copy_monday, "parental-copy"), False, False, 0)
        box.pack_start(buttons, False, False, 0)
        grant_box = Gtk.Box(spacing=8)
        for minutes in (15, 30, 60):
            grant_box.pack_start(self.button("+ " + duration(minutes), lambda m=minutes: self.grant(m),
                                             f"parental-grant-{minutes}"), False, False, 0)
        self.line(box, _t("Accorder du temps maintenant"), None, [grant_box])
        self.grant_label = self.text(_t("Temps accordé jusqu’à {time}.").format(time=today["grant_until"])
                                     if today.get("granted") else "", "subtitle", box)

    def set_day(self, day, **changes):
        self.cfg["days"][day].update(changes)
        self.schedule_save()

    def set_range(self, day, start, end_combo):
        end_combo.set_sensitive(start != "None")
        if start == "None":
            return self.set_day(day, **{"from": None, "to": None})
        end = end_combo.get_active_id() or "21:00"
        if clock(end) <= clock(start):
            end = ENDS[min(len(ENDS) - 1, STARTS.index(start) + 2)]
            end_combo.set_active_id(end)
            return
        self.set_day(day, **{"from": start, "to": end})

    def copy_monday(self):
        for day in DAYS[1:]:
            self.cfg["days"][day] = dict(self.cfg["days"]["mon"])
        self.save(refresh=True)

    def grant(self, minutes):
        def done(answer):
            if answer.get("ok"):
                self.grant_label.set_text(_t("Temps accordé jusqu’à {time}.").format(time=answer["until"]))
            else:
                self.failed(answer)
        self.background(["grant", self.user, str(minutes)], done)

    def schedule_save(self):
        if self.save_source:
            self.GLib.source_remove(self.save_source)
        self.save_source = self.GLib.timeout_add(700, lambda: self.save() and False)

    def save(self, refresh=False):
        self.save_source = None

        def done(answer):
            if answer.get("ok"):
                self.cfg = answer["config"]
                self.message.hide()
                if refresh:
                    self.open_account(self.user)
            else:
                self.failed(answer)
                self.open_account(self.user)
        self.background(["set", self.user], done, json.dumps(self.cfg))
        return False

    def apps_section(self, report):
        Gtk = self.Gtk
        self.section(_t("Applications"), _t("Une application bloquée se ferme aussitôt ouverte, et la tentative apparaît dans l’activité."))
        box = self.card()
        if not self.cfg["apps"]:
            self.text(_t("Aucune application bloquée."), "subtitle", box)
        names = desktop_names()
        for app in self.cfg["apps"]:
            self.line(box, shown_name(app["id"], app["name"], names), app["id"].split(":", 1)[1],
                      [self.button(_t("Débloquer"), lambda i=app["id"]: self.unblock(i))])
        choices = {}
        for app in report.get("apps", []):
            choices.setdefault(app["id"], names.get(app["id"], app["name"]))
        for ident, name in sorted(desktop_choices().items(), key=lambda item: item[1].casefold()):
            choices.setdefault(ident, name)
        blocked = {app["id"] for app in self.cfg["apps"]}
        combo = Gtk.ComboBoxText.new_with_entry()
        combo.set_name("parental-app-choice")
        combo.get_child().set_placeholder_text(_t("Choisir une application…"))
        for ident, name in choices.items():
            if ident not in blocked:
                combo.append(ident, f"{name} ({ident.split(':', 1)[1]})")
        combo.set_hexpand(True)
        # Proposer pendant la saisie les applications dont le nom contient le texte (#2350).
        completion = Gtk.EntryCompletion()
        completion.set_model(combo.get_model())
        completion.set_text_column(0)
        completion.set_minimum_key_length(1)
        completion.set_match_func(
            lambda _c, key, it, model: key.strip().casefold() in (model[it][0] or "").casefold(), combo.get_model())

        def picked(_completion, model, it):
            combo.set_active_id(model[it][1])
            return True
        completion.connect("match-selected", picked)
        combo.get_child().set_completion(completion)
        row = Gtk.Box(spacing=8)
        row.pack_start(combo, True, True, 0)
        unknown = Gtk.Label(xalign=0, wrap=True)
        unknown.set_name("parental-app-unknown")
        unknown.get_style_context().add_class("error")
        unknown.set_no_show_all(True)
        combo.get_child().connect("changed", lambda *_: unknown.hide())
        row.pack_start(self.button(_t("Bloquer"), lambda: self.block_choice(combo, choices, unknown), "parental-block"),
                       False, False, 0)
        box.pack_start(row, False, False, 0)
        box.pack_start(unknown, False, False, 0)

    def block_choice(self, combo, choices, unknown=None):
        ident = combo.get_active_id()
        if ident is None:
            typed = combo.get_child().get_text().strip()
            if not typed:
                return
            ident = typed_app(typed, choices)
            if ident is None:
                # Rien n'est bloqué : le dire sous le champ, au moment du clic, et
                # proposer l'application si elle existe sous un autre nom (#2350).
                found = typed_suggestions(typed, choices)
                names = _t(" ou ").join(f"« {choices[i]} »" for i in found)
                if len(found) == 1:
                    combo.set_active_id(found[0])
                    text = _t("Rien n’a été bloqué. Vouliez-vous dire {names} ? Elle est sélectionnée : "
                              "cliquez à nouveau sur Bloquer.").format(names=names)
                elif found:
                    text = _t("Rien n’a été bloqué. Vouliez-vous dire {names} ? Choisissez-la puis cliquez sur Bloquer.") \
                        .format(names=names)
                else:
                    text = _t("Application introuvable : {name}. Rien n’a été bloqué ; choisissez-la dans la liste.") \
                        .format(name=typed)
                if unknown is None:
                    return self.failed({"error": text})
                unknown.set_text(text)
                unknown.show()
                return None
        self.block(ident, choices.get(ident))

    def block(self, ident, name=None):
        if not APP_ID.match(ident) or protected(ident):
            return self.failed({"error": _t("Adresse non valable : {name}").format(name=ident.split(":", 1)[-1])})
        if all(app["id"] != ident for app in self.cfg["apps"]):
            self.cfg["apps"].append({"id": ident, "name": name or display_name(ident, {})})
        self.save(refresh=True)

    def unblock(self, ident):
        self.cfg["apps"] = [app for app in self.cfg["apps"] if app["id"] != ident]
        self.save(refresh=True)

    def web_section(self):
        Gtk = self.Gtk
        web = self.cfg["web"]
        self.section(_t("Sites web"), _t("S’applique à tous les navigateurs de ce compte."))
        box = self.card()
        mode = Gtk.ComboBoxText()
        mode.set_name("parental-web-mode")
        for key, label in (("off", _t("Aucun filtre")), ("block", _t("Bloquer les sites de la liste")),
                           ("allow", _t("Autoriser seulement les sites de la liste"))):
            mode.append(key, label)
        mode.set_active_id(web["mode"])
        mode.connect("changed", lambda w: self.set_web(mode=w.get_active_id()))
        self.line(box, _t("Filtre"), None, [mode])
        safe = Gtk.Switch(active=web["safesearch"])
        safe.set_name("parental-safesearch")
        safe.connect("notify::active", lambda w, _p: self.set_web(safesearch=w.get_active()))
        self.line(box, _t("Filtrer les recherches pour adultes"),
                  _t("Google, Bing et DuckDuckGo en recherche protégée, YouTube en mode restreint."), [safe])
        if web["mode"] == "off":
            return
        key = "allowed" if web["mode"] == "allow" else "blocked"
        self.text(_t("Sites autorisés") if key == "allowed" else _t("Sites bloqués"), None, box)
        if not web[key]:
            self.text(_t("Liste vide."), "subtitle", box)
        for site in web[key]:
            self.line(box, site, None, [self.button(_t("Retirer"), lambda s=site, k=key: self.remove_site(k, s))])
        entry = Gtk.Entry(placeholder_text=_t("Adresse (par exemple exemple.com)"))
        entry.set_name("parental-site")
        entry.set_hexpand(True)
        entry.connect("activate", lambda *_: self.add_site(key, entry.get_text()))
        row = Gtk.Box(spacing=8)
        row.pack_start(entry, True, True, 0)
        row.pack_start(self.button(_t("Ajouter"), lambda: self.add_site(key, entry.get_text()), "parental-site-add"), False, False, 0)
        box.pack_start(row, False, False, 0)

    def set_web(self, **changes):
        self.cfg["web"].update(changes)
        self.save(refresh="mode" in changes)

    def add_site(self, key, text):
        domain = clean_domain(text)
        if domain is None:
            return self.failed({"error": _t("Adresse non valable : {name}").format(name=text)})
        if domain not in self.cfg["web"][key]:
            self.cfg["web"][key].append(domain)
        self.save(refresh=True)

    def remove_site(self, key, site):
        self.cfg["web"][key] = [item for item in self.cfg["web"][key] if item != site]
        self.save(refresh=True)

    def activity_section(self, report):
        Gtk = self.Gtk
        self.section(_t("Activité des 7 derniers jours"))
        box = self.card()
        days = report.get("days", [])
        top = max([day["minutes"] for day in days] + [60])
        for day in days:
            bar = Gtk.LevelBar(min_value=0, max_value=top, value=day["minutes"])
            bar.set_size_request(220, 8)
            date = datetime.date.fromisoformat(day["date"])
            self.line(box, f"{_t(DAY_NAMES[day['day']])} {date.strftime('%d/%m')}", duration(day["minutes"]), [bar])
        apps = report.get("apps", [])
        if apps:
            self.text(_t("Applications les plus utilisées"), None, box)
            blocked = {app["id"] for app in self.cfg["apps"]}
            for app in apps:
                widgets = [] if app["id"] in blocked or protected(app["id"]) else \
                    [self.button(_t("Bloquer"), lambda i=app["id"], n=app["name"]: self.block(i, n))]
                self.line(box, app["name"], duration(app["minutes"]), widgets)
        attempts = [(item["name"], item["count"]) for item in report.get("blocked_apps", [])] + \
                   [(item["site"], item["count"]) for item in report.get("blocked_sites", [])]
        if attempts:
            self.text(_t("Tentatives bloquées"), None, box)
            for name, count in attempts:
                self.line(box, name, _t("{count} fois").format(count=count))
        if not apps and not attempts and not any(day["minutes"] for day in days):
            self.text(_t("Aucune activité enregistrée."), "subtitle", box)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

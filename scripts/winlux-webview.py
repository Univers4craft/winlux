#!/usr/bin/python3
"""Moteur web des applications WebView2 (#236), sur WebKitGTK.

Une application Windows qui embarque une vue web (WebView2) la demande au
runtime Winlux, qui lance ce programme : une fenêtre WebKitGTK placée dans
la fenêtre de l'application. Le dialogue passe par des lignes JSON :

  runtime → moteur (entrée standard)
    {"op": "create", "parent": XID, "x", "y", "w", "h", "visible"}
    {"op": "bounds", "x", "y", "w", "h"}   {"op": "visible", "on": true}
    {"op": "navigate", "uri"}   {"op": "html", "html"}   {"op": "reload"}
    {"op": "stop"}   {"op": "back"}   {"op": "forward"}
    {"op": "script_doc", "js"}   {"op": "exec", "id", "js"}
    {"op": "post", "json"}   {"op": "settings", ...}   {"op": "close"}
    {"op": "map", "host", "folder"}   {"op": "unmap", "host"}   (hôtes virtuels)
    {"op": "reparent", "parent"}   {"op": "background", "a", "r", "g", "b"}

  moteur → runtime (sortie standard)
    {"ev": "ready", "xid", "user_agent"}   {"ev": "nav_starting", "uri", "nav", "headers", "kind"}
    {"ev": "content_loading", "nav"}   {"ev": "source", "uri"}
    {"ev": "nav_completed", "nav", "ok", "status", "error"}   {"ev": "title", "title"}
    {"ev": "history", "back", "forward"}   {"ev": "message", "json", "string"}
    {"ev": "exec_result", "id", "ok", "json"}   {"ev": "error", "code"}

`window.open` ouvre une fenêtre surgissante liée à la page (#2727), comme
WebView2 quand l'hôte ne traite pas NewWindowRequested. Sa taille et sa
position ne suivent les options de `window.open` que si la page les donne
(#2736).

`window.chrome.webview` (postMessage, addEventListener) est fourni aux
pages, comme l'attendent les applications WebView2.

Le moteur WebKitGTK n'est pas dans l'image : à la première application qui
en a besoin, il est proposé au téléchargement (relais root, contrôle de
compte), puis le programme se relance.
"""
import json
import os
import subprocess
import sys
import threading
import warnings

# run_javascript reste la seule API commune aux WebKitGTK 4.1 des distributions.
warnings.filterwarnings("ignore", category=DeprecationWarning)

ENGINE_PACKAGE = "gir1.2-webkit2-4.1"


# ── Langue de l'interface : fr, en, es ou de ────────────────────────────────

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
    "Affichage web": ("Web display", "Visualización web", "Webanzeige"),
    "Cette application affiche des pages web. Winlux doit d’abord télécharger son moteur d’affichage web (environ 40 Mo).":
        ("This app shows web pages. Winlux first needs to download its web display engine (about 40 MB).",
         "Esta aplicación muestra páginas web. Winlux primero debe descargar su motor de visualización web (unos 40 MB).",
         "Diese App zeigt Webseiten an. Winlux muss zuerst die Webanzeige-Engine herunterladen (etwa 40 MB)."),
    "Paquet : {package}\nVersion : {version}\nÉditeur : {publisher}\nSource : {source}\n"
    "Les signatures et sommes SHA-256 du dépôt sont vérifiées pendant le téléchargement.":
        ("Package: {package}\nVersion: {version}\nPublisher: {publisher}\nSource: {source}\n"
         "The repository signatures and SHA-256 sums are checked during the download.",
         "Paquete: {package}\nVersión: {version}\nEditor: {publisher}\nOrigen: {source}\n"
         "Las firmas y sumas SHA-256 del repositorio se comprueban durante la descarga.",
         "Paket: {package}\nVersion: {version}\nHerausgeber: {publisher}\nQuelle: {source}\n"
         "Die Signaturen und SHA-256-Summen des Repositorys werden beim Herunterladen geprüft."),
    "inconnue": ("unknown", "desconocida", "unbekannt"),
    "dépôt APT de la distribution": ("distribution APT repository", "repositorio APT de la distribución",
                                     "APT-Repository der Distribution"),
    "Télécharger": ("Download", "Descargar", "Herunterladen"),
    "Plus tard": ("Later", "Más tarde", "Später"),
    "Téléchargement du moteur d’affichage web…": ("Downloading the web display engine…",
                                                 "Descargando el motor de visualización web…",
                                                 "Webanzeige-Engine wird heruntergeladen…"),
    "Le moteur d’affichage web n’a pas pu être installé. Vérifiez la connexion à Internet.":
        ("The web display engine could not be installed. Check the Internet connection.",
         "No se pudo instalar el motor de visualización web. Compruebe la conexión a Internet.",
         "Die Webanzeige-Engine konnte nicht installiert werden. Prüfen Sie die Internetverbindung."),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# Caractères d'un « token » HTTP (RFC 9110, §5.6.2).
_TCHAR = frozenset("!#$%&'*+-.^_`|~0123456789"
                   "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz")


def navigation_kind(WebKit2, action):
    """COREWEBVIEW2_NAVIGATION_KIND d'une navigation WebKit (#2975).

    RELOAD (0) pour un rechargement, BACK_OR_FORWARD (1) pour l'historique,
    NEW_DOCUMENT (2) pour le reste. Un formulaire renvoyé
    (FORM_RESUBMITTED : rechargement ou historique, WebKit ne dit pas
    lequel) compte comme un rechargement (inférence nommée).
    """
    T = WebKit2.NavigationType
    kind = action.get_navigation_type() if action is not None else None
    if kind in (T.RELOAD, T.FORM_RESUBMITTED):
        return 0
    if kind == T.BACK_FORWARD:
        return 1
    return 2


def web_error_status(error):
    """COREWEBVIEW2_WEB_ERROR_STATUS d'une erreur de chargement WebKit (#2980).

    Nom d'hôte inconnu → HOST_NAME_NOT_RESOLVED (13) ; connexion refusée →
    CANNOT_CONNECT (12) ; délai dépassé → TIMEOUT (7) ; hôte ou réseau
    injoignable → SERVER_UNREACHABLE (6) ; chargement annulé (par WebKit,
    par une nouvelle navigation ou par une décision de l'application) →
    OPERATION_CANCELED (14) ; le reste → UNKNOWN (0).
    """
    domain = getattr(error, "domain", "")
    code = getattr(error, "code", -1)
    if domain == "g-resolver-error-quark":
        return 13
    if domain == "g-io-error-quark":
        return {39: 12, 24: 7, 37: 6, 38: 6, 19: 14}.get(code, 0)
    if domain == "WebKitNetworkError" and code == 302:
        return 14
    if domain == "WebKitPolicyError" and code == 102:
        return 14
    return 0


def completion(code):
    """(IsSuccess, WebErrorStatus) d'une page chargée avec le code HTTP `code` (#2974).

    WebView2.idl : IsSuccess vaut FALSE quand « HTTP server responds with
    4xx ». WebErrorStatus : 401 → VALID_AUTHENTICATION_CREDENTIALS_REQUIRED
    (17), 407 → VALID_PROXY_AUTHENTICATION_REQUIRED (18), autre 4xx →
    UNKNOWN (0), inférence nommée : l'énumération n'a pas d'autre valeur
    pour une réponse HTTP.
    """
    if 400 <= code < 500:
        return False, {401: 17, 407: 18}.get(code, 0)
    return True, 0


def http_status(view):
    """Code HTTP de la ressource principale, 0 sans requête HTTP (#2966).

    WebView2.idl : HttpStatusCode vaut 0 quand la navigation n'a pas fait
    de requête HTTP (file://, schéma local) ou n'a reçu aucune réponse.
    """
    uri = view.get_uri() or ""
    if not uri.lower().startswith(("http://", "https://")):
        return 0
    resource = view.get_main_resource()
    response = resource.get_response() if resource is not None else None
    return response.get_status_code() if response is not None else 0


def request_headers(request):
    """En-têtes de la requête, « Nom: valeur » une ligne chacun (#2964).

    WebKit n'en porte souvent aucun au moment de la décision de navigation.
    Une ligne coupée (retour à la ligne dans un nom ou une valeur) est
    écartée : elle ne se relirait pas.
    """
    headers = request.get_http_headers() if request is not None else None
    if headers is None:
        return ""
    lines = []
    headers.foreach(lambda name, value, *_: lines.append((name, value)))
    return "\n".join(f"{n}: {v}" for n, v in lines
                     if n and "\n" not in n + v and "\r" not in n + v and ":" not in n)


def user_agent_header(value):
    """Agent utilisateur réécrit selon la grammaire de l'en-tête User-Agent.

    RFC 9110, §10.1.5 : produits « nom[/version] » (tokens) et commentaires
    entre parenthèses, séparés par des espaces. WebView2 applique toute
    chaîne (put_UserAgent), WebKitGTK refuse celles hors grammaire et garde
    l'ancienne (#2611). On garde donc chaque mot : un caractère interdit
    devient « - » dans un nom, « . » dans une version (« 1951/49 » →
    « 1951.49 »), « ? » dans un commentaire ; une parenthèse restée ouverte
    est fermée. Rend "" quand il ne reste rien (agent par défaut).
    """
    words, i, n = [], 0, len(value)
    while i < n:
        c = value[i]
        if c in " \t\r\n":
            i += 1
        elif c == "(":
            depth, j, out = 0, i, []
            while j < n:
                d = value[j]
                if d == "(":
                    depth += 1
                elif d == ")":
                    depth -= 1
                if d in "()" or (" " <= d <= "~" and d != "\\"):
                    out.append(d)
                elif d in "\t\r\n":
                    out.append(" ")
                else:
                    out.append("?")
                j += 1
                if depth == 0:
                    break
            out.append(")" * depth)
            words.append("".join(out))
            i = j
        else:
            j = i
            while j < n and value[j] not in " \t\r\n(":
                j += 1
            word = value[i:j]
            name, slash, version = word.partition("/")
            name = "".join(ch if ch in _TCHAR else "-" for ch in name).strip("-") or "-"
            version = "".join(ch if ch in _TCHAR else "." for ch in version).strip(".")
            words.append(name + ("/" + version if slash and version else ""))
            i = j
    # Un agent utilisateur commence par un produit, pas par un commentaire.
    while words and words[0].startswith("("):
        words.pop(0)
    return " ".join(words)


def emit(**event):
    """Événement vers le runtime (une ligne JSON)."""
    line = json.dumps(event, ensure_ascii=False)
    with _OUT_LOCK:
        sys.stdout.write(line + "\n")
        sys.stdout.flush()


_OUT_LOCK = threading.Lock()


# ── Moteur absent : téléchargement à la demande ─────────────────────────────

def engine_available():
    try:
        import gi
        gi.require_version("Gtk", "3.0")
        gi.require_version("WebKit2", "4.1")
        from gi.repository import WebKit2  # noqa: F401
        return True
    except (ImportError, ValueError):
        return False


def package_details():
    """Version candidate, éditeur et source du moteur, d'après APT et
    /etc/os-release, pour les nommer avant tout téléchargement (#2591)."""
    version, source = _t("inconnue"), _t("dépôt APT de la distribution")
    try:
        out = subprocess.run(["apt-cache", "policy", ENGINE_PACKAGE], capture_output=True, text=True,
                             timeout=20, env={**os.environ, "LC_ALL": "C"}).stdout
    except (OSError, subprocess.TimeoutExpired):
        out = ""
    candidate, in_candidate = None, False
    for line in out.splitlines():
        words = line.split()
        if line.strip().startswith("Candidate:") and len(words) > 1 and words[1] != "(none)":
            candidate = words[1]
        elif candidate and words and words[-1] != "Packages" and candidate in words:
            in_candidate = True
        elif in_candidate and len(words) >= 3 and "://" in words[1]:
            source = f"{words[1]} {words[2]}"
            break
    if candidate:
        version = candidate
    publisher = "Debian"
    try:
        with open("/etc/os-release", encoding="utf-8") as rel:
            for line in rel:
                if line.startswith("NAME="):
                    publisher = line[5:].strip().strip('"') or publisher
    except OSError:
        pass
    return version, publisher, source


def install_command():
    """Commande d'installation du moteur (relais root, ou root direct)."""
    apt = ["apt-get", "install", "-y", "-q", "--no-install-recommends", ENGINE_PACKAGE]
    if os.geteuid() == 0:
        return apt
    sock = os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")
    if os.path.exists(sock):
        return ["winlux-root", "exec", *apt]
    return None


def offer_install():
    """Demande puis installe le moteur ; True s'il est désormais là."""
    if os.environ.get("WINLUX_WEBVIEW_NO_INSTALL") == "1":
        return False
    command = install_command()
    if command is None:
        return False
    try:
        import gi
        gi.require_version("Gtk", "3.0")
        from gi.repository import Gtk, GLib
    except (ImportError, ValueError):
        return False
    dialog = Gtk.MessageDialog(message_type=Gtk.MessageType.QUESTION, buttons=Gtk.ButtonsType.NONE,
                               text=_t("Affichage web"))
    version, publisher, source = package_details()
    dialog.format_secondary_text(
        _t("Cette application affiche des pages web. Winlux doit d’abord "
           "télécharger son moteur d’affichage web (environ 40 Mo).") + "\n\n"
        + _t("Paquet : {package}\nVersion : {version}\nÉditeur : {publisher}\nSource : {source}\n"
             "Les signatures et sommes SHA-256 du dépôt sont vérifiées pendant le téléchargement.").format(
            package=ENGINE_PACKAGE, version=version, publisher=publisher, source=source))
    later = dialog.add_button(_t("Plus tard"), Gtk.ResponseType.CANCEL)
    dialog.add_button(_t("Télécharger"), Gtk.ResponseType.OK)
    # Refus par défaut, comme pour tout téléchargement proposé (#2589).
    dialog.set_default_response(Gtk.ResponseType.CANCEL)
    # Bouton par défaut visible (#2602), comme dans la Boutique (#1448) :
    # « Plus tard » porte la couleur d'accent et seul il a le cadre de focus
    # dès l'ouverture (GTK ne le dessine sinon qu'après une touche du clavier ;
    # l'affichage de la fenêtre remet ce réglage à zéro, d'où « map »).
    later.get_style_context().add_class("suggested-action")
    later.set_can_default(True)
    later.grab_default()
    later.grab_focus()
    dialog.connect("map", lambda d: d.set_focus_visible(True))
    answer = dialog.run()
    dialog.destroy()
    if answer != Gtk.ResponseType.OK:
        return False
    progress = Gtk.Window(title=_t("Affichage web"))
    progress.set_default_size(420, 90)
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12, margin=18)
    box.add(Gtk.Label(label=_t("Téléchargement du moteur d’affichage web…"), xalign=0))
    bar = Gtk.ProgressBar()
    box.add(bar)
    progress.add(box)
    progress.show_all()
    result = {}

    def work():
        try:
            result["code"] = subprocess.call(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError:
            result["code"] = 1
        GLib.idle_add(Gtk.main_quit)

    def pulse():
        bar.pulse()
        return "code" not in result

    GLib.timeout_add(120, pulse)
    threading.Thread(target=work, daemon=True).start()
    Gtk.main()
    progress.destroy()
    if result.get("code") == 0 and engine_available_fresh():
        return True
    failed = Gtk.MessageDialog(message_type=Gtk.MessageType.ERROR, buttons=Gtk.ButtonsType.CLOSE,
                               text=_t("Affichage web"))
    failed.format_secondary_text(_t("Le moteur d’affichage web n’a pas pu être installé. "
                                    "Vérifiez la connexion à Internet."))
    failed.run()
    failed.destroy()
    return False


def engine_available_fresh():
    """Le moteur vient d'être installé : vérifié dans un nouveau processus."""
    code = subprocess.call([sys.executable, "-c",
                            "import gi; gi.require_version('WebKit2', '4.1'); "
                            "from gi.repository import WebKit2"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return code == 0


# ── Pont window.chrome.webview ──────────────────────────────────────────────

BRIDGE = r"""
(function () {
  if (window.chrome && window.chrome.webview && window.chrome.webview.__winlux) return;
  var listeners = [];
  var webview = {
    __winlux: true,
    postMessage: function (message) {
      var isString = typeof message === "string";
      window.webkit.messageHandlers.winlux.postMessage(
        JSON.stringify({json: JSON.stringify(message), string: isString ? message : null}));
    },
    addEventListener: function (type, f) { if (type === "message") listeners.push(f); },
    removeEventListener: function (type, f) {
      var i = listeners.indexOf(f);
      if (i >= 0) listeners.splice(i, 1);
    },
    __dispatch: function (data) {
      var event = {data: data, source: webview};
      listeners.slice().forEach(function (f) { try { f(event); } catch (e) { console.error(e); } });
    },
    hostObjects: {}
  };
  window.chrome = window.chrome || {};
  window.chrome.webview = webview;
})();
"""


# ── Options de window.open (#2736) ─────────────────────────────────────────
# WebKitGTK rend toujours une géométrie pour la nouvelle fenêtre, même sans
# taille demandée (celle de la fenêtre de la page ouvrante, au minimum
# 100x100) : elle ne dit pas si la page a demandé une taille ou une
# position. Learn (ICoreWebView2WindowFeatures) : Width/Height et Left/Top
# sont ignorés si HasSize/HasPosition valent FALSE. Ce script relève donc,
# juste avant chaque window.open, si les options nomment une taille ou une
# position, d'après les noms de la norme HTML (« window open steps » :
# width, innerwidth, height, innerheight, left, screenx, top, screeny).
# Il ne transmet que deux booléens, jamais l'adresse ni les options.
POPUP_FEATURES = r"""
(function () {
  if (window.__winluxOpen) return;
  var open = window.open;
  if (typeof open !== "function") return;
  window.__winluxOpen = true;
  function names(features) {
    var found = {};
    String(features == null ? "" : features).toLowerCase()
      .split(/[\s,;]+/).forEach(function (t) {
        var n = t.split("=")[0];
        if (n) found[n] = true;
      });
    return found;
  }
  window.open = function (url, target, features) {
    try {
      var n = names(features);
      window.webkit.messageHandlers.winluxPopup.postMessage(JSON.stringify({
        size: !!(n.width || n.innerwidth || n.height || n.innerheight),
        position: !!(n.left || n.screenx || n.top || n.screeny)
      }));
    } catch (e) { /* le moteur garde alors la géométrie de WebKit */ }
    return open.apply(this, arguments);
  };
})();
"""

# Taille d'une fenêtre surgissante sans taille demandée : les deux tiers de
# la zone de travail de l'écran, au moins 640x480. Learn ne donne aucune
# taille par défaut ; ce choix est une inférence, pour qu'un contenu prévu
# pour une fenêtre ordinaire ne soit pas coupé.
POPUP_DEFAULT_FRACTION = 2 / 3
POPUP_DEFAULT_MIN = (640, 480)


def popup_default_size(workarea):
    """Taille par défaut, d'après la zone de travail (largeur, hauteur)."""
    width, height = workarea
    return (max(POPUP_DEFAULT_MIN[0], int(width * POPUP_DEFAULT_FRACTION)),
            max(POPUP_DEFAULT_MIN[1], int(height * POPUP_DEFAULT_FRACTION)))


LOCAL_SCHEME = "winlux-local"


class View:
    def __init__(self, Gtk, Gdk, GdkX11, GLib, Gio, WebKit2):
        self.Gtk, self.Gdk, self.GdkX11, self.GLib, self.Gio, self.WebKit2 = Gtk, Gdk, GdkX11, GLib, Gio, WebKit2
        self.window = None
        self.view = None
        self.nav = 0
        self.menus = True
        # AllowExternalDrop (#3237) : vrai par défaut, comme dans l'IDL.
        self.external_drop = True
        # Hôtes virtuels (SetVirtualHostNameToFolderMapping) : https://hôte/…
        # est servi depuis un dossier, sous le schéma winlux-local://.
        self.hosts = {}
        data = os.environ.get("WINLUX_WEBVIEW_DATA")
        if data:
            manager = WebKit2.WebsiteDataManager(base_data_directory=os.path.join(data, "donnees"),
                                                 base_cache_directory=os.path.join(data, "cache"))
            self.context = WebKit2.WebContext.new_with_website_data_manager(manager)
        else:
            self.context = WebKit2.WebContext.get_default()
        self.context.register_uri_scheme(LOCAL_SCHEME, self.serve_local)
        security = self.context.get_security_manager()
        security.register_uri_scheme_as_secure(LOCAL_SCHEME)
        security.register_uri_scheme_as_cors_enabled(LOCAL_SCHEME)
        language = os.environ.get("WINLUX_WEBVIEW_LANG")
        if language:
            self.context.set_preferred_languages([language])
        self.manager = WebKit2.UserContentManager()
        self.manager.register_script_message_handler("winlux")
        self.manager.connect("script-message-received::winlux", self.on_message)
        self.add_script(BRIDGE)
        # Options de window.open (#2736), pour la page et ses fenêtres
        # surgissantes ; celles-ci n'ont pas le pont chrome.webview.
        self.popup_request = None
        self.popup_manager = WebKit2.UserContentManager()
        for manager in (self.manager, self.popup_manager):
            manager.register_script_message_handler("winluxPopup")
            manager.connect("script-message-received::winluxPopup", self.on_popup_request)
            manager.add_script(WebKit2.UserScript.new(
                POPUP_FEATURES, WebKit2.UserContentInjectedFrames.ALL_FRAMES,
                WebKit2.UserScriptInjectionTime.START, None, None))

    def on_popup_request(self, _manager, result):
        """Taille et position nommées par le prochain window.open (#2736)."""
        try:
            request = json.loads(result.get_js_value().to_string())
            self.popup_request = (bool(request.get("size")), bool(request.get("position")))
        except (ValueError, AttributeError):
            self.popup_request = None

    # Scripts injetés à chaque document (AddScriptToExecuteOnDocumentCreated).
    def add_script(self, js):
        script = self.WebKit2.UserScript.new(js, self.WebKit2.UserContentInjectedFrames.ALL_FRAMES,
                                             self.WebKit2.UserScriptInjectionTime.START, None, None)
        self.manager.add_script(script)

    def blocks_drop(self, context):
        """AllowExternalDrop faux (#3237) : un glisser venu d'une autre
        application est refusé ; un glisser né dans ce moteur (dans la page)
        garde son chemin, comme le veut l'IDL (« from outside the bounds of
        webview2 »)."""
        return not self.external_drop and self.Gtk.drag_get_source_widget(context) is None

    def on_drag_motion(self, view, context, x, y, time):
        if not self.blocks_drop(context):
            return False
        self.Gdk.drag_status(context, 0, time)
        return True

    def on_drag_drop(self, view, context, x, y, time):
        if not self.blocks_drop(context):
            return False
        self.Gtk.drag_finish(context, False, False, time)
        return True

    def create(self, cmd):
        Gtk, WebKit2 = self.Gtk, self.WebKit2
        self.view = WebKit2.WebView(web_context=self.context, user_content_manager=self.manager)
        self.view.connect("context-menu", lambda *_: not self.menus)
        settings = self.view.get_settings()
        settings.set_enable_developer_extras(True)
        settings.set_javascript_can_access_clipboard(True)
        self.view.connect("load-changed", self.on_load)
        self.view.connect("load-failed", self.on_failed)
        self.view.connect("notify::title", self.on_title)
        self.view.connect("notify::uri", self.on_uri)
        self.view.connect("decide-policy", self.on_policy)
        self.view.connect("drag-motion", self.on_drag_motion)
        self.view.connect("drag-drop", self.on_drag_drop)
        # window.open (#2727) : WebView2 n'a pas de bloqueur de fenêtres
        # surgissantes ; sans NewWindowRequested traité par l'hôte, il ouvre
        # une fenêtre surgissante et la page garde la main sur elle.
        settings.set_javascript_can_open_windows_automatically(True)
        self.view.connect("create", self.on_create)
        self.popups = []
        self.window = Gtk.Window(type=Gtk.WindowType.TOPLEVEL)
        self.window.set_decorated(False)
        self.window.set_skip_taskbar_hint(True)
        self.window.set_skip_pager_hint(True)
        self.window.add(self.view)
        self.window.set_default_size(max(1, cmd.get("w", 1)), max(1, cmd.get("h", 1)))
        self.window.realize()
        gdk_window = self.window.get_window()
        parent = cmd.get("parent") or 0
        if parent:
            display = self.Gdk.Display.get_default()
            foreign = self.GdkX11.X11Window.foreign_new_for_display(display, parent)
            if foreign is not None:
                gdk_window.reparent(foreign, cmd.get("x", 0), cmd.get("y", 0))
        self.window.show_all()
        self.bounds(cmd)
        if not cmd.get("visible", True):
            self.window.hide()
        # Agent par défaut du moteur, que get_UserAgent rend tant que
        # l'application n'en a pas écrit un autre (#2611).
        emit(ev="ready", xid=gdk_window.get_xid(),
             user_agent=self.view.get_settings().get_user_agent() or "")

    # ── Fenêtres surgissantes (window.open, #2727) ──
    def on_create(self, view, action):
        """Signal « create » : la vue rendue devient le résultat de window.open.

        Learn (ICoreWebView2::add_NewWindowRequested) : quand l'hôte ne
        fournit ni NewWindow ni Handled, WebView2 ouvre une fenêtre surgissante
        et window.open rend un objet fenêtre que la page manipule (document,
        postMessage, close). La vue est liée à celle de la page (même
        processus web, même origine possible pour about:blank) ; elle n'a pas
        le pont chrome.webview, qui n'appartient qu'à la vue de l'hôte.
        Aucune navigation n'est lancée ici : WebKit charge lui-même l'adresse
        demandée (about:blank compris) dans la vue rendue.
        """
        WebKit2 = self.WebKit2
        popup = WebKit2.WebView(related_view=view, user_content_manager=self.popup_manager)
        # Options relevées juste avant ce window.open ; sans relevé, la
        # géométrie de WebKit est gardée telle quelle.
        popup.winlux_request = self.popup_request
        self.popup_request = None
        popup.get_settings().set_javascript_can_open_windows_automatically(True)
        popup.connect("create", self.on_create)
        popup.connect("ready-to-show", self.on_popup_ready)
        popup.connect("close", self.on_popup_close)
        uri = action.get_request().get_uri() or ""
        print(f"winlux-webview : window.open {uri.split('?')[0][:120]!r}, fenêtre surgissante (#2727)",
              file=sys.stderr, flush=True)
        return popup

    def on_popup_ready(self, popup):
        """ready-to-show : fenêtre de premier niveau à la taille demandée.

        Une taille ou une position que la page n'a pas demandée n'est pas
        reprise de WebKit (#2736) : la taille par défaut vient de la zone de
        travail, et le gestionnaire de fenêtres place la fenêtre.
        """
        Gtk = self.Gtk
        window = Gtk.Window(type=Gtk.WindowType.TOPLEVEL)
        geometry = popup.get_window_properties().get_geometry()
        size, position = getattr(popup, "winlux_request", None) or (True, True)
        if size and geometry.width > 0 and geometry.height > 0:
            w, h = max(100, geometry.width), max(100, geometry.height)
        else:
            w, h = popup_default_size(self.workarea())
        window.set_default_size(w, h)
        if position and (geometry.x or geometry.y):
            window.move(geometry.x, geometry.y)
        popup.connect("notify::title", lambda v, _p: window.set_title(v.get_title() or ""))
        window.add(popup)
        window.connect("delete-event", lambda *_: (popup.try_close(), True)[1])
        self.popups.append(window)
        window.show_all()

    def workarea(self):
        """Zone de travail de l'écran de la fenêtre de la page."""
        display = self.Gdk.Display.get_default()
        monitor = None
        gdk_window = self.window.get_window() if self.window else None
        if display is not None and gdk_window is not None:
            monitor = display.get_monitor_at_window(gdk_window)
        if monitor is None and display is not None:
            monitor = display.get_primary_monitor() or display.get_monitor(0)
        if monitor is None:
            return POPUP_DEFAULT_MIN
        area = monitor.get_workarea()
        return (area.width, area.height)

    def on_popup_close(self, popup):
        """window.close() de la page, ou fermeture acceptée : la fenêtre part."""
        window = popup.get_toplevel()
        if window in self.popups:
            self.popups.remove(window)
        if isinstance(window, self.Gtk.Window):
            window.destroy()
        else:
            popup.destroy()

    def reparent(self, parent):
        gdk_window = self.window.get_window() if self.window else None
        if gdk_window is None or not parent:
            return
        foreign = self.GdkX11.X11Window.foreign_new_for_display(self.Gdk.Display.get_default(), parent)
        if foreign is not None:
            gdk_window.reparent(foreign, 0, 0)

    # ── Hôtes virtuels ──
    def local_uri(self, uri):
        """https://hôte/chemin d'un hôte virtuel → winlux-local://hôte/chemin."""
        for scheme in ("https://", "http://"):
            if uri.lower().startswith(scheme):
                rest = uri[len(scheme):]
                host = rest.split("/", 1)[0].split(":", 1)[0].lower()
                if host in self.hosts:
                    return LOCAL_SCHEME + "://" + rest
        return None

    def serve_local(self, request):
        from urllib.parse import unquote, urlsplit
        parts = urlsplit(request.get_uri())
        folder = self.hosts.get((parts.hostname or "").lower())
        path = unquote(parts.path or "/")
        if folder is None:
            request.finish_error(self.GLib.Error.new_literal(self.Gio.io_error_quark(), "hôte inconnu",
                                                             int(self.Gio.IOErrorEnum.NOT_FOUND)))
            return
        root = os.path.realpath(folder)
        target = os.path.realpath(os.path.join(root, path.lstrip("/")))
        if os.path.isdir(target):
            target = os.path.join(target, "index.html")
        if not (target == root or target.startswith(root + os.sep)) or not os.path.isfile(target):
            request.finish_error(self.GLib.Error.new_literal(self.Gio.io_error_quark(), "introuvable",
                                                             int(self.Gio.IOErrorEnum.NOT_FOUND)))
            return
        import mimetypes
        mime = mimetypes.guess_type(target)[0] or "application/octet-stream"
        with open(target, "rb") as f:
            data = f.read()
        stream = self.Gio.MemoryInputStream.new_from_bytes(self.GLib.Bytes.new(data))
        request.finish(stream, len(data), mime)

    def set_user_agent(self, settings, wanted):
        """put_UserAgent : appliquée telle quelle si WebKit l'accepte (#2611)."""
        applied = user_agent_header(wanted)
        if applied != wanted.strip():
            print(f"winlux-webview : agent utilisateur hors grammaire HTTP {wanted[:400]!r}, "
                  f"appliqué {applied[:400]!r} (#2611)", file=sys.stderr, flush=True)
        # "" rend l'agent par défaut du moteur (webkit_settings_set_user_agent).
        settings.set_user_agent(applied or None)

    def bounds(self, cmd):
        if self.window is None:
            return
        w, h = max(1, cmd.get("w", 1)), max(1, cmd.get("h", 1))
        self.window.resize(w, h)
        gdk_window = self.window.get_window()
        if gdk_window is not None:
            gdk_window.move_resize(cmd.get("x", 0), cmd.get("y", 0), w, h)

    def on_policy(self, view, decision, kind):
        if kind == self.WebKit2.PolicyDecisionType.NAVIGATION_ACTION:
            uri = decision.get_navigation_action().get_request().get_uri()
            local = self.local_uri(uri)
            if local is not None:
                decision.ignore()
                self.view.load_uri(local)
                return True
            self.nav += 1
            emit(ev="nav_starting", uri=uri, nav=self.nav,
                 headers=request_headers(decision.get_navigation_action().get_request()),
                 kind=navigation_kind(self.WebKit2, decision.get_navigation_action()))
        return False

    def on_load(self, view, event):
        W = self.WebKit2.LoadEvent
        if event == W.COMMITTED:
            emit(ev="content_loading", nav=self.nav)
            emit(ev="history", back=view.can_go_back(), forward=view.can_go_forward())
        elif event == W.FINISHED:
            if not getattr(self, "_failed", False):
                code = http_status(view)
                ok, error = completion(code)
                emit(ev="nav_completed", nav=self.nav, ok=ok, status=code, error=error)
            self._failed = False

    def on_failed(self, view, event, uri, error):
        self._failed = True
        emit(ev="nav_completed", nav=self.nav, ok=False, status=0,
             error=web_error_status(error))
        return False

    def on_title(self, view, _param):
        emit(ev="title", title=view.get_title() or "")

    def on_uri(self, view, _param):
        emit(ev="source", uri=view.get_uri() or "")

    def on_message(self, _manager, result):
        try:
            value = result.get_js_value().to_string()
            data = json.loads(value)
        except (ValueError, AttributeError):
            return
        emit(ev="message", json=data.get("json") or "null", string=data.get("string"),
             source=self.view.get_uri() or "")

    def run_js(self, js, ident):
        def done(view, task):
            try:
                value = view.run_javascript_finish(task).get_js_value()
                text = "null" if value.is_undefined() else value.to_json(0)
                emit(ev="exec_result", id=ident, ok=True, json=text or "null")
            except Exception:  # noqa: BLE001 - erreur de script : null, comme WebView2
                emit(ev="exec_result", id=ident, ok=True, json="null")
        self.view.run_javascript(js, None, done)

    def handle(self, cmd):
        op = cmd.get("op")
        if op == "create":
            self.create(cmd)
        elif self.view is None:
            if op == "script_doc":
                self.add_script(cmd.get("js", ""))
            elif op == "close":
                return False
            return True
        elif op == "bounds":
            self.bounds(cmd)
        elif op == "visible":
            (self.window.show_all if cmd.get("on") else self.window.hide)()
        elif op == "navigate":
            uri = cmd.get("uri", "about:blank")
            self.view.load_uri(self.local_uri(uri) or uri)
        elif op == "map":
            self.hosts[cmd.get("host", "").lower()] = cmd.get("folder", "")
        elif op == "unmap":
            self.hosts.pop(cmd.get("host", "").lower(), None)
        elif op == "reparent":
            self.reparent(cmd.get("parent") or 0)
        elif op == "background":
            rgba = self.Gdk.RGBA()
            rgba.red, rgba.green, rgba.blue = (cmd.get(k, 255) / 255 for k in ("r", "g", "b"))
            rgba.alpha = cmd.get("a", 255) / 255
            self.view.set_background_color(rgba)
        elif op == "html":
            self.view.load_html(cmd.get("html", ""), "about:blank")
        elif op == "reload":
            self.view.reload()
        elif op == "stop":
            self.view.stop_loading()
        elif op == "back":
            self.view.go_back()
        elif op == "forward":
            self.view.go_forward()
        elif op == "script_doc":
            self.add_script(cmd.get("js", ""))
        elif op == "exec":
            self.run_js(cmd.get("js", ""), cmd.get("id", 0))
        elif op == "post":
            payload = cmd.get("json", "null")
            self.view.run_javascript(
                "window.chrome && window.chrome.webview && window.chrome.webview.__dispatch(%s);" % payload,
                None, None)
        elif op == "settings":
            settings = self.view.get_settings()
            if "script" in cmd:
                settings.set_enable_javascript(bool(cmd["script"]))
            if "menus" in cmd:
                self.menus = bool(cmd["menus"])
            if "devtools" in cmd:
                settings.set_enable_developer_extras(bool(cmd["devtools"]))
            if "zoom" in cmd:
                self.view.set_zoom_level(float(cmd["zoom"]) or 1.0)
            if "user_agent" in cmd:
                self.set_user_agent(settings, cmd["user_agent"] or "")
            if "external_drop" in cmd:
                self.external_drop = bool(cmd["external_drop"])
        elif op == "close":
            return False
        return True


def main():
    if not engine_available():
        if not offer_install():
            emit(ev="error", code="engine-missing")
            return 3
        os.execv(sys.executable, [sys.executable, *sys.argv])
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    gi.require_version("GdkX11", "3.0")
    gi.require_version("WebKit2", "4.1")
    from gi.repository import Gtk, Gdk, GdkX11, GLib, Gio, WebKit2
    view = View(Gtk, Gdk, GdkX11, GLib, Gio, WebKit2)

    def reader():
        for line in sys.stdin:
            try:
                cmd = json.loads(line)
            except ValueError:
                continue
            GLib.idle_add(dispatch, cmd)
        GLib.idle_add(Gtk.main_quit)

    def dispatch(cmd):
        if not view.handle(cmd):
            Gtk.main_quit()
        return False

    threading.Thread(target=reader, daemon=True).start()
    Gtk.main()
    return 0


if __name__ == "__main__":
    sys.exit(main())

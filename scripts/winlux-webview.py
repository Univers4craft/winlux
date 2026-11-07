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

  moteur → runtime (sortie standard)
    {"ev": "ready", "xid"}   {"ev": "nav_starting", "uri", "nav"}
    {"ev": "content_loading", "nav"}   {"ev": "source", "uri"}
    {"ev": "nav_completed", "nav", "ok", "status"}   {"ev": "title", "title"}
    {"ev": "history", "back", "forward"}   {"ev": "message", "json", "string"}
    {"ev": "exec_result", "id", "ok", "json"}   {"ev": "error", "code"}

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
    dialog.format_secondary_text(_t("Cette application affiche des pages web. Winlux doit d’abord "
                                    "télécharger son moteur d’affichage web (environ 40 Mo)."))
    dialog.add_button(_t("Plus tard"), Gtk.ResponseType.CANCEL)
    dialog.add_button(_t("Télécharger"), Gtk.ResponseType.OK)
    dialog.set_default_response(Gtk.ResponseType.OK)
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


class View:
    def __init__(self, Gtk, Gdk, GdkX11, GLib, WebKit2):
        self.Gtk, self.Gdk, self.GdkX11, self.GLib, self.WebKit2 = Gtk, Gdk, GdkX11, GLib, WebKit2
        self.window = None
        self.view = None
        self.nav = 0
        self.manager = WebKit2.UserContentManager()
        self.manager.register_script_message_handler("winlux")
        self.manager.connect("script-message-received::winlux", self.on_message)
        self.add_script(BRIDGE)

    # Scripts injetés à chaque document (AddScriptToExecuteOnDocumentCreated).
    def add_script(self, js):
        script = self.WebKit2.UserScript.new(js, self.WebKit2.UserContentInjectedFrames.ALL_FRAMES,
                                             self.WebKit2.UserScriptInjectionTime.START, None, None)
        self.manager.add_script(script)

    def create(self, cmd):
        Gtk, WebKit2 = self.Gtk, self.WebKit2
        self.view = WebKit2.WebView.new_with_user_content_manager(self.manager)
        settings = self.view.get_settings()
        settings.set_enable_developer_extras(True)
        settings.set_javascript_can_access_clipboard(True)
        self.view.connect("load-changed", self.on_load)
        self.view.connect("load-failed", self.on_failed)
        self.view.connect("notify::title", self.on_title)
        self.view.connect("notify::uri", self.on_uri)
        self.view.connect("decide-policy", self.on_policy)
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
        emit(ev="ready", xid=gdk_window.get_xid())

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
            self.nav += 1
            emit(ev="nav_starting", uri=uri, nav=self.nav)
        return False

    def on_load(self, view, event):
        W = self.WebKit2.LoadEvent
        if event == W.COMMITTED:
            emit(ev="content_loading", nav=self.nav)
            emit(ev="history", back=view.can_go_back(), forward=view.can_go_forward())
        elif event == W.FINISHED:
            if not getattr(self, "_failed", False):
                emit(ev="nav_completed", nav=self.nav, ok=True, status=200)
            self._failed = False

    def on_failed(self, view, event, uri, error):
        self._failed = True
        emit(ev="nav_completed", nav=self.nav, ok=False, status=0)
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
            return True
        elif op == "bounds":
            self.bounds(cmd)
        elif op == "visible":
            (self.window.show_all if cmd.get("on") else self.window.hide)()
        elif op == "navigate":
            self.view.load_uri(cmd.get("uri", "about:blank"))
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
            if "devtools" in cmd:
                settings.set_enable_developer_extras(bool(cmd["devtools"]))
            if "zoom" in cmd:
                self.view.set_zoom_level(float(cmd["zoom"]) or 1.0)
            if "user_agent" in cmd:
                settings.set_user_agent(cmd["user_agent"])
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
    from gi.repository import Gtk, Gdk, GdkX11, GLib, WebKit2
    view = View(Gtk, Gdk, GdkX11, GLib, WebKit2)

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

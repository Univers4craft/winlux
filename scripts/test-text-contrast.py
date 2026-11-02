#!/usr/bin/python3
"""Lisibilité du texte de toutes les fenêtres GTK de Winlux, dans chaque thème.

À lancer dans un affichage X dédié (xvfb-run). Chaque application native
(Paramètres page par page, outils système, Bienvenue, UAC, autorisations,
écran de verrouillage, signalement de plantage, applications par défaut,
installateur…) est ouverte dans un processus neuf pour chaque thème :

    clair, sombre                    thème GTK seul
    clair+perso, sombre+perso        plus le gtk.css écrit par winlux-personalize
                                     (avec chaque couleur d'accentuation proposée)
    contraste, contraste-inverse     thèmes à contraste élevé de Paramètres
                                     > Accessibilité (Désert, Aquatique…)

Une fois la fenêtre affichée, chaque libellé, champ et icône symbolique visible
doit atteindre 4,5:1 contre le fond réellement peint derrière lui (3:1 pour
le gros texte et les libellés désactivés, comme WCAG AA). En allemand, langue
aux mots les plus longs, aucun libellé ne doit être coupé (« … ») ni dépasser
de sa fenêtre.

    test-text-contrast.py                  tous les écrans, tous les thèmes
    test-text-contrast.py --only settings  écrans dont le nom contient « settings »
    test-text-contrast.py --themes clair,sombre+perso   thèmes dont le nom commence ainsi
    test-text-contrast.py --shots DOSSIER  garde une capture de chaque écran
    test-text-contrast.py --list           noms des écrans
"""
import concurrent.futures
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

directory = Path(__file__).resolve().parent
project = directory.parent
python = sys.executable or "/usr/bin/python3"
MINIMUM, LARGE = 4.5, 3.0
# Couleurs d'accentuation proposées par Paramètres > Personnalisation.
ACCENTS = runpy.run_path(str(directory / "winlux-settings-pages.py"), run_name="contrast_accents")["ACCENTS"] \
    if os.environ.get("WINLUX_CONTRAST_CHILD") is None else ()
THEMES = {
    # nom : (thème GTK, sombre, gtk.css de winlux-personalize)
    "clair": ("Adwaita", False, False),
    "sombre": ("Adwaita-dark", True, False),
    "clair+perso": ("Adwaita", False, True),
    "sombre+perso": ("Adwaita-dark", True, True),
    "contraste": ("HighContrast", False, False),
    "contraste-inverse": ("HighContrastInverse", True, False),
}


def screens():
    """(nom, commande, variables) de chaque écran natif vérifié."""
    found = []
    settings = runpy.run_path(str(directory / "winlux-settings.py"), run_name="contrast_list")
    for page in range(len(settings["PAGES"])):
        if page in settings.get("MERGED", {}):
            continue
        found.append((f"settings-{page:02d}", [str(directory / "winlux-settings.py")], {"WINLUX_SETTINGS_PAGE": str(page)}))
    for page in ("quick", "appearance", "apps", "files", "recovery", "tasks", "notifications"):
        found.append((f"tools-{page}", [str(directory / "winlux-system-tools.py"), page], {"WINLUX_TOOLS_WINDOW": "1"}))
    found += [
        ("welcome", [str(directory / "winlux-welcome.py")], {}),
        ("default-apps", [str(directory / "winlux-default-apps.py")], {}),
        ("uac", [str(directory / "winlux-uac.py"), "--app", "setup.exe", "--path", r"C:\Users\test\Downloads\setup.exe"], {}),
        ("permission", [str(directory / "winlux-permission.py"), "ask", "camera", "/tmp/Zoom.exe", "Zoom.exe"], {}),
        ("report", [str(directory / "winlux-report.py")], {}),
        ("claude-code", [str(project / "integration/native/claude-code/winlux-claude-code.py")], {}),
        ("assistant-configurer", [str(directory / "winlux-assistant.py"), "--configurer"], {}),
    ]
    return found


def launch(command, environment):
    try:
        # Bus de session propre : Paramètres est une Gtk.Application à instance unique.
        bus = ["dbus-run-session", "--"] if shutil.which("dbus-run-session") else []
        return subprocess.run(bus + [python, "-B", __file__, "--child", *command], env=environment, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True, timeout=90).stdout
    except subprocess.TimeoutExpired as error:
        output = error.stdout or ""
        return output.decode(errors="replace") if isinstance(output, bytes) else output


def lock_screen_problems():
    """Écran de verrouillage : texte blanc posé sur le fond d'écran. Le voile
    calculé doit laisser l'heure et la date à 4,5:1 sur chaque fond livré."""
    import gi
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf
    lock = runpy.run_path(str(directory / "winlux-lock.py"), run_name="contrast_lock")
    problems = []
    for image in sorted((project / "assets/wallpapers").glob("*.png")):
        pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(image), 320, 180, False)
        for view, base in (("horloge", 0.18), ("connexion", 0.40)):
            alpha = lock["text_veil"](pixbuf, base)
            small = pixbuf.scale_simple(64, 36, GdkPixbuf.InterpType.BILINEAR)
            data, stride, channels = small.get_pixels(), small.get_rowstride(), small.get_n_channels()
            values = sorted(0.2126 * lock["_channel"](data[o] * (1 - alpha)) + 0.7152 * lock["_channel"](data[o + 1] * (1 - alpha))
                            + 0.0722 * lock["_channel"](data[o + 2] * (1 - alpha))
                            for o in (y * stride + x * channels for y in range(36) for x in range(64)))
            ratio = 1.05 / (values[int(len(values) * 0.95)] + 0.05)
            if ratio < MINIMUM:
                problems.append(f"écran de verrouillage ({view}) · fond {image.name} : texte blanc à {ratio:.2f}:1")
    return problems


def personalize_css(dark, accent):
    module = runpy.run_path(str(directory / "winlux-personalize.py"), run_name="contrast_css")
    return module["appearance_css"](dark, accent)


def parent(only, verbose, themes=(), shots=None):
    targets = [s for s in screens() if not only or any(o in s[0] for o in only)]
    failures = []
    with tempfile.TemporaryDirectory() as temp:
        temp = Path(temp)
        shipped = project / "integration/native/config"
        runs = []
        for theme, (gtk_theme, dark, personal) in THEMES.items():
            accents = ACCENTS if personal else (None,)
            for accent in accents:
                runs.append((theme + (f" {accent}" if accent else ""), gtk_theme, dark, personal, accent, "fr"))
        runs.append(("clair (allemand)", "Adwaita", False, False, None, "de"))
        runs.append(("sombre (allemand)", "Adwaita-dark", True, False, None, "de"))
        if themes:
            runs = [run for run in runs if any(run[0].startswith(t) for t in themes)]
        jobs = []
        # Sans bus de session par processus, les instances de Paramètres se rejoindraient : une à la fois.
        workers = int(os.environ.get("WINLUX_CONTRAST_JOBS", "6")) if shutil.which("dbus-run-session") else 1
        pool = concurrent.futures.ThreadPoolExecutor(max_workers=workers)
        for label, gtk_theme, dark, personal, accent, lang in runs:
            home = temp / label.replace(" ", "_").replace("#", "").replace("(", "").replace(")", "")
            config = home / ".config"
            (config / "gtk-3.0").mkdir(parents=True)
            # Thèmes livrés par Winlux (Adwaita-dark), comme dans /usr/share/themes.
            (home / "share").mkdir(parents=True, exist_ok=True)
            shutil.copytree(project / "integration/native/themes", home / "share/themes")
            prefer_dark = dark
            if personal:
                # Ce qu'écrit winlux-personalize : un thème du compte (Adwaita +
                # couleurs Winlux) choisi par son nom, sans préférence sombre.
                mode = "dark" if dark else "light"
                gtk_theme, prefer_dark = f"Winlux-{mode}-1", False
                folder = home / "share/themes" / gtk_theme / "gtk-3.0"
                folder.mkdir(parents=True)
                base = "gtk-contained-dark.css" if dark else "gtk-contained.css"
                folder.joinpath("gtk.css").write_text(
                    f'@import url("resource:///org/gtk/libgtk/theme/Adwaita/{base}");\n'
                    + personalize_css(dark, accent) + "\n", encoding="utf-8")
                (config / "winlux").mkdir(parents=True)
                (config / "winlux/appearance.json").write_text(json.dumps({"theme": mode, "accent": accent}))
            (config / "gtk-3.0/settings.ini").write_text(
                "[Settings]\ngtk-theme-name=%s\ngtk-application-prefer-dark-theme=%d\ngtk-enable-animations=false\n"
                % (gtk_theme, prefer_dark), encoding="utf-8")
            (config / "gtk-3.0/gtk.css").write_text((shipped / "gtk-3.0/gtk.css").read_text(encoding="utf-8"), encoding="utf-8")
            for name, command, extra in targets:
                if accent and accent != ACCENTS[0] and not name.startswith(("settings-00", "settings-08", "tools-quick", "uac", "welcome")):
                    continue  # chaque accent : un échantillon d'écrans suffit
                report = home / f"{name}.json"
                environment = {key: value for key, value in os.environ.items() if key not in ("GTK_THEME", "LANG", "LANGUAGE")}
                environment.update(HOME=str(home), XDG_CONFIG_HOME=str(config), XDG_STATE_HOME=str(home / "state"),
                                   XDG_DATA_HOME=str(home / "share"), XDG_CACHE_HOME=str(home / "cache"),
                                   WINLUX_LANG=lang, LANG="C.UTF-8",
                                   NO_AT_BRIDGE="1", GTK_A11Y="none", WINLUX_CONTRAST_CHILD=str(report),
                                   WINLUX_CONTRAST_LANG=lang, WINLUX_LOCK_ROOT=str(home / "lock"),
                                   WINLUX_PERMISSIONS_FILE=str(home / "permissions.tsv"),
                                   WINLUX_REPORT_CONF=str(home / "report.conf"),
                                   WINLUX_JOURNAL_DIR=str(home / "journal"), WINLUX_RUN_DIR=str(home / "run"),
                                   WINLUX_PSTORE=str(home / "pstore"), WINLUX_SESSION_USER="test", TZ="UTC")
                environment.update(extra)
                if shots:
                    Path(shots).mkdir(parents=True, exist_ok=True)
                    environment["WINLUX_CONTRAST_SHOT"] = str(Path(shots) / f"{name}-{home.name}.png")
                jobs.append((label, name, report, pool.submit(launch, command, environment)))
        for label, name, report, job in jobs:
            output = job.result()
            if not report.is_file():
                failures.append(f"{label} · {name} : aucune fenêtre affichée\n    " + "\n    ".join(output.strip().splitlines()[-6:]))
                continue
            data = json.loads(report.read_text())
            for problem in data["problems"]:
                failures.append(f"{label} · {name} · {problem}")
            if verbose:
                print(f"{label} · {name} : {data['checked']} texte(s) vérifié(s), {len(data['problems'])} problème(s)")
        pool.shutdown()
    failures += lock_screen_problems()
    if failures:
        print("\n".join(failures))
        print(f"\n{len(failures)} texte(s) illisible(s) ou coupé(s)")
        return 1
    print(f"OK : texte lisible dans {len(targets)} écran(s), {len(runs)} thème(s)/langue(s)")
    return 0


# ── Mesure dans l'application (processus enfant) ────────────────────────────

def luminance(rgba):
    def channel(value):
        return value / 12.92 if value <= 0.03928 else ((value + 0.055) / 1.055) ** 2.4
    return 0.2126 * channel(rgba.red) + 0.7152 * channel(rgba.green) + 0.0722 * channel(rgba.blue)


def ratio(first, second):
    lighter, darker = sorted((luminance(first), luminance(second)), reverse=True)
    return (lighter + 0.05) / (darker + 0.05)


def blend(top, bottom):
    result = top.copy()
    alpha = top.alpha
    result.red = top.red * alpha + bottom.red * (1 - alpha)
    result.green = top.green * alpha + bottom.green * (1 - alpha)
    result.blue = top.blue * alpha + bottom.blue * (1 - alpha)
    result.alpha = 1
    return result


def audit(Gtk, Pango, lang):
    """Contraste (et coupures en allemand) de chaque texte visible."""
    problems, checked = [], 0

    def painted_pixels(node):
        """Couleur la plus présente quand le nœud se dessine (dégradés, images)."""
        import cairo
        from gi.repository import Gdk
        width, height = node.get_allocated_width(), node.get_allocated_height()
        if width < 2 or height < 2:
            return None
        surface = cairo.ImageSurface(cairo.FORMAT_RGB24, width, height)
        cr = cairo.Context(surface)
        cr.set_source_rgb(1, 0, 1)
        cr.paint()
        node.draw(cr)
        surface.flush()
        data, stride = surface.get_data(), surface.get_stride()
        counts = {}
        for y in range(0, height, max(1, height // 12)):
            for x in range(0, width, max(1, width // 24)):
                offset = y * stride + x * 4
                pixel = (data[offset + 2], data[offset + 1], data[offset])
                counts[pixel] = counts.get(pixel, 0) + 1
        counts.pop((255, 0, 255), None)
        if not counts:
            return None
        r, g, b = max(counts, key=counts.get)
        color = Gdk.RGBA()
        color.red, color.green, color.blue, color.alpha = r / 255, g / 255, b / 255, 1
        return color

    def background(widget):
        """Couleur réellement peinte derrière le widget : fonds empilés des parents."""
        chain = []
        while widget is not None:
            chain.append(widget)
            widget = widget.get_parent()
        painted = None
        for node in reversed(chain):
            context = node.get_style_context()
            color = context.get_property("background-color", context.get_state())
            image = context.get_property("background-image", context.get_state())
            if painted is None:
                painted = color.copy()
                painted.alpha = 1
            elif color.alpha > 0:
                painted = blend(color, painted)
            if image is not None and node is not chain[0]:
                # Dégradé (boutons Adwaita) : on mesure les pixels réellement dessinés.
                painted = painted_pixels(node) or painted
        return painted

    def walk(widget):
        yield widget
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children():
                yield from walk(child)

    def describe(widget):
        if isinstance(widget, Gtk.Image):
            return widget.get_icon_name()[0] or "icône"
        text = widget.get_text() if hasattr(widget, "get_text") else ""
        return (text or getattr(widget, "get_placeholder_text", lambda: "")() or "").strip()[:60]

    for window in Gtk.Window.list_toplevels():
        if not window.get_mapped() or window.get_window_type() != Gtk.WindowType.TOPLEVEL:
            continue
        window_width = window.get_allocated_width()
        for widget in walk(window):
            if not widget.get_mapped() or not widget.is_drawable():
                continue
            if isinstance(widget, Gtk.Image) and widget.get_storage_type() != Gtk.ImageType.ICON_NAME:
                continue
            if isinstance(widget, Gtk.Image) and not (widget.get_icon_name()[0] or "").endswith("-symbolic"):
                continue  # icônes en couleur
            if not isinstance(widget, (Gtk.Label, Gtk.Entry, Gtk.Image)):
                continue
            if isinstance(widget, Gtk.Label) and not widget.get_text().strip():
                continue
            if isinstance(widget, Gtk.Entry) and not (widget.get_text() or widget.get_placeholder_text()):
                continue
            checked += 1
            context = widget.get_style_context()
            fg = context.get_color(context.get_state())
            bg = background(widget)
            fg = blend(fg, bg)
            value = ratio(fg, bg)
            font = context.get_property("font", context.get_state())
            size = font.get_size() / Pango.SCALE * (96 / 72) if font else 14
            bold = font is not None and font.get_weight() >= Pango.Weight.BOLD
            large = size >= 24 or (bold and size >= 18.66)
            if not widget.is_sensitive():
                continue  # commandes inactives : exemptées par WCAG (Adwaita les grise à 2,8:1)
            limit = LARGE if large else MINIMUM
            # Champ vide : le texte d'exemple n'est pas obligatoire, il reste visible à 3:1.
            if isinstance(widget, Gtk.Entry) and not widget.get_text():
                limit = LARGE
            if value < limit:
                problems.append(f"{type(widget).__name__} « {describe(widget)} » : {value:.2f}:1 "
                                f"(texte {fg.to_string()} sur {bg.to_string()}, minimum {limit})")
            if lang == "de" and isinstance(widget, Gtk.Label):
                layout = widget.get_layout()
                if layout is not None and layout.is_ellipsized():
                    problems.append(f"libellé coupé en allemand : « {describe(widget)} »")
                else:
                    allocation = widget.get_allocation()
                    x = widget.translate_coordinates(window, 0, 0)
                    if x and x[0] + min(allocation.width, widget.get_preferred_width()[1]) > window_width + 1:
                        problems.append(f"libellé hors de la fenêtre en allemand : « {describe(widget)} »")
    return problems, checked


def child(command):
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GLib, Gtk, Pango
    report = Path(os.environ["WINLUX_CONTRAST_CHILD"])
    lang = os.environ.get("WINLUX_CONTRAST_LANG", "fr")
    state = {"tries": 0}

    def measure():
        windows = [w for w in Gtk.Window.list_toplevels()
                   if w.get_mapped() and w.get_window_type() == Gtk.WindowType.TOPLEVEL]
        if not windows and os.environ.get("WINLUX_CONTRAST_POPUP"):
            # Écran de verrouillage : fenêtres « popup » plein écran.
            windows = [w for w in Gtk.Window.list_toplevels() if w.get_mapped() and w.get_allocated_width() > 400]
        state["tries"] += 1
        if not windows and state["tries"] < 60:
            return True  # fenêtre pas encore affichée
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)
        problems, checked = audit(Gtk, Pango, lang) if windows else (["aucune fenêtre"], 0)
        shot = os.environ.get("WINLUX_CONTRAST_SHOT")
        if shot and windows:
            import cairo
            # Rendu hors écran : indépendant des fenêtres voisines sur l'affichage.
            window = max(windows, key=lambda w: w.get_allocated_width() * w.get_allocated_height())
            surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, window.get_allocated_width(),
                                         window.get_allocated_height())
            window.draw(cairo.Context(surface))
            surface.write_to_png(shot)
        report.write_text(json.dumps({"problems": problems, "checked": checked}, ensure_ascii=False))
        sys.stdout.flush()
        os._exit(0)
    GLib.timeout_add(400, measure)
    sys.argv = command
    sys.path.insert(0, str(Path(command[0]).parent))
    runpy.run_path(command[0], run_name="__main__")
    # L'application est sortie d'elle-même sans afficher de fenêtre.
    if not report.exists():
        Gtk.main()


if __name__ == "__main__":
    arguments = sys.argv[1:]
    if arguments[:1] == ["--child"]:
        child(arguments[1:])
    elif arguments[:1] == ["--list"]:
        print("\n".join(name for name, _, _ in screens()))
    else:
        only = arguments[arguments.index("--only") + 1].split(",") if "--only" in arguments else []
        themes = arguments[arguments.index("--themes") + 1].split(",") if "--themes" in arguments else []
        shots = arguments[arguments.index("--shots") + 1] if "--shots" in arguments else None
        raise SystemExit(parent(only, "-v" in arguments, themes, shots))

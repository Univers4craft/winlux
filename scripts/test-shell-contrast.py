#!/usr/bin/python3
"""Contraste du texte du menu Démarrer, des panneaux de la barre, de l'horloge et des boîtes d’arrêt.

À lancer dans un affichage X dédié (xvfb-run). Pour chaque thème (clair et
sombre, avec et sans le gtk.css écrit par winlux-personalize), chaque libellé,
champ de saisie et icône symbolique visible doit atteindre 4,5:1 contre le fond
réellement peint derrière lui (WCAG AA). Le menu est vérifié épinglés, liste
« Toutes les applications » et recherche saisie ; les panneaux volants son
(avec mélangeur par application), alimentation (avec modes) et notifications +
calendrier.

L'horloge de la barre (winlux-clock) est vérifiée deux fois : sa couleur
contre le fond de barre déclaré dans gtk.css, puis, si xfce4-panel et le
greffon genmon sont installés, sur les pixels réellement affichés par la barre
(en thème clair, l'heure était gris foncé sur noir).

    test-shell-contrast.py           (relance chaque cas dans un processus neuf)
    test-shell-contrast.py CAS       (un cas : light, dark, light-user, dark-user)
"""
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import sys
import tempfile

directory = Path(__file__).resolve().parent
MINIMUM = 4.5
CASES = ("light", "dark", "light-user", "dark-user")


def parent_case():
    with tempfile.TemporaryDirectory() as temp:
        temp = Path(temp)
        apps = temp / "share/applications"
        apps.mkdir(parents=True)
        for identifier, name, category in [
                ("winlux-explorer", "Explorateur de fichiers", "System"),
                ("winlux-settings", "Paramètres", "Settings"),
                ("org.xfce.mousepad", "Bloc-notes", "Utility"),
                ("winlux-test-paint", "Paint de test", "Graphics")]:
            (apps / f"{identifier}.desktop").write_text(
                f"[Desktop Entry]\nType=Application\nName={name}\nExec=true\n"
                f"Icon=utilities-terminal\nCategories={category};\n", encoding="utf-8")
        # Processeur simulé (intel_pstate + EPP) : les trois modes d'alimentation.
        cpufreq = temp / "sysfs/sys/devices/system/cpu/cpufreq/policy0"
        cpufreq.mkdir(parents=True)
        for name, value in [("scaling_driver", "intel_pstate"), ("scaling_available_governors", "performance powersave"),
                            ("scaling_governor", "powersave"), ("energy_performance_preference", "balance_performance"),
                            ("energy_performance_available_preferences", "default performance balance_performance balance_power power")]:
            (cpufreq / name).write_text(value + "\n")
        shipped = directory.parent / "integration/native/config"
        failed = False
        for case in CASES:
            config = temp / case / "config"
            (config / "gtk-3.0").mkdir(parents=True)
            dark = case.startswith("dark")
            (config / "gtk-3.0/settings.ini").write_text(
                "[Settings]\ngtk-theme-name=%s\ngtk-application-prefer-dark-theme=%d\n"
                "gtk-enable-animations=false\n" % ("Adwaita-dark" if dark else "Adwaita", dark), encoding="utf-8")
            # gtk.css livré (couleurs de la barre), puis ce qu'ajoute winlux-personalize.
            css = (shipped / "gtk-3.0/gtk.css").read_text(encoding="utf-8")
            if case.endswith("-user"):
                # Ce qu'écrit winlux-personalize theme light|dark (priorité « utilisateur »).
                bg, fg, surface = ("#181b24", "#ecedf5", "#242a38") if dark else ("#f3f5fa", "#202430", "#ffffff")
                css += (f"\n/* Winlux appearance */\nwindow, window box, list, viewport {{background-color:{bg}; color:{fg};}} "
                        f"button {{background:{surface}; color:{fg}; border-color:#8178b7;}} "
                        f"entry {{background:{surface}; color:{fg};}}\n/* End Winlux appearance */\n")
            (config / "gtk-3.0/gtk.css").write_text(css, encoding="utf-8")
            # Barre des tâches réelle : horloge et icônes pointées vers le dépôt.
            shutil.copytree(shipped / "xfce4", config / "xfce4")
            for path in list((config / "xfce4").rglob("*.rc")) + list((config / "xfce4").rglob("*.xml")):
                text = path.read_text(encoding="utf-8")
                text = text.replace("/usr/bin/winlux-clock", f"/bin/sh {directory}/winlux-clock.sh")
                text = re.sub(r"/usr/bin/winlux-tray \w+", "true", text)
                path.write_text(text, encoding="utf-8")
            state = temp / case / "state/winlux"
            state.mkdir(parents=True)
            (state / "notifications.json").write_text(
                '[{"id":1,"app":"Mises à jour Winlux","title":"Mise à jour disponible","body":"Winlux est prête.",'
                '"actions":["install","Installer"],"time":1793520000},'
                '{"id":2,"app":"Firefox","title":"Téléchargement terminé","body":"setup.exe enregistré.","actions":[],"time":1793541600}]',
                encoding="utf-8")
            environment = {key: value for key, value in os.environ.items() if key != "GTK_THEME"}
            environment.update(XDG_CONFIG_HOME=str(config), HOME=str(temp / case), XDG_DATA_HOME=str(temp / "share"),
                               XDG_DATA_DIRS="/usr/share", WINLUX_LANG="fr", NO_AT_BRIDGE="1", GTK_A11Y="none",
                               XDG_STATE_HOME=str(temp / case / "state"), WINLUX_TODAY="2026-11-01", TZ="UTC",
                               WINLUX_POWER_ROOT=str(temp / "sysfs"))
            result = subprocess.run([sys.executable, "-B", __file__, case], env=environment)
            failed |= result.returncode != 0
        raise SystemExit(1 if failed else 0)


def child_case(case):
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk, GLib

    def luminance(rgba):
        def channel(value):
            return value / 12.92 if value <= 0.03928 else ((value + 0.055) / 1.055) ** 2.4
        return 0.2126 * channel(rgba.red) + 0.7152 * channel(rgba.green) + 0.0722 * channel(rgba.blue)

    def blend(top, bottom):
        alpha = top.alpha
        result = top.copy()
        result.red = top.red * alpha + bottom.red * (1 - alpha)
        result.green = top.green * alpha + bottom.green * (1 - alpha)
        result.blue = top.blue * alpha + bottom.blue * (1 - alpha)
        result.alpha = 1
        return result

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
            if painted is None:
                painted = color.copy(); painted.alpha = 1
            elif color.alpha > 0:
                painted = blend(color, painted)
        return painted

    def walk(widget):
        yield widget
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children():
                yield from walk(child)

    problems = []

    def check(window, where):
        for widget in walk(window):
            if not widget.get_mapped():
                continue
            if isinstance(widget, Gtk.Image) and widget.get_storage_type() != Gtk.ImageType.ICON_NAME:
                continue  # icônes en couleur : pas de couleur de texte
            if not isinstance(widget, (Gtk.Label, Gtk.Entry, Gtk.Image)):
                continue
            if isinstance(widget, Gtk.Label) and not widget.get_text().strip():
                continue
            context = widget.get_style_context()
            fg = context.get_color(context.get_state())
            bg = background(widget)
            fg = blend(fg, bg)
            lighter, darker = sorted((luminance(fg), luminance(bg)), reverse=True)
            ratio = (lighter + 0.05) / (darker + 0.05)
            # Libellés désactivés (bouton « Muet » sans sortie audio) : 3:1 suffit (WCAG les exempte).
            limit = 3.0 if not widget.is_sensitive() else MINIMUM
            if ratio < limit:
                text = widget.get_text() if hasattr(widget, "get_text") else widget.get_icon_name()[0]
                problems.append(f"{case} · {where} · {type(widget).__name__} « {text} » : {ratio:.2f}:1 "
                                f"(texte {fg.to_string()} sur {bg.to_string()})")

    def settle():
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)

    module = runpy.run_path(str(directory / "winlux-start-menu.py"), run_name="contrast_test")
    start = module["Start"]()

    def run_start():
        settle()  # run() a déjà ouvert le menu (do_activate)
        check(start.window, "menu Démarrer épinglés")
        start.toggle_list(); settle()
        check(start.window, "menu Démarrer toutes les applications")
        start.search.set_text("Para"); start.render(); settle()
        check(start.window, "menu Démarrer recherche")
        start.hide(); start.quit()
    GLib.timeout_add(300, run_start)
    start.run([])

    for kind, audio_state in (("volume", "absent"), ("volume", "mixer"), ("power", None), ("calendar", None)):
        popup_module = runpy.run_path(str(directory / "winlux-popup.py"), run_name="contrast_test")
        popup = popup_module["Popup"](kind)
        module_globals = popup_module["Popup"].refresh.__globals__
        if audio_state == "mixer":
            # Sortie présente et deux applications qui jouent du son.
            module_globals["audio"] = lambda: ("pactl", 64, False, "Haut-parleurs")
            module_globals["app_streams"] = lambda: [(7, "Firefox", "firefox", 40, False), (9, "Spotify.exe", "", 100, True)]
        elif audio_state == "absent":
            module_globals["audio"] = lambda: (None, 0, False, "")
            module_globals["app_streams"] = lambda: []

        def run_popup(popup=popup, kind=kind, audio_state=audio_state):
            popup.refresh(); settle()
            if kind == "power":
                GLib.timeout_add(400, finish_popup, popup, kind, audio_state)  # détection des modes en arrière-plan
            else:
                finish_popup(popup, kind, audio_state)
            return False

        def finish_popup(popup, kind, audio_state):
            popup.refresh(); settle()
            if kind == "power" and not popup.mode_buttons:
                problems.append(f"{case} · panneau alimentation : modes du processeur simulé non affichés")
            if audio_state == "mixer" and len(popup.app_rows) != 2:
                problems.append(f"{case} · panneau son : {len(popup.app_rows)} barre(s) d'application au lieu de 2")
            if audio_state == "absent" and popup.output.get_visible():
                problems.append(f"{case} · panneau son : curseur affiché sans sortie audio")
            if kind == "calendar":
                popup.shift_month(1); settle()  # navigation : décembre, jours d'autres mois atténués
                popup.go_today(); settle()
            check(popup.window, f"panneau {kind}" + (f" ({audio_state})" if audio_state else ""))
            popup.hide(); popup.quit()
            return False
        GLib.timeout_add(300, run_popup)
        popup.run([])

    # Boîtes d'arrêt (winlux-power) : confirmation, applications bloquantes, erreur.
    power_module = runpy.run_path(str(directory / "winlux-power.py"), run_name="contrast_test")
    for action in ("shutdown", "reboot"):
        ui = power_module["PowerUI"](action)
        settle()
        check(ui.window, f"boîte {action} confirmation")
        ui.show_blocked({"0x1": ("mousepad.Mousepad", "Rapport.txt - Bloc-notes", "1")})
        settle()
        check(ui.window, f"boîte {action} applications bloquantes")
        ui.show_error(power_module["UPDATE_BUSY"])
        settle()
        check(ui.window, f"boîte {action} erreur")
        ui.ungrab()
        ui.window.destroy()
        settle()

    check_clock(case, problems, luminance)

    if problems:
        print("\n".join(problems))
        raise SystemExit(1)
    print(f"OK: {case}, tout le texte du menu Démarrer, des panneaux de la barre, de l'horloge et des boîtes d’arrêt dépasse {MINIMUM}:1")


def hex_luminance(value, luminance):
    from gi.repository import Gdk
    color = Gdk.RGBA()
    assert color.parse(value), value
    return luminance(color)


def ratio_of(first, second):
    lighter, darker = sorted((first, second), reverse=True)
    return (lighter + 0.05) / (darker + 0.05)


def check_clock(case, problems, luminance):
    """Horloge de la barre : couleur déclarée, puis pixels réellement affichés."""
    import time
    output = subprocess.run(["/bin/sh", str(directory / "winlux-clock.sh")], capture_output=True, text=True).stdout
    found = re.search(r'foreground="(#[0-9a-fA-F]{6})"', output)
    css = (Path(os.environ["XDG_CONFIG_HOME"]) / "gtk-3.0/gtk.css").read_text(encoding="utf-8")
    panel = re.search(r"\.xfce4-panel\s*\{[^}]*background:\s*(#[0-9a-fA-F]{6})", css)
    if not found or not panel:
        problems.append(f"{case} · horloge : couleur du texte ou fond de barre introuvable")
        return
    declared = ratio_of(hex_luminance(found[1], luminance), hex_luminance(panel[1], luminance))
    if declared < MINIMUM:
        problems.append(f"{case} · horloge : {found[1]} sur {panel[1]} = {declared:.2f}:1")
    if not (shutil.which("xfce4-panel") and shutil.which("dbus-run-session")):
        print(f"{case} · barre réelle non vérifiée : xfce4-panel absent")
        return
    import gi
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk
    process = subprocess.Popen(["dbus-run-session", "--", "xfce4-panel", "--disable-wm-check"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    try:
        root = Gdk.get_default_root_window()
        width, height = root.get_width(), root.get_height()
        best, strong, background = 0.0, 0, None
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            time.sleep(1)
            pixbuf = Gdk.pixbuf_get_from_window(root, width - 80, height - 40, 80, 40)
            if pixbuf is None:
                continue
            data, stride, channels = pixbuf.get_pixels(), pixbuf.get_rowstride(), pixbuf.get_n_channels()
            pixels = [tuple(data[y * stride + x * channels:y * stride + x * channels + 3])
                      for y in range(40) for x in range(80)]
            counts = {}
            for pixel in pixels:
                counts[pixel] = counts.get(pixel, 0) + 1
            background = max(counts, key=counts.get)
            if len(counts) < 3 or background == (0, 0, 0):
                continue  # barre pas encore peinte

            def lum(pixel):
                color = Gdk.RGBA()
                color.red, color.green, color.blue = (value / 255 for value in pixel)
                return luminance(color)
            base = lum(background)
            ratios = [ratio_of(lum(pixel), base) for pixel in counts]
            best = max(ratios)
            strong = sum(counts[pixel] for pixel, value in zip(counts, ratios) if value >= MINIMUM)
            if strong >= 30:
                break
        if strong < 30:
            problems.append(f"{case} · horloge de la barre illisible : meilleur contraste {best:.2f}:1, "
                            f"{strong} pixel(s) au-dessus de {MINIMUM}:1 (fond {background})")
    finally:
        try:
            os.killpg(process.pid, 15)
        except ProcessLookupError:
            pass
        process.wait(timeout=10)


if __name__ == "__main__":
    if len(sys.argv) > 1:
        child_case(sys.argv[1])
    else:
        parent_case()

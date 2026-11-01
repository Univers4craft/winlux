#!/usr/bin/python3
"""Contraste du texte du menu Démarrer et des fenêtres volume/alimentation.

À lancer dans un affichage X dédié (xvfb-run). Pour chaque thème (clair et
sombre, avec et sans le gtk.css écrit par winlux-personalize), chaque libellé,
champ de saisie et icône symbolique visible doit atteindre 4,5:1 contre le fond
réellement peint derrière lui (WCAG AA). Le menu est vérifié épinglés, liste
« Toutes les applications » et recherche saisie.

    test-shell-contrast.py           (relance chaque cas dans un processus neuf)
    test-shell-contrast.py CAS       (un cas : light, dark, light-user, dark-user)
"""
import os
from pathlib import Path
import runpy
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
        failed = False
        for case in CASES:
            config = temp / case / "config"
            (config / "gtk-3.0").mkdir(parents=True)
            dark = case.startswith("dark")
            (config / "gtk-3.0/settings.ini").write_text(
                "[Settings]\ngtk-theme-name=%s\ngtk-application-prefer-dark-theme=%d\n"
                "gtk-enable-animations=false\n" % ("Adwaita-dark" if dark else "Adwaita", dark), encoding="utf-8")
            if case.endswith("-user"):
                # Ce qu'écrit winlux-personalize theme light|dark (priorité « utilisateur »).
                bg, fg, surface = ("#181b24", "#ecedf5", "#242a38") if dark else ("#f3f5fa", "#202430", "#ffffff")
                (config / "gtk-3.0/gtk.css").write_text(
                    f"window, window box, list, viewport {{background-color:{bg}; color:{fg};}} "
                    f"button {{background:{surface}; color:{fg}; border-color:#8178b7;}} "
                    f"entry {{background:{surface}; color:{fg};}}", encoding="utf-8")
            environment = {key: value for key, value in os.environ.items() if key != "GTK_THEME"}
            environment.update(XDG_CONFIG_HOME=str(config), HOME=str(temp / case), XDG_DATA_HOME=str(temp / "share"),
                               XDG_DATA_DIRS="/usr/share", WINLUX_LANG="fr", NO_AT_BRIDGE="1", GTK_A11Y="none")
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

    for kind in ("volume", "power"):
        popup_module = runpy.run_path(str(directory / "winlux-popup.py"), run_name="contrast_test")
        popup = popup_module["Popup"](kind)

        def run_popup(popup=popup, kind=kind):
            popup.refresh(); settle()
            check(popup.window, f"fenêtre {kind}")
            popup.hide(); popup.quit()
        GLib.timeout_add(300, run_popup)
        popup.run([])

    if problems:
        print("\n".join(problems))
        raise SystemExit(1)
    print(f"OK: {case}, tout le texte du menu Démarrer et des fenêtres volume/alimentation dépasse {MINIMUM}:1")


if __name__ == "__main__":
    if len(sys.argv) > 1:
        child_case(sys.argv[1])
    else:
        parent_case()

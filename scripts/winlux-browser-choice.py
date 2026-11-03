#!/usr/bin/python3
"""Choix du navigateur pour ouvrir un lien, tant qu'aucun n'est par défaut.

Appelé par winlux-browser quand plusieurs navigateurs sont installés et
qu'aucun n'a été choisi par défaut. Arguments : les navigateurs installés,
« catalogue:<id> » (Chrome, Edge, Firefox) ou « systeme:<commande> ».
Écrit le choix sur la sortie standard, précédé de « toujours » si
l'utilisateur en fait son navigateur par défaut ; code 1 si Annuler.
Aucun navigateur n'est mis en avant : l'ordre reçu est alphabétique et rien
n'est présélectionné.
"""
import os
from pathlib import Path
import sys

ROOT = os.environ.get("WINLUX_ROOT", "")


def interface_lang():
    """WINLUX_LANG, sinon /etc/winlux/locale.conf (Paramètres > Langue), sinon LANG."""
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in Path(ROOT + "/etc/winlux/locale.conf").read_text().splitlines():
                if line.startswith("LANG="): code = line[5:].strip()
        except OSError: pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


LANG = interface_lang()
# Texte français -> (anglais, espagnol, allemand).
_TR = {
    "Ouvrir le lien": ("Open link", "Abrir el enlace", "Link öffnen"),
    "Avec quel navigateur voulez-vous ouvrir ce lien ?": ("Which browser do you want to open this link with?",
        "¿Con qué navegador quiere abrir este enlace?", "Mit welchem Browser möchten Sie diesen Link öffnen?"),
    "Nous vous recommandons de choisir un navigateur par défaut, pour ne pas avoir à choisir à chaque fois.": (
        "We recommend choosing a default browser, so you don't have to choose every time.",
        "Le recomendamos elegir un navegador predeterminado para no tener que elegir cada vez.",
        "Wir empfehlen, einen Standardbrowser festzulegen, damit Sie nicht jedes Mal wählen müssen."),
    "Toujours utiliser ce navigateur (navigateur par défaut)": ("Always use this browser (default browser)",
        "Usar siempre este navegador (navegador predeterminado)", "Immer diesen Browser verwenden (Standardbrowser)"),
    "Ouvrir": ("Open", "Abrir", "Öffnen"),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "{name} (système)": ("{name} (system)", "{name} (sistema)", "{name} (System)"),
}


def _t(text, **values):
    columns = {"en": 0, "es": 1, "de": 2}
    if LANG in columns and text in _TR: text = _TR[text][columns[LANG]]
    return text.format(**values) if values else text


CATALOGUE = {"chrome": "Google Chrome", "edge": "Microsoft Edge", "firefox": "Mozilla Firefox"}
SYSTEM = {"firefox": "Mozilla Firefox", "chromium": "Chromium", "chromium-browser": "Chromium",
          "google-chrome-stable": "Google Chrome", "microsoft-edge-stable": "Microsoft Edge",
          "microsoft-edge": "Microsoft Edge"}
ICONS = {"chrome": "google-chrome", "edge": "microsoft-edge", "firefox": "firefox",
         "chromium": "chromium", "chromium-browser": "chromium",
         "google-chrome-stable": "google-chrome", "microsoft-edge-stable": "microsoft-edge"}


def entries(arguments):
    """(valeur, libellé, icône, peut devenir le défaut) pour chaque navigateur reçu."""
    result = []
    for value in arguments:
        kind, _, name = value.partition(":")
        if kind == "catalogue" and name in CATALOGUE:
            result.append((value, CATALOGUE[name], ICONS[name], True))
        elif kind == "systeme" and name:
            label = _t("{name} (système)", name=SYSTEM.get(name, name))
            result.append((value, label, ICONS.get(name, "web-browser"), False))
    return result


def answer(value, always):
    """Ligne rendue à winlux-browser."""
    return f"toujours {value}" if always and value.startswith("catalogue:") else value


def main(arguments):
    choices = entries(arguments)
    if not choices: return 1
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk

    dialog = Gtk.Dialog(title=_t("Ouvrir le lien"))
    dialog.set_icon_name("web-browser")
    dialog.set_keep_above(True)
    dialog.set_position(Gtk.WindowPosition.CENTER)
    dialog.set_default_size(420, -1)
    dialog.add_button(_t("Annuler"), Gtk.ResponseType.CANCEL)
    ok = dialog.add_button(_t("Ouvrir"), Gtk.ResponseType.OK)
    ok.get_style_context().add_class("suggested-action")
    ok.set_sensitive(False)
    box = dialog.get_content_area()
    box.set_spacing(10)
    box.set_border_width(16)
    title = Gtk.Label(xalign=0, wrap=True)
    title.set_markup("<b>" + _t("Avec quel navigateur voulez-vous ouvrir ce lien ?") + "</b>")
    box.pack_start(title, False, False, 0)

    theme = Gtk.IconTheme.get_default()
    listbox = Gtk.ListBox()
    listbox.set_selection_mode(Gtk.SelectionMode.SINGLE)
    for value, label, icon, _ in choices:
        row = Gtk.ListBoxRow()
        line = Gtk.Box(spacing=12, margin=8)
        if not theme.has_icon(icon): icon = "web-browser"
        line.pack_start(Gtk.Image.new_from_icon_name(icon, Gtk.IconSize.DND), False, False, 0)
        line.pack_start(Gtk.Label(label=label, xalign=0), True, True, 0)
        row.add(line)
        row.value = value
        listbox.add(row)
    frame = Gtk.Frame()
    frame.add(listbox)
    box.pack_start(frame, False, False, 0)

    always = Gtk.CheckButton(label=_t("Toujours utiliser ce navigateur (navigateur par défaut)"))
    always.set_sensitive(False)
    box.pack_start(always, False, False, 0)
    hint = Gtk.Label(label=_t("Nous vous recommandons de choisir un navigateur par défaut, pour ne pas avoir à choisir à chaque fois."),
                     xalign=0, wrap=True, max_width_chars=50)
    hint.get_style_context().add_class("dim-label")
    box.pack_start(hint, False, False, 0)

    def selected(_listbox, row):
        ok.set_sensitive(row is not None)
        # Seuls les navigateurs du catalogue peuvent devenir le navigateur par défaut.
        can = row is not None and row.value.startswith("catalogue:")
        always.set_sensitive(can)
        if not can: always.set_active(False)
    listbox.connect("row-selected", selected)
    listbox.connect("row-activated", lambda *_: dialog.response(Gtk.ResponseType.OK))

    dialog.show_all()
    # Rien n'est présélectionné : aucun navigateur n'est mis en avant.
    listbox.unselect_all()
    response = dialog.run()
    row = listbox.get_selected_row()
    if response != Gtk.ResponseType.OK or row is None: return 1
    print(answer(row.value, always.get_active()))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

#!/usr/bin/python3
"""Renommer plusieurs fichiers d'un coup, avec un aperçu avant de valider.

Ouvert par l'Explorateur (« Renommer plusieurs fichiers… » quand plusieurs
éléments sont sélectionnés) ou par Thunar (clic droit). Règles cumulables :
rechercher / remplacer (texte ou expression régulière, casse respectée ou
non), nouveau nom numéroté, préfixe, suffixe, casse du nom, casse de
l'extension. L'aperçu signale les noms vides, les caractères interdits par
Windows, les doublons et les fichiers qui existent déjà ; rien n'est renommé
tant qu'un problème reste. Le dernier renommage peut être annulé.

    winlux-rename FICHIER...           fenêtre de renommage
    winlux-rename DOSSIER              tous les fichiers du dossier
    winlux-rename preview JSON FICHIER...  aperçu en texte (tests)
    winlux-rename apply JSON FICHIER...    renomme sans fenêtre
    winlux-rename undo                 annule le dernier renommage

JSON : {"find", "replace", "regex", "match_case", "base", "start", "digits",
"prefix", "suffix", "case": keep|lower|upper|title, "ext": keep|lower|upper,
"order": selection|name|date}. Journal d'annulation :
~/.local/state/winlux/rename-undo.json.
"""
import json
import os
from pathlib import Path
import re
import sys
import uuid


def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="): code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}
_TR = {
    'Renommer plusieurs fichiers': ('Rename multiple files', 'Cambiar el nombre de varios archivos', 'Mehrere Dateien umbenennen'),
    'Rechercher': ('Search for', 'Buscar', 'Suchen nach'),
    'Remplacer par': ('Replace with', 'Reemplazar por', 'Ersetzen durch'),
    'Expression régulière': ('Use regular expressions', 'Expresión regular', 'Regulärer Ausdruck'),
    'Respecter la casse': ('Match case', 'Coincidir mayúsculas y minúsculas', 'Groß-/Kleinschreibung beachten'),
    'Nouveau nom numéroté': ('Numbered new name', 'Nuevo nombre numerado', 'Neuer nummerierter Name'),
    'Laisser vide pour garder les noms': ('Leave empty to keep the names', 'Dejar vacío para conservar los nombres', 'Leer lassen, um die Namen zu behalten'),
    'À partir de': ('Start at', 'Empezar en', 'Beginnen bei'),
    'Chiffres': ('Digits', 'Dígitos', 'Ziffern'),
    'Préfixe': ('Prefix', 'Prefijo', 'Präfix'),
    'Suffixe': ('Suffix', 'Sufijo', 'Suffix'),
    'Casse du nom': ('Name case', 'Mayúsculas del nombre', 'Schreibweise des Namens'),
    'Extension': ('Extension', 'Extensión', 'Erweiterung'),
    'Ordre': ('Order', 'Orden', 'Reihenfolge'),
    'Inchangée': ('Unchanged', 'Sin cambios', 'Unverändert'),
    'minuscules': ('lowercase', 'minúsculas', 'Kleinbuchstaben'),
    'MAJUSCULES': ('UPPERCASE', 'MAYÚSCULAS', 'GROSSBUCHSTABEN'),
    'Première Lettre En Majuscule': ('Title Case', 'Primera Letra En Mayúscula', 'Erster Buchstabe Groß'),
    'Sélection': ('Selection', 'Selección', 'Auswahl'),
    'Nom': ('Name', 'Nombre', 'Name'),
    'Date de modification': ('Date modified', 'Fecha de modificación', 'Änderungsdatum'),
    'Nom actuel': ('Original', 'Nombre actual', 'Ursprünglicher Name'),
    'Nouveau nom': ('Renamed', 'Nuevo nombre', 'Neuer Name'),
    'Renommer': ('Rename', 'Cambiar nombre', 'Umbenennen'),
    'Fermer': ('Close', 'Cerrar', 'Schließen'),
    'Annuler le dernier renommage': ('Undo last rename', 'Deshacer el último cambio de nombre', 'Letztes Umbenennen rückgängig machen'),
    'nom vide': ('empty name', 'nombre vacío', 'leerer Name'),
    'caractère interdit': ('invalid character', 'carácter no válido', 'ungültiges Zeichen'),
    'nom en double': ('duplicate name', 'nombre duplicado', 'doppelter Name'),
    'existe déjà': ('already exists', 'ya existe', 'existiert bereits'),
    'expression régulière invalide': ('invalid regular expression', 'expresión regular no válida', 'ungültiger regulärer Ausdruck'),
    '{count} fichiers seront renommés.': ('{count} files will be renamed.', 'Se cambiará el nombre de {count} archivos.', '{count} Dateien werden umbenannt.'),
    '{count} fichiers renommés.': ('{count} files renamed.', 'Se cambió el nombre de {count} archivos.', '{count} Dateien umbenannt.'),
    'Aucun changement pour l’instant.': ('No changes yet.', 'Todavía no hay cambios.', 'Noch keine Änderungen.'),
    'Corrigez les noms marqués en rouge.': ('Fix the names marked in red.', 'Corrija los nombres marcados en rojo.', 'Korrigieren Sie die rot markierten Namen.'),
    'Renommage annulé : {count} fichiers ont retrouvé leur nom.': (
        'Rename undone: {count} files got their names back.',
        'Cambio deshecho: {count} archivos recuperaron su nombre.',
        'Umbenennen rückgängig: {count} Dateien haben ihren Namen zurück.'),
    'Certains fichiers n’ont pas pu être renommés.': ('Some files could not be renamed.', 'No se pudo cambiar el nombre de algunos archivos.', 'Einige Dateien konnten nicht umbenannt werden.'),
}


def _t(text, **values):
    row = _TR.get(text)
    text = row[_COL[_LANG]] if row and _LANG in _COL else text
    return text.format(**values) if values else text


DEFAULTS = {"find": "", "replace": "", "regex": False, "match_case": False, "base": "", "start": 1,
            "digits": 1, "prefix": "", "suffix": "", "case": "keep", "ext": "keep", "order": "selection"}
# Caractères refusés par Windows dans un nom (les .exe doivent pouvoir l'ouvrir).
FORBIDDEN = set('\\/:*?"<>|') | {chr(c) for c in range(32)}
RESERVED = {"CON", "PRN", "AUX", "NUL"} | {f"COM{n}" for n in range(1, 10)} | {f"LPT{n}" for n in range(1, 10)}


def undo_file():
    base = Path(os.environ.get("XDG_STATE_HOME") or Path.home() / ".local/state")
    return base / "winlux/rename-undo.json"


def split_name(name):
    """('photo.tar', '.gz') ; un nom qui commence par un point n'a pas d'extension."""
    stem, dot, ext = name.rpartition(".")
    if not dot or not stem: return name, ""
    return stem, "." + ext


def change_case(text, mode):
    if mode == "lower": return text.lower()
    if mode == "upper": return text.upper()
    if mode == "title": return re.sub(r"[^\W_]+", lambda m: m.group(0)[:1].upper() + m.group(0)[1:].lower(), text)
    return text


def new_name(name, index, opts):
    """Nouveau nom d'un fichier, à la place `index` de la liste (0 pour le premier)."""
    o = dict(DEFAULTS, **opts)
    stem, ext = split_name(name)
    if o["find"]:
        flags = 0 if o["match_case"] else re.IGNORECASE
        pattern = o["find"] if o["regex"] else re.escape(o["find"])
        replacement = o["replace"] if o["regex"] else o["replace"].replace("\\", "\\\\")
        stem = re.sub(pattern, replacement, stem, flags=flags)
    if o["base"]:
        number = str(int(o["start"]) + index).zfill(max(1, int(o["digits"])))
        stem = f"{o['base']} ({number})" if "#" not in o["base"] else o["base"].replace("#", number, 1)
    stem = o["prefix"] + change_case(stem, o["case"]) + o["suffix"]
    if o["ext"] in ("lower", "upper"): ext = change_case(ext, o["ext"])
    return stem + ext


def ordered(paths, order):
    paths = [Path(p) for p in paths]
    if order == "name": return sorted(paths, key=lambda p: p.name.casefold())
    if order == "date":
        def mtime(p):
            try: return p.stat().st_mtime
            except OSError: return 0
        return sorted(paths, key=mtime)
    return paths


def plan(paths, opts):
    """[(chemin, nouveau nom, problème ou '')] dans l'ordre choisi."""
    o = dict(DEFAULTS, **opts)
    if o["regex"] and o["find"]:
        try: re.compile(o["find"])
        except re.error: return [(p, p.name, "expression régulière invalide") for p in ordered(paths, o["order"])]
    files = ordered(paths, o["order"])
    rows = [(p, new_name(p.name, i, o)) for i, p in enumerate(files)]
    moving = {str(p) for p, name in rows if name != p.name}
    seen = {}
    for p, name in rows:
        seen.setdefault((str(p.parent), name.casefold()), []).append(p)
    result = []
    for p, name in rows:
        problem = ""
        if name == p.name:
            pass
        elif not name.strip(" .") or (name.startswith(".") and not p.name.startswith(".")):
            problem = "nom vide"
        elif set(name) & FORBIDDEN or split_name(name)[0].upper() in RESERVED or name.endswith((" ", ".")):
            problem = "caractère interdit"
        elif len(seen[(str(p.parent), name.casefold())]) > 1:
            problem = "nom en double"
        else:
            target = p.parent / name
            # Un fichier existant qui change lui aussi de nom libère sa place ;
            # changer seulement la casse d'un nom reste permis.
            if (target.exists() or target.is_symlink()) and str(target) not in moving \
                    and name.casefold() != p.name.casefold():
                problem = "existe déjà"
        result.append((p, name, problem))
    return result


def execute(rows):
    """Renomme en deux temps (noms provisoires) pour permettre les échanges.

    Renvoie ([(nouveau chemin, ancien chemin)], échecs) et garde le journal d'annulation."""
    changes = [(p, p.parent / name) for p, name, problem in rows if name != p.name and not problem]
    if any(problem for _, _, problem in rows): return [], len(rows)
    temporary, done, failed = [], [], 0
    for source, target in changes:
        step = source.parent / f".winlux-rename-{uuid.uuid4().hex[:8]}"
        try:
            os.rename(source, step)
            temporary.append((source, step, target))
        except OSError:
            failed += 1
    for source, step, target in temporary:
        try:
            if target.exists() and target.name.casefold() != source.name.casefold(): raise FileExistsError
            os.rename(step, target)
            done.append((str(target), str(source)))
        except OSError:
            os.rename(step, source)
            failed += 1
    if done:
        path = undo_file()
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(done, ensure_ascii=False), encoding="utf-8")
        except OSError:
            pass
    return done, failed


def can_undo():
    try: return bool(json.loads(undo_file().read_text(encoding="utf-8")))
    except (OSError, ValueError): return False


def undo():
    """Remet les noms d'avant le dernier renommage ; renvoie (remis, échecs)."""
    try: pairs = json.loads(undo_file().read_text(encoding="utf-8"))
    except (OSError, ValueError): return 0, 0
    rows = [(Path(current), Path(before).name, "") for current, before in pairs if Path(current).exists()]
    for current, before in pairs:
        if Path(before).parent != Path(current).parent: return 0, len(pairs)
    restored, failed = execute(rows)
    restored = len(restored)
    try: undo_file().unlink()
    except OSError: pass
    return restored, failed + len(pairs) - len(rows)


def expand(args):
    """Un seul dossier : ses fichiers ; sinon les chemins donnés (fichiers ou dossiers)."""
    paths = [Path(a[7:] if a.startswith("file://") else a) for a in args]
    if len(paths) == 1 and paths[0].is_dir():
        return sorted((p for p in paths[0].iterdir() if not p.name.startswith(".")), key=lambda p: p.name.casefold())
    return [p for p in paths if p.exists() or p.is_symlink()]


# ── Fenêtre ─────────────────────────────────────────────────────────────────

def window(paths):
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GLib, Gtk
    win = Gtk.Window(title=_t("Renommer plusieurs fichiers"))
    win.set_role("winlux-rename")
    win.set_icon_name("edit-rename")
    win.set_default_size(820, 560)
    win.set_position(Gtk.WindowPosition.CENTER)
    outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=14)
    win.add(outer)
    body = Gtk.Box(spacing=14)
    outer.pack_start(body, True, True, 0)
    form = Gtk.Grid(row_spacing=8, column_spacing=8)
    body.pack_start(form, False, False, 0)
    widgets = {}
    row = 0

    def label(text):
        nonlocal row
        form.attach(Gtk.Label(label=text, xalign=0), 0, row, 1, 1)

    def entry(key, text, placeholder=""):
        nonlocal row
        label(text)
        widget = Gtk.Entry(width_chars=22, placeholder_text=placeholder)
        widget.connect("changed", refresh)
        form.attach(widget, 1, row, 2, 1)
        widgets[key] = widget
        row += 1

    def check(key, text):
        nonlocal row
        widget = Gtk.CheckButton(label=text)
        widget.connect("toggled", refresh)
        form.attach(widget, 1, row, 2, 1)
        widgets[key] = widget
        row += 1

    def combo(key, text, choices):
        nonlocal row
        label(text)
        widget = Gtk.ComboBoxText()
        for value, title in choices: widget.append(value, title)
        widget.set_active(0)
        widget.connect("changed", refresh)
        form.attach(widget, 1, row, 2, 1)
        widgets[key] = widget
        row += 1

    def refresh(*_):
        GLib.idle_add(update)

    entry("find", _t("Rechercher"))
    entry("replace", _t("Remplacer par"))
    check("regex", _t("Expression régulière"))
    check("match_case", _t("Respecter la casse"))
    form.attach(Gtk.Separator(), 0, row, 3, 1)
    row += 1
    entry("base", _t("Nouveau nom numéroté"), _t("Laisser vide pour garder les noms"))
    for key, title, low, high in (("start", _t("À partir de"), 0, 99999), ("digits", _t("Chiffres"), 1, 6)):
        label(title)
        spin = Gtk.SpinButton.new_with_range(low, high, 1)
        spin.set_value(1)
        spin.connect("value-changed", refresh)
        form.attach(spin, 1, row, 1, 1)
        widgets[key] = spin
        row += 1
    entry("prefix", _t("Préfixe"))
    entry("suffix", _t("Suffixe"))
    cases = [("keep", _t("Inchangée")), ("lower", _t("minuscules")), ("upper", _t("MAJUSCULES"))]
    combo("case", _t("Casse du nom"), cases + [("title", _t("Première Lettre En Majuscule"))])
    combo("ext", _t("Extension"), cases)
    combo("order", _t("Ordre"), [("selection", _t("Sélection")), ("name", _t("Nom")), ("date", _t("Date de modification"))])

    store = Gtk.ListStore(str, str, str, str)
    view = Gtk.TreeView(model=store)
    for number, title in ((0, _t("Nom actuel")), (1, _t("Nouveau nom"))):
        cell = Gtk.CellRendererText(ellipsize=3)
        colors = {"foreground": 3} if number else {}
        column = Gtk.TreeViewColumn(title, cell, text=number, **colors)
        column.set_expand(True)
        column.set_resizable(True)
        view.append_column(column)
    problem_cell = Gtk.CellRendererText()
    view.append_column(Gtk.TreeViewColumn("", problem_cell, text=2, foreground=3))
    scroll = Gtk.ScrolledWindow()
    scroll.add(view)
    body.pack_start(scroll, True, True, 0)
    status = Gtk.Label(xalign=0)
    outer.pack_start(status, False, False, 0)
    buttons = Gtk.Box(spacing=8)
    outer.pack_start(buttons, False, False, 0)
    undo_button = Gtk.Button(label=_t("Annuler le dernier renommage"))
    buttons.pack_start(undo_button, False, False, 0)
    close = Gtk.Button(label=_t("Fermer"))
    rename = Gtk.Button(label=_t("Renommer"))
    rename.get_style_context().add_class("suggested-action")
    buttons.pack_end(rename, False, False, 0)
    buttons.pack_end(close, False, False, 0)
    current = {"paths": list(paths), "rows": []}

    def options():
        values = {}
        for key, widget in widgets.items():
            if isinstance(widget, Gtk.SpinButton): values[key] = int(widget.get_value())
            elif isinstance(widget, Gtk.CheckButton): values[key] = widget.get_active()
            elif isinstance(widget, Gtk.ComboBoxText): values[key] = widget.get_active_id()
            else: values[key] = widget.get_text()
        return values

    def update():
        rows = plan(current["paths"], options())
        current["rows"] = rows
        store.clear()
        for p, name, problem in rows:
            color = "#c42b1c" if problem else ("" if name == p.name else "#6a3df0")
            store.append([p.name, name, _t(problem) if problem else "", color or None])
        changed = sum(1 for p, name, _ in rows if name != p.name)
        problems = any(problem for _, _, problem in rows)
        status.set_text(_t("Corrigez les noms marqués en rouge.") if problems else
                        _t("{count} fichiers seront renommés.", count=changed) if changed else
                        _t("Aucun changement pour l’instant."))
        rename.set_sensitive(changed > 0 and not problems)
        undo_button.set_visible(can_undo())
        # Résultat du dernier renommage, gardé tant que rien n'est retapé.
        if current.get("message") and not changed: status.set_text(current["message"])
        current["message"] = current.get("message") if not changed else None
        return False

    def do_rename(_):
        done, failed = execute(current["rows"])
        moved = {before: Path(after) for after, before in done}
        current["paths"] = [moved.get(str(p), p) for p in current["paths"]]
        for key in ("find", "replace", "base", "prefix", "suffix"): widgets[key].set_text("")
        current["message"] = (_t("Certains fichiers n’ont pas pu être renommés.") if failed else
                              _t("{count} fichiers renommés.", count=len(done)))
        update()

    def do_undo(_):
        try: pairs = json.loads(undo_file().read_text(encoding="utf-8"))
        except (OSError, ValueError): pairs = []
        back = {current_path: before for current_path, before in pairs}
        restored, failed = undo()
        current["paths"] = [Path(back.get(str(p), p)) for p in current["paths"]]
        current["message"] = (_t("Certains fichiers n’ont pas pu être renommés.") if failed else
                              _t("Renommage annulé : {count} fichiers ont retrouvé leur nom.", count=restored))
        update()

    rename.connect("clicked", do_rename)
    undo_button.connect("clicked", do_undo)
    close.connect("clicked", lambda _: win.destroy())
    win.connect("destroy", Gtk.main_quit)
    win.show_all()
    update()
    widgets["find"].grab_focus()
    Gtk.main()
    return 0


def main(argv):
    args = argv[1:]
    if args == ["undo"]:
        restored, failed = undo()
        print(_t("Renommage annulé : {count} fichiers ont retrouvé leur nom.", count=restored))
        return 1 if failed else 0
    if len(args) >= 2 and args[0] in ("preview", "apply"):
        try: opts = json.loads(args[1])
        except ValueError: return 2
        rows = plan(expand(args[2:]), opts)
        if args[0] == "apply":
            done, failed = execute(rows)
            print(len(done))
            return 1 if failed else 0
        for p, name, problem in rows:
            print(f"{p.name}\t{name}\t{problem}")
        return 0
    paths = expand(args)
    if not paths:
        print(__doc__.strip().split("\n\n")[2], file=sys.stderr)
        return 2
    return window(paths)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

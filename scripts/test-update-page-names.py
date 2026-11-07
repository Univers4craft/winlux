#!/usr/bin/python3
"""Paramètres > Mises à jour : les deux pages portent des noms distincts (#637).

Demande de damien : la page 7 (Winlux lui-même, par le manifeste) s'appelle
« Mise à jour OS », la page 17 (logiciels et pilotes Linux, APT) s'appelle
« Mise à jour application », dans les quatre langues et dans les deux piles
(Paramètres natifs GTK et shell PE), ainsi que dans la recherche du menu
Démarrer, l'entrée épinglée, l'assistant et la notification du shell PE.
"""
import ast
from pathlib import Path
import re

project = Path(__file__).resolve().parent.parent
OS = {"fr": "Mise à jour OS", "en": "OS update", "es": "Actualización del SO", "de": "OS-Update"}
APP = {"fr": "Mise à jour application", "en": "Application update",
       "es": "Actualización de aplicaciones", "de": "Anwendungsupdate"}
OLD = ("Mises à jour système", "System updates", "Systemupdates", "Actualizaciones del sistema")
errors = []


def check(condition, label):
    if not condition:
        errors.append(label)


def source(path):
    return (project / path).read_text(encoding="utf-8")


def translations(path):
    """Le dictionnaire _TR d'un script : clé française -> (en, es, de)."""
    for node in ast.parse(source(path)).body:
        if isinstance(node, ast.Assign) and any(getattr(t, "id", "") == "_TR" for t in node.targets):
            return ast.literal_eval(node.value)
    raise SystemExit(f"_TR introuvable dans {path}")


def check_tr(path, labels):
    table = translations(path)
    row = table.get(labels["fr"])
    check(row == (labels["en"], labels["es"], labels["de"]), f"{path} : traduction de « {labels['fr']} » = {row}")


# Paramètres natifs (GTK) : titres des pages 7 et 17 et leurs traductions.
settings = source("scripts/winlux-settings.py")
pages = re.search(r"^PAGES = \[(.*?)\]\n", settings, re.S | re.M).group(1)
titles = re.findall(r'_t\("([^"]+)"\)', pages)
check(titles[7] == OS["fr"], f"winlux-settings.py : page 7 = {titles[7]}")
check(titles[17] == APP["fr"], f"winlux-settings.py : page 17 = {titles[17]}")
check_tr("scripts/winlux-settings.py", OS)
check_tr("scripts/winlux-settings.py", APP)
check('(_t("Mises à jour"), [7, 17])' in settings, "la catégorie « Mises à jour » doit garder les pages 7 et 17")

# Menu Démarrer : recherche des Paramètres (titre + mots-clés des 4 langues).
start = source("scripts/winlux-start-menu.py")
for page, labels in ((7, OS), (17, APP)):
    entry = re.search(rf'^\s*\({page}, "([^"]+)", "([^"]+)"\),', start, re.M)
    check(entry and entry.group(1) == labels["fr"], f"menu Démarrer : page {page} = {entry and entry.group(1)}")
    words = entry.group(2).casefold() if entry else ""
    for word in ("os update", "actualizacion del so", "os-update") if page == 7 else \
                ("application update", "actualizacion de aplicaciones", "anwendungsupdate"):
        check(word in words, f"menu Démarrer : mot-clé « {word} » absent de la page {page}")
    check_tr("scripts/winlux-start-menu.py", labels)

# Entrée épinglée du menu Démarrer : nom traduit dans les quatre langues.
desktop = source("integration/native/applications/winlux-updates.desktop")
check(f"\nName={OS['fr']}\n" in desktop, "winlux-updates.desktop : Name")
for lang in ("en", "es", "de"):
    check(f"\nName[{lang}]={OS[lang]}\n" in desktop, f"winlux-updates.desktop : Name[{lang}]")
check("Exec=/usr/bin/winlux-shell-app settings 7" in desktop, "winlux-updates.desktop doit ouvrir la page 7")

# Textes qui renvoient vers ces pages.
check_tr("scripts/winlux-assistant.py", OS)
check('7: "Mise à jour OS"' in source("scripts/winlux-assistant.py"), "assistant : page 7")
pages_extra = translations("scripts/winlux-settings-pages.py")
hint = pages_extra.get("Les mises à jour de Winlux lui-même sont dans « Mise à jour OS ».")
check(hint and all(OS[lang] in text for lang, text in zip(("en", "es", "de"), hint)), f"indice de la page 17 : {hint}")

# Shell PE : tables de locale.rs (Windows-1252) et notification de settings.rs.
locale = (project / "testexe/src/shell/locale.rs").read_bytes().decode("cp1252")
index = {name: int(value) for name, value in re.findall(r"pub const (S_\w+): usize = (\d+);", locale)}


def table(lang):
    body = re.search(rf"static {lang}: \[&\[u8\]; STRING_COUNT\] = \[(.*?)\n\];", locale, re.S).group(1)
    literals = re.findall(r'^\s*b"((?:[^"\\]|\\.)*)",', body, re.M)
    return [re.sub(r"\\x([0-9A-Fa-f]{2})", lambda m: chr(int(m.group(1), 16)), s) for s in literals]


for lang in ("fr", "en", "es", "de"):
    strings = table(lang.upper())
    check(strings[index["S_UPDATES"]] == OS[lang], f"locale.rs {lang} : S_UPDATES = {strings[index['S_UPDATES']]}")
    check(strings[index["S_SYSTEM_UPDATES"]] == APP[lang],
          f"locale.rs {lang} : S_SYSTEM_UPDATES = {strings[index['S_SYSTEM_UPDATES']]}")
shell = (project / "testexe/src/shell/settings.rs").read_bytes().decode("cp1252")
shell = re.sub(r"\\x([0-9A-Fa-f]{2})", lambda m: chr(int(m.group(1), 16)), shell)
for lang in ("fr", "en", "es", "de"):
    check(shell.count("> " + APP[lang] + '"') == 2, f"settings.rs : notification {lang} ne cite pas « {APP[lang]} »")

# Plus aucun ancien nom de page affiché.
for path in ("scripts/winlux-settings.py", "scripts/winlux-start-menu.py", "scripts/winlux-settings-pages.py",
             "scripts/winlux-assistant.py"):
    text = source(path)
    for old in OLD:
        check(f"'{old}'" not in text and f'"{old}"' not in text, f"{path} : ancien libellé « {old} »")
for old in OLD:
    check(f'b"{old}"' not in locale and f"> {old}" not in shell, f"shell PE : ancien libellé « {old} »")

if errors:
    raise SystemExit("\n".join(errors))
print("Pages Mise à jour OS et Mise à jour application : libellés vérifiés dans les quatre langues.")

#!/usr/bin/python3
"""Explorateur, Corbeille, Bloc-notes et binfmt (#11, #10, #209, #214), sans VM.

Le comportement réel est vérifié ailleurs : `cargo test -p winlux archive::copy`
(moteur de copie, pause, annulation, conflits, vidage) et explorer_regression.exe
dans scripts/test-runtime.sh (Explorateur et Bloc-notes dans Winlux). Ce test
rapide garde les câblages qui ne doivent pas disparaître, et exécute pour de
vrai l'enregistrement binfmt de l'init Live sur un faux /proc.
"""
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SHELL = ROOT / "testexe/src/shell"
checks = 0


def ok(condition, label):
    global checks
    if not condition:
        print(f"ÉCHEC : {label}", file=sys.stderr)
        raise SystemExit(1)
    checks += 1
    print(f"  OK     {label}")


explorer = (SHELL / "explorer.rs").read_text(encoding="utf-8")
copy = (SHELL / "explorer_copy.rs").read_text(encoding="utf-8")
notepad = (SHELL / "notepad.rs").read_text(encoding="utf-8")
ffi = (ROOT / "runtime/src/archive/ffi.rs").read_text(encoding="utf-8")
lib = (ROOT / "testexe/src/lib.rs").read_text(encoding="utf-8")

# ── #11 : copie de fond ─────────────────────────────────────────────────────
for name in ("WinluxFileCopy", "WinluxFileDelete", "WinluxJobPause", "WinluxJobStats"):
    ok(f'"{name}" => fn' in ffi and f"pub fn {name}(" in lib, f"{name} exporté par le runtime et déclaré pour les PE")
body = explorer[explorer.index("unsafe fn paste() {"):explorer.index("// ---------- Drag & drop de fichiers")]
ok("copy::transfer(" in body and "copy_tree(" not in body, "Coller passe par la copie de fond, plus par copy_tree sur le fil de la fenêtre")
for fn in ("pub unsafe fn drop_files_to", "pub unsafe fn paste_to"):
    part = explorer[explorer.index(fn):]
    part = part[:part.index("\n}\n")]
    ok("copy::transfer(" in part and "copy_tree(" not in part, f"{fn.split()[-1]} passe par la copie de fond")
ok("Mode::Replace" in copy and "Mode::Skip" in copy and "Mode::KeepBoth" in copy, "boîte Remplacer / Ignorer / Conserver les deux")
for word in ("Pause", "Reprendre", "Temps restant", "Vitesse", "\\xC9l\\xE9ments restants"):
    ok(word in copy, f"carte de progression : « {word} »")
ok("crate::explorer::background_tick()" in (SHELL / "wm.rs").read_text(encoding="utf-8"),
   "une copie lancée depuis le bureau est suivie même Explorateur fermé")

# ── #10 : Vider la Corbeille ────────────────────────────────────────────────
ok("Item::EmptyTrash => copy::ask_empty_trash()" in explorer, "menu « Vider la Corbeille » relié à l'action")
ok("copy::ask_empty_trash();" in explorer and "loc != TRASH" in explorer, "bouton « Vider la Corbeille » dans la barre de commandes")
ok("supprimer d\\xE9finitivement" in copy, "Vider la Corbeille demande confirmation")

# ── #209 : Bloc-notes et mot de passe ───────────────────────────────────────
size = re.search(r"const MAX_TEXT: usize = ([^;]+);", notepad).group(1)
ok(eval(size) >= 8 << 20, f"Bloc-notes : documents jusqu'à 8 Mo (MAX_TEXT = {size})")
opening = explorer[explorer.index("crate::notepad::open_new();"):explorer.index("wm::open(wm::APP_NOTEPAD);")]
ok("load_file_into_active" in opening and "ReadFile" not in opening, "l'Explorateur ouvre le fichier entier dans le Bloc-notes")
ok("d.truncated" in notepad and "if d.truncated {" in notepad, "Ctrl+S n'écrase pas un fichier chargé en partie")
ok("d.crlf" in notepad, "fins de ligne Windows conservées à l'enregistrement")
ok("dots[n]" not in explorer and "fn masked(" in explorer, "mot de passe d'archive : plus d'écriture hors tableau au 64e caractère")
ok("fn copy_as_path(" in explorer and "cliphist::set_text" in explorer, "Copier le chemin d'accès va dans le presse-papiers")

# ── Finition de l'Explorateur en 1366x768 ───────────────────────────────────
ok("unsafe fn row_icon(" in explorer and explorer.count("row_icon(g, e.kind") == 2,
   "vues Détails et Liste : icônes de fichiers de 12 px (row_icon)")
ok("Glyph::Chevron\n" not in explorer.split("heads: [(&[u8], i32, SortKey); 4]")[1][:1500],
   "en-tête de colonne : le tri croissant n'est plus dessiné « › »")
ok('b"Application Windows"' not in explorer and '[b"Application", b"Application"' in explorer,
   "type d'un .exe : « Application » comme Windows 11 (plus tronqué en 1366x768)")

ok("fn drive_space(" in explorer and "usage_bar(g, tx" in explorer and "usage_bar(g, dx" in explorer,
   "Ce PC : barre d'occupation et espace libre en Mosaïques et en Détails (#456)")
ok('b"Espace libre"' in explorer and 'b"Occupation"' in explorer, "Ce PC : colonnes Occupation et Espace libre")

props = (ROOT / "testexe/src/shell/explorer_props.rs").read_text(encoding="utf-8")
ok("let widest = items[..n].iter().map(|&i| text_width(item_label(i), 12))" in explorer,
   "menu contextuel : largeur calée sur le libellé le plus long (plus de texte coupé)")
ok('b"\\xE9l\\xE9ment s\\xE9lectionn\\xE9"' in explorer and "tr(S_SELECTED)" not in explorer,
   "barre d'état : « 1 élément sélectionné » au lieu de « 1 sélectionné(s) »")
ok("if files_only {" in explorer and "size_text(total, &mut sb)" in explorer, "barre d'état : taille totale des fichiers sélectionnés")
ok("Item::Properties =>" in explorer and "props::open(p, i)" in explorer and "key_state(0x12)" in explorer,
   "Propriétés : menu contextuel et Alt+Entrée")
ok("fn key(vk: u32, _ch: u16)" in props and "0x0D => close(true)" in props,
   "Propriétés : Entrée/Échap sur la touche virtuelle (le caractère d'Alt+Entrée ne referme pas la boîte)")
ok("SetFileAttributesW" in props and "fn walk(" in props and "props::regression(root)" in explorer,
   "Propriétés : lecture seule appliquée, taille de dossier, test de régression")

# ── #214 : binfmt en Live, exécuté sur un faux /proc ────────────────────────
iso = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
block = iso[iso.index("# binfmt : début"):iso.index("# binfmt : fin")]
conf = (ROOT / "integration/winlux-binfmt.conf").read_text(encoding="utf-8")
ok(conf.lstrip().startswith("#"), "le fichier binfmt a bien des commentaires (cas qui cassait)")
with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)
    proc = tmp / "binfmt_misc"
    proc.mkdir()
    (proc / "register").write_text("")
    (tmp / "winlux.conf").write_text(conf)
    script = (block.replace("/bin/busybox ", "")
              .replace("binfmt=/proc/sys/fs/binfmt_misc", f"binfmt={proc}")
              .replace("binfmt_conf=/etc/binfmt.d/winlux.conf", f"binfmt_conf={tmp / 'winlux.conf'}")
              # Le vrai fichier register reçoit une règle par écriture : on les accumule.
              .replace('> "$binfmt/register"', '>> "$binfmt/register"'))
    subprocess.run(["sh", "-eu", "-c", script], check=True)
    written = (proc / "register").read_text()
    rules = [line for line in conf.splitlines() if line.startswith(":")]
    ok(written.splitlines() == rules and "#" not in written, "init Live : seules les règles « :winlux:… » sont écrites dans register")
    (proc / "register").write_text("")
    (proc / "winlux").write_text("enabled\n")
    subprocess.run(["sh", "-eu", "-c", script], check=True)
    ok((proc / "register").read_text() == "", "init Live : une règle déjà enregistrée n'est pas réécrite")

print(f"test-explorer-fixes : {checks} vérifications OK")

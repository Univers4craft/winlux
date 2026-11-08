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
ok("unsafe fn row_icon(" in explorer and "_ if s == 16 => row_icon(g, e.kind, x, y)" in explorer,
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

# ── #209 / #10 : Suppr hors dossier, chemins d'Accueil, survol, taille de la Corbeille ──
ok("Les lecteurs et les dossiers d'Accueil ne peuvent pas" in explorer,
   "Suppr dans Ce PC ou Accueil : un message au lieu du silence")
ok(".position(|pl| pl.name == e.name())" not in explorer and "place_label(i) == e.name()" in explorer,
   "item_path d'Accueil compare le libellé traduit, pas le nom français")
ok("|| inside(&geo.toolbar, x, y)" not in explorer and "hot_changed(from, (x, y))" in explorer,
   "Mouse::Move ne repeint que si une zone survolée change")
ok("wm::MOUSE_X" not in explorer.replace("inside(r, wm::MOUSE_X, wm::MOUSE_Y)", ""),
   "tous les survols de l'Explorateur passent par hovered()")
ok("fn enforce_trash_limit(" in copy and "copy::enforce_trash_limit(since)" in explorer and "taille-max-mo=" in copy,
   "Corbeille : taille maximale (5 % du disque ou taille-max-mo) et purge des plus anciens")

ok("x > ir.left + detail_cols(&ir).1" not in explorer,
   "vue Détails : un clic dans Date, Type ou Taille sélectionne la ligne")
ok("fn content_text(" in props and 'b" dossier"' in props and 'b" folder"' in props,
   "Propriétés : « 1 dossier », « 1 folder » au singulier")

# ── #589 : icônes Prisme intégrées au lieu des primitives ──
icons_rs = (SHELL / "explorer_icons.rs").read_text(encoding="utf-8")
icons_data = (SHELL / "explorer_icons_data.rs").read_text(encoding="utf-8")
icons_bin = (SHELL / "explorer_icons.bin").read_bytes()
ok(f"ICON_BYTES: usize = {len(icons_bin)};" in icons_data, "explorer_icons.bin correspond à la table générée")
ok("GdiAlphaBlend(hdc" in icons_rs and "icons::of_glyph(gl)" in explorer and "icons::of_place" in explorer,
   "Explorateur : icônes composées par AlphaBlend, dossiers d'Accueil à pictogramme")
ok(not (ROOT / "assets/icons/fluent").exists() and "Fluent" not in icons_data,
   "un seul jeu d'icônes : Prisme, comme la session native")

# ── Boîte Ouvrir / Enregistrer des .exe = l'Explorateur Winlux ─────────────
pick_rs = (SHELL / "explorer_pick.rs").read_text(encoding="utf-8")
files_rs = (ROOT / "runtime/src/dlls/comdlg32_files.rs").read_text(encoding="utf-8")
ok("pick_with_explorer(owner, &request, &app)" in files_rs and '"/usr/bin/winlux-shell-app"' in files_rs,
   "GetOpenFileName / GetSaveFileName ouvrent l'Explorateur Winlux")
ok('cmd.arg("explorer")' in files_rs and "dialogs::run_builtin" in files_rs,
   "boîte intégrée gardée en repli (sans affichage, tests WINLUX_FILEDLG)")
ok("Focus::FileName" in explorer and "pick::draw(g, &geo.status)" in explorer and "pick::shows(e.name(), e.dir)" in explorer,
   "Explorateur : Nom du fichier, type et boutons à la place de la barre d'état, liste filtrée")
ok("pick::choose(p, index)" in explorer and "fn finish(" in pick_rs and "ExitProcess(0)" in pick_rs,
   "Explorateur : double-clic sur un fichier = Ouvrir, le choix est rendu au programme")
ok("SetEnvironmentVariableA(name.as_ptr(), core::ptr::null())" in pick_rs,
   "les programmes lancés depuis la boîte ne deviennent pas une boîte Ouvrir")

# ── #663 : « Terminal » ouvrait Mousepad en session native ──────────────────
wm_rs = (SHELL / "wm.rs").read_text(encoding="utf-8")
launch = wm_rs[wm_rs.index("pub unsafe fn native_launch"):]
ok("if app == APP_TERMINAL {" in launch[:400] and "native_terminal(None)" in launch[:400],
   "session native : le Terminal ne retombe plus sur « editor » (Mousepad)")
ok('b"shell:terminal"' in wm_rs and "wm::native_terminal(Some(dir))" in explorer,
   "Explorateur : Terminal ouvre winlux-terminal dans le dossier affiché")

# ── #699 : la boîte COM de Vista (IFileOpenDialog) passe par l'Explorateur ──
com_rs = (ROOT / "runtime/src/dlls/filedialog_com.rs").read_text(encoding="utf-8")
ole_rs = (ROOT / "runtime/src/dlls/ole32.rs").read_text(encoding="utf-8")
apps_rs = (ROOT / "runtime/src/dlls/common_apps.rs").read_text(encoding="utf-8")
ok("filedialog_com::create_builtin(&guid, &iid, out)" in ole_rs,
   "CoCreateInstance(CLSID_FileOpenDialog / CLSID_FileSaveDialog) ne rend plus REGDB_E_CLASSNOTREG")
ok("files::file_dialog(" in com_rs and "pick_folder(" in com_rs,
   "IFileDialog::Show ouvre l'Explorateur Winlux, comme GetOpenFileNameW")
ok("filedialog_com::sh_create_item_from_parsing_name" in apps_rs and "E_NOTIMPL" not in apps_rs.split("SHCreateItemFromParsingName")[0][-200:],
   "SHCreateItemFromParsingName rend un vrai IShellItem")
ok('b"folder"' in pick_rs and "fn accept_folder(" in pick_rs,
   "FOS_PICKFOLDERS : l'Explorateur sait choisir un dossier")

# ── #674 : Explorateur étroit, colonnes dans l'ordre, masquées par la droite ─
ok("while shown > 0 && NAME_MIN" in explorer and "sx.min(ir.right) - tx - 12" in explorer,
   "vue Détails étroite : Type et Taille disparaissent au lieu de « … » ou d'être inversées")

# ── #738 : Accueil en cartes, le défilement ne passe plus sur la barre ────
order = explorer[explorer.index("pub unsafe fn draw(g: &mut Gfx, f: &Frame)"):]
order = order[:order.index("\n}\n")]
ok(order.index("draw_pane(g, &geo.panes[0], 0)") < order.index("draw_toolbar(g, &geo)"),
   "barre d'adresse et de commandes dessinées après les volets (le GDI ne découpe pas)")
ok("pane_label(g, &rect(r.left, r.top - 22, r.right, r.bottom), pn, focused)" in explorer,
   "double volet : l'en-tête du volet redessiné par-dessus les cartes défilées")

home_rs = (SHELL / "explorer_home.rs").read_text(encoding="utf-8")
ok("if shown(l.title_y, l.title_y + 40)" in home_rs and "frame.top.max(r.top - 24)" in home_rs,
   "#757 : titres et cadre des Récents défilés hors du volet ne sont plus dessinés sur les onglets")

# ── #739 : « Compatibilité » d'un .exe ouvre sa carte dans Paramètres ──────
wm_rs = (SHELL / "wm.rs").read_text(encoding="utf-8")
settings_rs = (SHELL / "settings.rs").read_text(encoding="utf-8")
ok("WINLUX_SETTINGS_EXE" in wm_rs and "native_settings(10)" in wm_rs
   and "crate::wm::native_exe_settings(&z)" in settings_rs,
   "session native : Programmes Windows (10) avec le chemin du .exe, plus la page Écran")
settings_py = (ROOT / "scripts/winlux-settings.py").read_text(encoding="utf-8")
ok('"--linux-path"' in settings_py and '"--linux-path"' in (ROOT / "runtime/src/main.rs").read_text(encoding="utf-8"),
   "Paramètres convertit le chemin Windows avec les règles du runtime (winlux --linux-path)")

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

# ── #689 : direction « Prisme » (Accueil en cartes, volet Détails) ─────────
home = (SHELL / "explorer_home.rs").read_text(encoding="utf-8")
ok('#[path = "explorer_home.rs"]' in explorer and "home::fill_home(p)" in explorer,
   "Accueil : lecteurs et récents ajoutés après les dossiers personnels")
ok("home::draw(g, pn, r, which, focused)" in explorer and "home::item_at(p, r, x, y)" in explorer,
   "Accueil : cartes dessinées et cliquables avec la même mise en page")
ok("home::draw_details(g, &d)" in explorer and "details_button(geo)" in explorer,
   "Volet Détails : bouton « Détails » dans la barre de commandes et volet à droite")
ok(explorer.count("home::name_badge(") >= 3, "Pastille « Windows » dans les vues Détails, Liste et Mosaïques")
ok("home::regression();" in explorer and "pub(super) unsafe fn regression()" in home,
   "Accueil et volet Détails vérifiés par explorer_regression")
ok(all(w in home for w in ('b"Lecteurs"', 'b"Drives"', 'b"Unidades"', 'b"Laufwerke"')),
   "Accueil : titres de section dans les quatre langues")

# ── #886, #890, #891 : icônes Prisme et thème en direct ─────────────────────
gen = (ROOT / "scripts/gen-explorer-icons.py").read_text(encoding="utf-8")
prisme = (ROOT / "scripts/generer-icones.py").read_text(encoding="utf-8")
icons_dir = ROOT / "assets/icons/explorateur"
for variant, name in (("Spreadsheet", "type-tableur"), ("Presentation", "type-presentation"),
                      ("Font", "type-police"), ("Web", "type-web"), ("Code", "type-code"),
                      ("Script", "type-script"), ("Terminal", "terminal"), ("Search", "rechercher")):
    ok(f'("{variant}", "{name}")' in gen and all((icons_dir / f"{name}-{t}.png").exists() for t in (16, 20, 24, 32, 48)),
       f"icône Prisme {name} générée et intégrée")
    ok(f'"{name}"' in prisme, f"{name} produite par scripts/generer-icones.py (pas à la main)")
data = (SHELL / "explorer_icons_data.rs").read_text(encoding="utf-8")
ok("Spreadsheet," in data and "Terminal," in data, "explorer_icons_data.rs régénéré")
ok('if any(&[b"exe", b"com", b"msi", b"appimage"])' in explorer and 'b"sh", b"bash"' in explorer,
   "#890 : .sh classé script, plus programme Windows")
ok("types_regression();" in home_rs, "#890 : types, pastille et libellés vérifiés par explorer_regression")
demo = (ROOT / "testexe/src/bin/gui_demo.rs").read_text(encoding="utf-8")
ok("if gfx::theme_animating() {\n                        InvalidateRect(hwnd" in demo,
   "#891 : la transition clair/sombre est repeinte jusqu'au bout dans une fenêtre ouverte")

# ── #888 : maquette Prisme ; #916 : unités et dates en allemand ────────────
wm_rs = (SHELL / "wm.rs").read_text(encoding="utf-8")
ok("fn native_frameless" in wm_rs and "DefWindowProcA(hwnd, 0x00A1, ht, 0)" in wm_rs
   and "0x8000_0000 | 0x0004_0000" in demo,
   "#888 : Explorateur natif sans barre de titre, onglets en légende (déplacer, bords, boutons)")
gui = (ROOT / "runtime/src/dlls/gui.rs").read_text(encoding="utf-8")
x11 = (ROOT / "runtime/src/dlls/gui_x11.rs").read_text(encoding="utf-8")
ok("fn begin_move_resize" in gui and "_NET_WM_MOVERESIZE" in x11 and "WM_CHANGE_STATE" in x11
   and "_NET_WM_STATE_MAXIMIZED_VERT" in x11,
   "#888 : runtime — WM_NCLBUTTONDOWN/WM_SYSCOMMAND et ShowWindow réduire/agrandir via le gestionnaire")
ok('c"Explorateur de fichiers"' in demo and 'c"Datei-Explorer"' in demo,
   "#888 : titre de fenêtre traduit, plus « Explorer Winlux »")
# Les tests de session cherchent la fenêtre sous ses titres traduits : l'ancien
# titre « Explorer Winlux » faisait échouer le contrôle « release » (session invitée).
_titled = [p.name for p in (ROOT / "scripts").glob("*.py")
           if p.name != "test-explorer-fixes.py"
           and any(k in p.read_text(encoding="utf-8")
                   for k in ('wait_title("Explorer Winlux")', '"Explorer Winlux" in title'))]
guest = (ROOT / "scripts/test-native-session-guest.py").read_text(encoding="utf-8")
ok(not _titled and 'wait_title("Explorer Winlux")' not in guest and "EXPLORER_TITLES" in guest
   and '"Datei-Explorer"' in guest,
   "#888 : la session invitée cherche l'Explorateur sous ses quatre titres traduits")
ok("Cmd::More" in explorer and "Cmd::Share" in explorer and "fn view_label" in explorer,
   "#888 : barre de commandes compacte (Partager, affichage courant ▾, « … »)")
ok('const NET: &[u8] = b"::net";' in explorer and "push(net_label(), Glyph::NetDrive, NET, 0);" in explorer,
   "#888 : Réseau dans le volet de navigation")
ok("fn center_wrap" in explorer and "layout_regression();" in explorer,
   "#888 : noms sur deux lignes, vérifiés par explorer_regression")
ok('b" GB", b" TB"' in explorer and "formats_regression();" in explorer,
   "#916 : unités GB et date 08.11.2026 en allemand")

print(f"test-explorer-fixes : {checks} vérifications OK")

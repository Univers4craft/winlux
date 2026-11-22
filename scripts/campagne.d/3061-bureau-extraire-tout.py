# #3061 : clic droit sur une archive du Bureau (xfdesktop, actions de
# uca.xml) → « Extraire tout… », qui ouvre l'Explorateur sur le dossier de
# l'archive et l'extrait dans un nouveau dossier à son nom.
import fnmatch as _fn

def _actions_3061(text):
    out = []
    for a in ET.fromstring(text).findall("action"):
        pats = [p for p in (a.findtext("patterns") or "").split(";") if p]
        out.append((a, pats))
    return out

def _offert_3061(text, nom):
    return [a for a, pats in _actions_3061(text)
            if any(_fn.fnmatchcase(nom, p) for p in pats)
            and (a.findtext("command") or "").startswith("winlux-shell-app explorer --extraire %f")]

_uca_3061 = (NATIVE / "config/Thunar/uca.xml").read_text(encoding="utf-8")
for _nom in ("photos.zip", "photos.7z", "SAUVEGARDE.ZIP", "sources.tar.gz", "pilote.rar"):
    ok(len(_offert_3061(_uca_3061, _nom)) == 1, f"Bureau : « Extraire tout… » proposé sur {_nom} (#3061)")
for _nom in ("notes.txt", "photo.jpg", "setup.exe", "disque.iso"):
    ok(not _offert_3061(_uca_3061, _nom), f"Bureau : pas d'« Extraire tout… » sur {_nom} (#3061)")
_a = _offert_3061(_uca_3061, "photos.zip")[0]
_noms = {n.get("{http://www.w3.org/XML/1998/namespace}lang", "fr"): n.text for n in _a.findall("name")}
ok(_noms == {"fr": "Extraire tout…", "en": "Extract all…", "es": "Extraer todo…", "de": "Alle extrahieren…"}
   and _a.findtext("range") == "1",
   "Bureau : « Extraire tout… » traduit en fr/en/de/es, pour un seul fichier (#3061)")

# winlux-shell-app explorer --extraire passe l'archive à l'Explorateur, en
# chemin Windows, même donnée en URI file:// avec accents et espaces.
with tempfile.TemporaryDirectory() as _d:
    _d = Path(_d)
    _rt = fake_tool(_d, "runtime", 'env | grep "^WINLUX_SHELL_" | sort > "$0.env"; echo "$@" >> "$0.env"')
    for _arg, _attendu in (
        ("/home/damien/Téléchargements/photos d'été.zip", "Z:\\home\\damien\\Téléchargements\\photos d'été.zip"),
        ("file:///home/damien/T%C3%A9l%C3%A9chargements/photos%20d'%C3%A9t%C3%A9.zip",
         "Z:\\home\\damien\\Téléchargements\\photos d'été.zip"),
    ):
        _p = subprocess.run(["sh", str(SCRIPTS / "winlux-shell-app.sh"), "explorer", "--extraire", _arg],
                            env={**os.environ, "WINLUX_RUNTIME": str(_rt)}, capture_output=True, text=True)
        _env = (_d / "runtime.env").read_text(encoding="utf-8") if (_d / "runtime.env").exists() else ""
        print(_p.stdout + _p.stderr + _env)
        ok(_p.returncode == 0 and f"WINLUX_SHELL_EXTRACT={_attendu}\n" in _env
           and "WINLUX_SHELL_APP=explorer\n" in _env and "WINLUX_SHELL_PATH=" not in _env,
           f"winlux-shell-app explorer --extraire donne l'archive à l'Explorateur ({_arg[:7]}) (#3061)")
        (_d / "runtime.env").unlink(missing_ok=True)
    _p = subprocess.run(["sh", str(SCRIPTS / "winlux-shell-app.sh"), "explorer", "--extraire"],
                        env={**os.environ, "WINLUX_RUNTIME": str(_rt)}, capture_output=True, text=True)
    ok(_p.returncode == 2 and not (_d / "runtime.env").exists(),
       "winlux-shell-app explorer --extraire sans fichier refuse sans rien lancer (#3061)")

# Profil existant : l'action est ajoutée une fois, les actions personnelles restent.
with tempfile.TemporaryDirectory() as _d:
    _home = Path(_d)
    (_home / ".config/Thunar").mkdir(parents=True)
    _perso = "<action>\n\t<name>Mon action</name>\n\t<command>mon-outil %f</command>\n</action>\n"
    (_home / ".config/Thunar/uca.xml").write_text("<actions>\n" + _perso + "</actions>\n", encoding="utf-8")
    native_config(_home)
    native_config(_home)
    _t = (_home / ".config/Thunar/uca.xml").read_text(encoding="utf-8")
    print(_t)
    ok(len(_offert_3061(_t, "photos.zip")) == 1 and "<command>mon-outil %f</command>" in _t,
       "profil existant : « Extraire tout… » ajouté une seule fois, action personnelle gardée (#3061)")

# L'Explorateur lit l'archive demandée au démarrage et lance l'extraction.
_wm = (ROOT / "testexe/src/shell/wm.rs").read_text(encoding="utf-8")
_ex = (ROOT / "testexe/src/shell/explorer.rs").read_text(encoding="utf-8")
ok('c"WINLUX_SHELL_EXTRACT"' in _wm and "pub unsafe fn extract_at(" in _ex,
   "l'Explorateur sait démarrer sur l'extraction d'une archive (extract_at, #3061)")

# À 125 %, la première session pose 120 ppp même si xsettings rend déjà 96 :
# sans marqueur, 96 est la valeur par défaut, pas un réglage fait à la main.
# Une autre valeur sans marqueur, ou une valeur différente de celle du
# marqueur, reste gardée (#3437). La fonction des deux sessions est exécutée
# avec un faux xfconf-query.
_cas_3437 = {
    # nom : (marqueur, DPI en place, échelle, DPI attendu après, marqueur attendu)
    "premiere-session-96": (None, "96", "1.25", "120", "1:120"),
    "premiere-session-vide": (None, "", "1.5", "144", "1:144"),
    "manuel-sans-marqueur": (None, "110", "1.25", "110", None),
    "manuel-apres-marqueur": ("1:120", "144", "1.25", "144", "1:120"),
    "deja-pose": ("1:120", "120", "1.25", "120", "1:120"),
}
_vus_3437 = {}
for _script in ("winlux-user-session.sh", "winlux-native-session.sh"):
    _src = (SCRIPTS / _script).read_text(encoding="utf-8")
    _debut = _src.find("gtk_scale() {")
    _fn = _src[_debut:_src.index("\n}\n", _debut) + 3] if _debut >= 0 else "gtk_scale() { :; }\n"
    for _k, (_marq, _dpi, _echelle, _att_dpi, _att_marq) in _cas_3437.items():
        with tempfile.TemporaryDirectory() as _d:
            _d = Path(_d)
            _etat = _d / "dpi"
            _etat.write_text(_dpi)
            _faux = _d / "xfconf-query"
            _faux.write_text(
                '#!/bin/sh\n'
                'for a; do last=$a; done\n'
                'case "$*" in *"-p /Xft/DPI"*" -s "*) printf %s "$last" >"$ETAT" ;;\n'
                '  *"-p /Xft/DPI"*) test -s "$ETAT" && cat "$ETAT" || exit 1 ;; esac\n')
            _faux.chmod(0o755)
            if _marq:
                (_d / ".local/share/winlux").mkdir(parents=True)
                (_d / ".local/share/winlux/gtk-scale").write_text(_marq + "\n")
            _code = _fn.replace("/usr/bin/xfconf-query", str(_faux)) + 'gtk_scale "$1"\n'
            subprocess.run(["sh", "-c", _code, "sh", _echelle],
                           env={"HOME": str(_d), "ETAT": str(_etat), "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
                                "LC_ALL": "C"}, timeout=20)
            _m = _d / ".local/share/winlux/gtk-scale"
            _obtenu = (_etat.read_text().strip(), _m.read_text().strip() if _m.exists() else None)
            _vus_3437[(_script, _k)] = _obtenu == (_att_dpi, _att_marq)
            if not _vus_3437[(_script, _k)]:
                print(f"      {_script} {_k} : obtenu {_obtenu}, attendu {(_att_dpi, _att_marq)}")
ok(len(_vus_3437) == 10 and all(_vus_3437.values()),
   "Échelle GTK : la première session pose le DPI de l'échelle, un réglage manuel est gardé (#3437)")

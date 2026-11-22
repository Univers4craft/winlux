# #3001 : « Reprendre où vous en étiez » ne nomme pas le portail de fichiers
# (xdg-desktop-portal-gtk) comme application d'un fichier récent : le
# programme réellement utilisé, ou rien.
_menu_3001 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_3001")
_home_3001 = Path(tempfile.mkdtemp(prefix="campagne-3001-"))
(_home_3001 / "seul.png").write_text("x")
(_home_3001 / "gimp.png").write_text("x")
_xbel_3001 = _home_3001 / "recently-used.xbel"
def _signet_3001(nom, *applis):
    return (f'<bookmark href="file://{_home_3001}/{nom}" added="2026-11-01T10:00:00Z"><info><metadata>'
            + "".join(f'<bookmark:application name="{a}" modified="2026-11-0{i + 1}T10:00:00Z"/>'
                      for i, a in enumerate(applis))
            + "</metadata></info></bookmark>")
_xbel_3001.write_text('<xbel version="1.0" xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks">'
                      + _signet_3001("seul.png", "xdg-desktop-portal-gtk")
                      + _signet_3001("gimp.png", "gimp", "xdg-desktop-portal-gtk") + "</xbel>")
_vus_3001 = {p.name: a for p, a, _ in _menu_3001["recent_files"](_xbel_3001, limit=5, home=_home_3001)}
print(f"      fichiers récents : {_vus_3001}")
ok(_vus_3001 == {"seul.png": "", "gimp.png": "gimp"},
   "Démarrer : un fichier récent choisi par le portail de fichiers est attribué à l'application, jamais au portail (#3001)")

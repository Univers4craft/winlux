# #3047 : la calculatrice livrée dans l'ISO a son entrée au menu Démarrer,
# qui lance le programme réellement installé, avec son icône Prisme.
import configparser as _cp_3047
_de_3047 = _cp_3047.ConfigParser(interpolation=None)
_de_3047.read(NATIVE / "applications/winlux-calculator.desktop", encoding="utf-8")
_entree_3047 = dict(_de_3047["Desktop Entry"]) if _de_3047.has_section("Desktop Entry") else {}
_cible_3047 = (_entree_3047.get("exec") or "").split()[-1:]
_iso_3047 = (SCRIPTS / "build-live-iso.sh").read_text(encoding="utf-8")
print(f"      entrée : {_entree_3047.get('name')!r} → {_entree_3047.get('exec')!r}")
ok(_entree_3047.get("name") == "Calculatrice" and _cible_3047 == ["/opt/winlux/apps/calc.exe"]
   and '"$stage/opt/winlux/apps/calc.exe"' in _iso_3047
   and (NATIVE / "icons/Winlux/scalable/apps" / (_entree_3047.get("icon", "?") + ".svg")).is_file(),
   "Démarrer : la Calculatrice a son entrée et lance la calculatrice installée (#3047)")

# #3046 : le bureau natif a un outil de capture d'écran. Windows + Maj + S et
# Impr. écran l'ouvrent, Windows + Impr. écran et Alt + Impr. écran capturent
# sans question ; une entrée « Capture d’écran » est au menu Démarrer.
import configparser as _cp_3046
import xml.etree.ElementTree as _et_3046
_ns_3046 = "{http://openbox.org/3.4/rc}"
_rc_3046 = _et_3046.parse(NATIVE / "config/openbox/rc.xml").getroot().find(_ns_3046 + "keyboard")
_keys_3046 = {b.get("key"): (b.findtext(f"{_ns_3046}action/{_ns_3046}command") or "") for b in _rc_3046}
_vus_3046 = {k: _keys_3046.get(k) for k in ("W-S-s", "Print", "W-Print", "A-Print")}
print(f"      raccourcis de capture : {_vus_3046}")
ok(_vus_3046 == {"W-S-s": "winlux-capture", "Print": "winlux-capture",
                 "W-Print": "winlux-capture ecran", "A-Print": "winlux-capture fenetre"},
   "Capture d'écran : Windows + Maj + S, Impr. écran, Windows + Impr. écran et Alt + Impr. écran (#3046)")
_de_3046 = _cp_3046.ConfigParser(interpolation=None)
_de_3046.read(NATIVE / "applications/winlux-capture.desktop", encoding="utf-8")
_entree_3046 = dict(_de_3046["Desktop Entry"]) if _de_3046.has_section("Desktop Entry") else {}
print(f"      entrée de menu : {_entree_3046.get('name')!r} → {_entree_3046.get('exec')!r}")
ok(_entree_3046.get("exec") == "/usr/bin/winlux-capture" and _entree_3046.get("name") == "Capture d’écran"
   and (NATIVE / "icons/Winlux/scalable/apps" / (_entree_3046.get("icon", "?") + ".svg")).is_file()
   and "'winlux-capture.py:winlux-capture'" in (SCRIPTS / "stage-desktop-components.sh").read_text(encoding="utf-8"),
   "Capture d'écran : entrée « Capture d’écran » au menu Démarrer, programme installé et icône Prisme (#3046)")
_paquets_3046 = runpy.run_path(str(SCRIPTS / "prepare-desktop-packages.py"))["PACKAGES"]
ok("xclip" in _paquets_3046,
   "Capture d'écran : l'image reste au presse-papiers après la fermeture de l'outil, rien de résident (#3046)")

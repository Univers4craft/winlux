# Un programme x86 qui demande IUISettings2 à l'objet UISettings du runtime
# la reçoit au lieu d'un refus (outil de déploiement d'Office, #3177).
import re as _re
_rt = (ROOT / "runtime" / "src" / "wow32" / "com_winrt.rs").read_text(encoding="utf-8")
_decl = _re.search(r'static IUISETTINGS2: Iface = Iface \{\s*name: "IUISettings2",\s*iid: guid\(\s*0xBAD8_2401,', _rt)
_liste = _re.search(r"static ALL: &\[&Iface\] = &\[(.*?)\];", _rt, _re.S)
_dans = bool(_liste and "&IUISETTINGS2," in _liste.group(1))
print(f"#3177 : déclaration {'trouvée' if _decl else 'absente'}, dans ALL : {_dans}")
ok(bool(_decl) and _dans and "fn reglages_d_interface_2_decrits" in _rt,
   "IUISettings2 est décrite pour les programmes x86 (#3177)")

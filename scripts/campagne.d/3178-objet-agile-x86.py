# Un programme x86 qui demande IAgileObject à un objet du runtime la reçoit
# au lieu d'un refus (outil de déploiement d'Office, #3178).
import re as _re
_com = (ROOT / "runtime" / "src" / "wow32" / "com.rs").read_text(encoding="utf-8")
_decl = _re.search(r'static IAGILEOBJECT: Iface = Iface \{\s*name: "IAgileObject",\s*iid: guid\(\s*0x94EA_2B94,', _com)
_liste = _re.search(r"static ALL: &\[&Iface\] = &\[(.*?)\];", _com, _re.S)
print(f"#3178 : déclaration {'trouvée' if _decl else 'absente'}, "
      f"dans ALL : {bool(_liste and '&IAGILEOBJECT,' in _liste.group(1))}")
ok(bool(_decl) and bool(_liste) and "&IAGILEOBJECT," in _liste.group(1)
   and "fn objet_agile_decrit" in _com,
   "IAgileObject est décrite pour les programmes x86 (#3178)")

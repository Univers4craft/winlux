# Un programme x86 reçoit GetVarDesc, GetNames et GetIDsOfNames d'un
# ITypeInfo du runtime : VARDESC traduit en disposition x86, tableaux de
# pointeurs dont le compte suit passés dans les deux sens (#2967).
_com = (ROOT / "runtime" / "src" / "wow32" / "com.rs").read_text(encoding="utf-8")
_ti = (ROOT / "runtime" / "src" / "wow32" / "com_typeinfo.rs").read_text(encoding="utf-8")
ok("fn var_desc" in _ti and "fn vardesc_en_disposition_x86_2967" in _ti,
   "VARDESC est rendu en disposition x86 (36 octets), constante comprise (#2967)")
ok("InPtrsNext" in _com and "OutPtrsNext" in _com
   and "fn tableaux_de_pointeurs_d_itypeinfo_2967" in _com,
   "GetNames et GetIDsOfNames d'ITypeInfo passent leur tableau de pointeurs en x86 (#2967)")

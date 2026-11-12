# Pare-feu : get__NewEnum rend un IEnumVARIANT réel (#2036).
_e = (ROOT / "runtime/src/dlls/enum_variant.rs").read_text(encoding="utf-8")
_f1 = (ROOT / "runtime/src/dlls/netfw.rs").read_text(encoding="utf-8")
_f2 = (ROOT / "runtime/src/dlls/netfw2.rs").read_text(encoding="utf-8")
_x = (ROOT / "runtime/src/wow32/com_netfw.rs").read_text(encoding="utf-8")
ok("pub fn new_enum(" in _e and "VT_DISPATCH" in _e, "IEnumVARIANT du runtime (#2036)")
ok("super::enum_variant::new_enum(" in _f1, "INetFwAuthorizedApplications::get__NewEnum servi (#2036)")
ok("super::enum_variant::new_enum(" in _f2, "INetFwRules::get__NewEnum servi (#2036)")
ok("&IENUMVARIANT," in _x, "IEnumVARIANT décrit pour les programmes x86 (#2036)")

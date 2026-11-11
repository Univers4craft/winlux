# CLSIDFromProgID trouve les classes MSXML intégrées sans clé HKCR (outil de déploiement d'Office, #1758).
_src = (ROOT / "runtime/src/dlls/ole32.rs").read_text(encoding="utf-8")
ok("msxml::clsid_for_prog_id" in _src, "CLSIDFromProgID se rabat sur les ProgID MSXML intégrés (#1758)")

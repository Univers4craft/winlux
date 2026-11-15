# advapi32 : GetSidLengthRequired exporté, taille d'un SID à n sous-autorités (#2533).
_s = (ROOT / "runtime/src/dlls/security.rs").read_text()
ok('"GetSidLengthRequired" => fn GetSidLengthRequired' in _s,
   "advapi32 exporte GetSidLengthRequired, importé par armsvc.exe (#2533)")

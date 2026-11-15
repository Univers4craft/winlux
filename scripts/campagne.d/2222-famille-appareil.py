# ntdll exporte RtlConvertDeviceFamilyInfoToString (#2222), demandée par
# GetProcAddress au démarrage de Teams.
_s = (ROOT / "runtime/src/dlls/ntdll.rs").read_text()
ok('"RtlConvertDeviceFamilyInfoToString" => fn rtl_convert_device_family_info_to_string' in _s,
   "ntdll exporte RtlConvertDeviceFamilyInfoToString (#2222)")

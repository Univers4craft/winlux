# CoCreateInstance sert CLSID_TF_InputProcessorProfiles : steamwebhelper (CEF) ne s'arrête plus sur un CHECK (#1602).
ok("fn profils_de_saisie_et_profil_actif" in (ROOT / "runtime/src/dlls/msctf_profiles.rs").read_text(encoding="utf-8")
   and "msctf_profiles::create_builtin" in (ROOT / "runtime/src/dlls/ole32.rs").read_text(encoding="utf-8"),
   "profils de saisie TSF servis par CoCreateInstance (#1602)")

# #1792 icuuc.dll ; #1816 message borné ; #1817 security-provider et ImpersonateNamedPipeClient.
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
i = (runtime_dlls / "icuuc.rs").read_text(encoding="utf-8")
l = (ROOT / "runtime/src/loader.rs").read_text(encoding="utf-8")
ok('name: "icuuc"' in m and '"ucnv_toUnicode" => fn ucnv_to_unicode' in i and '"UCNV_FROM_U_CALLBACK_SUBSTITUTE"' in i,
   "icuuc.dll sert les convertisseurs ucnv_* de Qt6Core (#1792)")
ok("fn shorten(" in l and 'format!("{e} ← {me}")' in l,
   "« DLL introuvable » reste court : 60 fonctions au plus, puis la chaîne des modules (#1816)")
ok('base.starts_with("api-ms-win-security-provider-l1-")' in m and '"ImpersonateNamedPipeClient",' in m,
   "api-ms-win-security-provider et ImpersonateNamedPipeClient servis par advapi32 (#1817)")

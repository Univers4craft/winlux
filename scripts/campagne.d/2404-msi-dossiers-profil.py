# msiexec : TempFolder, AppDataFolder et les autres dossiers du profil sous C:\Users\<compte> (#2404).
_t = (ROOT / "runtime/src/msi/tests.rs").read_text(encoding="utf-8")
ok("fn dossiers_du_profil_sous_users_compte(" in _t,
   "msiexec : dossiers du profil sous C:\\Users\\<compte>, TempFolder égal à GetTempPath (#2404)")

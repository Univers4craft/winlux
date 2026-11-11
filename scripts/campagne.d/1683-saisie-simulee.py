# #1683 : la saisie simulée passe par XTEST dans la session, sous autorisation, jamais par uhid.
saisie = (runtime_dlls / "saisie.rs").read_text(encoding="utf-8")
perm = (ROOT / "runtime" / "src" / "permissions.rs").read_text(encoding="utf-8")
tool = (ROOT / "scripts" / "winlux-permission.py").read_text(encoding="utf-8")
settings = (ROOT / "scripts" / "winlux-settings.py").read_text(encoding="utf-8")
ok("xtest_fake_input" in saisie and "uhid" not in saisie.replace("/dev/uhid reste fermé", ""),
   "saisie simulée : XTEST dans la session du compte, aucun appareil uhid (#1683)")
ok("Capability::Input" in saisie and "session_locked()" in saisie
   and "Capability::Sandbox | Capability::Input" in perm,
   "saisie simulée : autorisation explicite, jamais implicite, et rien sur l'écran verrouillé (#1683)")
ok('"input": ("Autoriser « {app} » à taper au clavier et à piloter la souris à votre place ?"' in tool
   and "Darf „{app}“ für Sie tippen und die Maus steuern?" in tool
   and '"input": "Taper et piloter la souris à votre place"' in settings
   and "'Taper et piloter la souris à votre place': ('Type and control the mouse for you'" in settings,
   "saisie simulée : question en 4 langues et interrupteur dans Paramètres > Confidentialité (#1683)")
ok((ROOT / "scripts" / "test-sendinput.sh").exists() and (ROOT / "testc" / "msvc" / "sendinput_msvc.c").exists(),
   "saisie simulée : programme x64 et x86 sous Xvfb (#1683)")

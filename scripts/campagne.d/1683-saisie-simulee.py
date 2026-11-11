# #1683 : la saisie simulée passe par XTEST dans la session, sous autorisation, jamais par uhid.
saisie = (runtime_dlls / "saisie.rs").read_text(encoding="utf-8")
perm = (ROOT / "runtime" / "src" / "permissions.rs").read_text(encoding="utf-8")
tool = (ROOT / "scripts" / "winlux-permission.py").read_text(encoding="utf-8")
settings = (ROOT / "scripts" / "winlux-settings.py").read_text(encoding="utf-8")
code = "\n".join(l for l in saisie.splitlines() if not l.lstrip().startswith("//"))
ok("xtest_fake_input" in saisie and not any(m in code for m in ("uhid", "uinput", "/dev/", "O_RDWR")),
   "saisie simulée : XTEST dans la session du compte, jamais /dev/uhid ni /dev/uinput (#1683)")
regles = [p for p in ROOT.rglob("*.rules") if "target" not in p.parts and ".git" not in p.parts]
ok(not any(m in p.read_text(encoding="utf-8", errors="replace") for p in regles for m in ("uhid", "uinput")),
   "saisie simulée : aucune règle udev n'ouvre /dev/uhid ni /dev/uinput aux comptes (#1683)")
ok("consent_shown()" in saisie and '"winlux-uac"' in saisie.replace("b\"winlux-uac\"", '"winlux-uac"'),
   "saisie simulée : rien n'est tapé pendant une demande d'autorisation ou d'élévation (#1683)")
ok("Capability::Input" in saisie and "session_locked()" in saisie
   and "let implied = cap != Capability::Input;" in perm,
   "saisie simulée : autorisation explicite, jamais implicite, et rien sur l'écran verrouillé (#1683)")
ok('"input": ("Autoriser « {app} » à taper au clavier et à piloter la souris à votre place ?"' in tool
   and "Darf „{app}“ für Sie tippen und die Maus steuern?" in tool
   and '"input": "Taper et piloter la souris à votre place"' in settings
   and "'Taper et piloter la souris à votre place': ('Type and control the mouse for you'" in settings,
   "saisie simulée : question en 4 langues et interrupteur dans Paramètres > Confidentialité (#1683)")
ok((ROOT / "scripts" / "test-sendinput.sh").exists() and (ROOT / "testc" / "msvc" / "sendinput_msvc.c").exists(),
   "saisie simulée : programme x64 et x86 sous Xvfb (#1683)")

# #2495 : CreateAppContainerProfile crée le profil (puis ERROR_ALREADY_EXISTS)
# et DeriveAppContainerSidFromAppContainerName rend le même SID S-1-15-2-….
# Le comportement est gardé par le test unitaire nommé.
ew = (ROOT / "runtime" / "src" / "dlls" / "electron_win32.rs").read_text(encoding="utf-8")
ok("fn profil_appcontainer_cree_puis_existant" in ew, "test du profil AppContainer créé puis existant (#2495)")

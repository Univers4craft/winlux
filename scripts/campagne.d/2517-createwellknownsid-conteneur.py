# #2517 : CreateWellKnownSid connaît WinCreatorOwnerRightsSid et les SID de capacité
# d'AppContainer. Le comportement est gardé par le test unitaire nommé.
t = (runtime_dlls / "security" / "tests.rs").read_text(encoding="utf-8")
ok("fn sid_bien_connus_des_conteneurs" in t, "CreateWellKnownSid rend S-1-3-4 et les SID S-1-15-3-* de capacité (#2517)")

# #2527 : GetAppContainerNamedObjectPath est servie par kernel32 et rend le chemin
# des objets nommés d'un AppContainer. Le comportement est gardé par le test unitaire nommé.
t = (runtime_dlls / "security" / "tests.rs").read_text(encoding="utf-8")
ok("fn chemin_des_objets_nommes_d_un_conteneur" in t, "GetAppContainerNamedObjectPath servie par kernel32 (#2527)")

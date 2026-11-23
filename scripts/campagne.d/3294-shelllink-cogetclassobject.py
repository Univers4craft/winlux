# #3294 : CoGetClassObject sert la classe ShellLink que CoCreateInstance
# crée déjà ; gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/dlls/shell_link/tests.rs").read_text(encoding="utf-8")
ok("fn fabrique_shelllink_par_cogetclassobject_3294" in t, "fabrique ShellLink par CoGetClassObject (#3294)")
o = (ROOT / "runtime/src/dlls/ole32.rs").read_text(encoding="utf-8")
ok("fn classes_integrees_servies_par_cogetclassobject_3294" in o, "toutes les classes intégrées servies par CoGetClassObject (#3294)")

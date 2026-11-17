# #2765 : un programme x86 (installeur de Creative Cloud) passe son propre
# objet COM à OleRun ; le runtime lisait sa table x86 comme une table win64
# et fautait à l'adresse 0. OleRun, OleIsRunning, OleLockRunning et
# OleSetContainedObject reçoivent un mandataire, et IRunnableObject est
# décrite. Comportement gardé par le test
# wow32::com_ole::objet_executable_x86_passe_par_un_mandataire.
com_ole_2765 = (ROOT / "runtime" / "src" / "wow32" / "com_ole.rs").read_text(encoding="utf-8")
ok("static IRUNNABLEOBJECT: Iface" in com_ole_2765
   and "fn objet_executable_x86_passe_par_un_mandataire" in com_ole_2765,
   "OleRun d'un programme x86 : objet passé par un mandataire (#2765)")

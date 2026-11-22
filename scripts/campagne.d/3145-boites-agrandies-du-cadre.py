# Une boîte de dialogue de modèle garde la zone client de son modèle : sa
# fenêtre est agrandie du cadre du gestionnaire, et les boutons du bas
# restent visibles (#3145, régression de #3036).
dlg_tests_3145 = (ROOT / "runtime/src/dlls/dialogs/tests.rs").read_text(encoding="utf-8")
ok("fn boite_agrandie_du_cadre_du_gestionnaire_3145" in dlg_tests_3145,
   "Boîtes de dialogue : zone client = modèle, fenêtre agrandie du cadre (#3145)")

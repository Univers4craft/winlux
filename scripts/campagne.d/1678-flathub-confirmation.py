# #1678 : une application Flathub ne s'installe qu'après la confirmation qui
# nomme produit, version, éditeur, source et révision, Annuler par défaut ;
# Flathub est proposé coché à l'installation, décochable (accord de damien).
store_py = (SCRIPTS / "winlux-store.py").read_text(encoding="utf-8")
catalogs = (ROOT / "integration/store-catalogs.json").read_text(encoding="utf-8")
ok('notes=("fp_linux",)' in store_py and '"fp_commit":' in store_py and "--show-commit" in store_py,
   "Store : confirmation avant d'installer une application Flathub, révision montrée (#1678)")
ok('f"--commit={commit}"' in store_py,
   "Store : la révision Flathub installée est celle confirmée (#1678)")
ok('"preselected": true' in catalogs and catalogs.count('"preselected"') == 1,
   "Boutiques : seul Flathub est proposé coché, et reste décochable (#1678)")

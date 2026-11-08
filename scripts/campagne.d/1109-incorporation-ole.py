# Incorporation OLE (#1109) : OleCreateFromFile, GetClassFile, descripteurs de menu, OleTranslateAccelerator.
ok('"OleCreateFromFile" => fn ole_create_from_file' in (runtime_dlls / "ole32_inplace.rs").read_text()
   and '"OleSetMenuDescriptor"=>fn ole_set_menu_descriptor' not in (runtime_dlls / "ole32.rs").read_text()
   and "insertion_depuis_un_fichier" in (SCRIPTS / "test-all.d" / "1109-incorporation-ole.sh").read_text(),
   "OLE : OleCreateFromFile, GetClassFile, menus et accélérateurs de l'activation sur place (#1109)")

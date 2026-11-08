# Objets OLE incorporés (#1236) : OleCreate, OleLoad, OleSave, OleRun, OleDraw, porte-avis.
ok('"OleCreate" => fn ole_create' in (runtime_dlls / "ole32_embed.rs").read_text()
   and '"OleRun"=>fn ole_s_ok' not in (runtime_dlls / "ole32.rs").read_text()
   and "objet_incorpore_cree_enregistre_et_recharge" in (SCRIPTS / "test-all.d" / "1236-objets-ole-incorpores.sh").read_text(),
   "OLE : OleCreate, OleLoad, OleSave, OleRun, OleDraw et porte-avis (#1236)")

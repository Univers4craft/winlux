# Stockage composé (#1235) : StgCreateDocfile, StgOpenStorage, IStorage, ILockBytes au format [MS-CFB].
ok('"StgCreateDocfile" => fn stg_create_docfile' in (runtime_dlls / "ole32_storage.rs").read_text()
   and "ole32_storage::exports()" in (runtime_dlls / "mod.rs").read_text()
   and "docfile_ecrit_puis_relu_et_reconnu" in (SCRIPTS / "test-all.d" / "1235-stockage-compose.sh").read_text(),
   "Stockage composé : docfile [MS-CFB] créé, relu et reconnu par StgIsStorageFile (#1235)")

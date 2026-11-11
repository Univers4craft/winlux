# Inno Setup : dossier d'installation par défaut toujours absolu (#1862).
_inno = (ROOT / "runtime/src/setup/inno.rs").read_text(encoding="utf-8")
ok("fn rooted_app_dir(" in _inno and "fn dossier_par_defaut_toujours_absolu" in _inno,
   "Inno : DefaultDirName sans lecteur placé sous Program Files (#1862)")

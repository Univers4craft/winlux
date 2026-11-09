# UAC (#1382) : l'éditeur d'un .exe signé (Authenticode vérifié) est affiché, « inconnu » seulement sans signature valide.
ok("fn examine_pe" in (ROOT / "runtime/src/setup/appx_signature.rs").read_text()
   and "--publisher" in (ROOT / "runtime/src/uac.rs").read_text()
   and '"--publisher"' in (SCRIPTS / "winlux-uac.py").read_text(),
   "UAC : éditeur vérifié lu dans la signature Authenticode du .exe (#1382)")

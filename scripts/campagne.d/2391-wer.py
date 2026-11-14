# wer.dll intégrée : rapports d'erreurs créés et soumis sans aucune
# transmission (OneDrive, #2391). Le comportement est vérifié par le test
# unitaire nommé ci-dessous ; ce contrôle vérifie les noms.
_w = (ROOT / "runtime/src/dlls/wer.rs").read_text()
_d = (ROOT / "runtime/src/dlls/mod.rs").read_text()
ok('name: "wer"' in _d and all(f'"{n}"' in _w for n in (
    "WerReportCreate", "WerReportSetParameter", "WerReportAddDump",
    "WerReportSubmit", "WerReportCloseHandle")),
   "wer.dll sert les cinq fonctions importées par OneDriveSetup (#2391)")
ok("fn rapport_cree_soumis_sans_envoi()" in _w,
   "test : rapport soumis, résultat WerDisabled, rien transmis (#2391)")

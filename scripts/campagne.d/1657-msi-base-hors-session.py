# msi.dll : base ouverte hors session, résumé du paquet et empreinte de fichier (#1657).
src = (ROOT / "runtime/src/msi/summary.rs").read_text(encoding="utf-8")
leg = (ROOT / "runtime/src/dlls/legacy_dlls.rs").read_text(encoding="utf-8")
ok(all(f'"{n}"' in src for n in ["MsiOpenDatabaseW", "MsiDatabaseCommit", "MsiGetSummaryInformationA",
                                 "MsiSummaryInfoGetPropertyA", "MsiSummaryInfoGetPropertyCount",
                                 "MsiSummaryInfoPersist", "MsiSummaryInfoSetPropertyA",
                                 "MsiSummaryInfoSetPropertyW", "MsiGetFileHashW"])
   and "crate::msi::summary::exports()" in leg,
   "msi.dll : MsiOpenDatabase, résumé et MsiGetFileHash servis, aussi par numéro d'ordre (#1657)")

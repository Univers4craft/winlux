# msi.dll servie aussi par ordinal (WixCA.dll, actions personnalisées WiX), d'après Msi.Lib du SDK public (#1607).
tsv = (runtime_dlls / "msi_ordinals.tsv").read_text(encoding="utf-8")
legacy = (runtime_dlls / "legacy_dlls.rs").read_text(encoding="utf-8")
ok("26\tMsiDatabaseGetPrimaryKeysW" in tsv and "49\tMsiGetActiveDatabase" in tsv and "fn with_msi_ordinals" in legacy,
   "msi.dll : ordinaux importés par WixCA.dll servis (#1607)")

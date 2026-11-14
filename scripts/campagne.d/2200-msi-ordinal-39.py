# #2200 : msi!#39 (MsiEnumComponentQualifiersW d'après Msi.Lib du SDK public)
# servi ; l'outil de déploiement d'Office s'arrêtait en 0xC06D007F.
src = (runtime_dlls / "legacy_dlls.rs").read_text(encoding="utf-8")
table = (runtime_dlls / "msi_ordinals.tsv").read_text(encoding="utf-8")
ok('"MsiEnumComponentQualifiersW" => fn msi_enum_component_qualifiers' in src
   and "39\tMsiEnumComponentQualifiersW" in table,
   "msi : ordinal 39 (MsiEnumComponentQualifiersW) servi (#2200)")

# #1855 qwave.dll intégrée (discord_voice.node) ; #1860 famille asymétrique de bcrypt (ISCC).
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
q = (runtime_dlls / "qwave.rs").read_text(encoding="utf-8")
b = (runtime_dlls / "bcrypt.rs").read_text(encoding="utf-8")
ok('name: "qwave"' in m and '"QOSCreateHandle" => fn QOSCreateHandle' in q and '"QOSAddSocketToFlow"' in q,
   "qwave.dll sert l'API qWAVE (#1855)")
ok(all(f'"{n}"=>' in b for n in ("BCryptFinalizeKeyPair", "BCryptGenerateKeyPair", "BCryptImportKeyPair", "BCryptSignHash", "BCryptVerifySignature")),
   "bcrypt sert la famille asymétrique qu'ISCmplr.dll résout au chargement (#1860)")

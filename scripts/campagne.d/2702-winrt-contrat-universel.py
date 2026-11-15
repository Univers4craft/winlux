# #2702 : UniversalApiContract présent jusqu'à 10.0, comme le build 19041.
# Le comportement est gardé par le test unitaire nommé.
ai = (runtime_dlls / "winrt_apiinfo.rs").read_text(encoding="utf-8")
ok("fn contrat_universel_2702" in ai, "contrat UniversalApiContract d'ApiInformation (#2702)")

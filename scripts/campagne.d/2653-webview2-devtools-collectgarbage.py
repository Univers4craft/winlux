# #2653 : HeapProfiler.collectGarbage répond par CallDevToolsProtocolMethod.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn devtools_ramasse_miettes_2653" in wv, "test de HeapProfiler.collectGarbage (#2653)")

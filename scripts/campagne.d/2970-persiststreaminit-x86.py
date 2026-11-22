# Le contrôle WebBrowser rend IPersistStreamInit à un hôte x86 par un
# mandataire, pour qu'il l'initialise par InitNew ou Load (#2970).
_wb = (ROOT / "runtime" / "src" / "wow32" / "com_webbrowser.rs").read_text(encoding="utf-8")
ok('name: "IPersistStreamInit"' in _wb and "&IPERSISTSTREAMINIT," in _wb
   and "fn persist_stream_init_decrit_2970" in _wb,
   "IPersistStreamInit du contrôle WebBrowser est décrite pour les programmes x86 (#2970)")

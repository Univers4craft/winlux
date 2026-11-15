# #2228 : le contrôle navigateur web (CLSID_WebBrowser) montre le contrat
# qu'un installeur écrit dans son document. Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/dlls/webbrowser/tests.rs").read_text(encoding="utf-8")
ok("fn document_ecrit_par_write_montre_en_texte" in t, "contrôle navigateur : contrat écrit dans le document et montré (#2228)")

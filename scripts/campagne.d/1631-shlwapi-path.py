# #1631 : famille Path* de shlwapi complétée (Pythonwin, PathStripToRootW).
sp = (runtime_dlls / "shlwapi_path.rs").read_text(encoding="utf-8")
ok(all(f'"{n}" => fn' in sp for n in ("PathStripToRootW", "PathStripToRootA", "PathRelativePathToW",
   "PathCommonPrefixW", "PathQuoteSpacesW", "PathUnquoteSpacesW", "PathIsUNCW", "PathGetArgsW")),
   "shlwapi sert PathStripToRoot et le reste de la famille Path* (#1631)")

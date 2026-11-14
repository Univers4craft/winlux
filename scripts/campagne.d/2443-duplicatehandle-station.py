# #2443 : DuplicateHandle copie un handle de station de fenêtres ou de
# bureau. Le comportement est gardé par le test unitaire nommé.
ew = (ROOT / "runtime" / "src" / "dlls" / "electron_win32.rs").read_text(encoding="utf-8")
ok("fn duplicatehandle_copie_station_et_bureau" in ew, "test de DuplicateHandle sur station et bureau (#2443)")
ok("pub(crate) fn duplicate_user_object" in ew, "les stations et bureaux sont duplicables (#2443)")

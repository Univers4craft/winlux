# user32 : GetWindowDisplayAffinity et SetWindowDisplayAffinity pour OBS Studio (#2226).
_e = (ROOT / "runtime/src/dlls/electron_win32.rs").read_text()
ok('"GetWindowDisplayAffinity" => fn GetWindowDisplayAffinity' in _e
   and '"SetWindowDisplayAffinity" => fn SetWindowDisplayAffinity' in _e,
   "user32 exporte GetWindowDisplayAffinity et SetWindowDisplayAffinity (#2226)")

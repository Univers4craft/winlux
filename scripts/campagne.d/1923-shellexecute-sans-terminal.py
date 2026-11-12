# ShellExecute d'un programme console caché (SW_HIDE, SEE_MASK_NO_CONSOLE) ou lancé par un service : pas de terminal visible (#1923).
_s = (ROOT / "runtime/src/dlls/shell32.rs").read_text(encoding="utf-8")
ok("fn hidden_launch(show: i32, mask: u32) -> bool" in _s
   and 'cmd.env("WINLUX_NO_TERMINAL", "1");' in _s
   and 'cmd.env_remove("WINLUX_SERVICE_NAME");' in _s
   and "fn lancement_cache_sans_terminal" in _s,
   "ShellExecute caché ou depuis un service : sans terminal visible, avec test (#1923)")

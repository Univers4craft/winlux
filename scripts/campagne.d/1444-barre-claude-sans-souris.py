# #1444 : la fenêtre en attente de sa propre barre de titre (Electron) est
# réessayée à chaque message distribué par son thread, pas seulement au
# passage de la souris.
src = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
ok("unsafe fn recheck_own_frame() {" in src and "fn frame_retry_delay(" in src,
   "Double barre de titre de Claude sans geste de la souris (#1444)")

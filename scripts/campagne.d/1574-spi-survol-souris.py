# #1574 : SPI_GETMOUSEHOVERTIME/WIDTH/HEIGHT rendent 400 ms et 4 px (barres d'outils WinForms).
src = (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
ok("0x0062 | 0x0064 | 0x0066 if !data.is_null()" in src,
   "user32 : délai et zone de survol de la souris (#1574)")

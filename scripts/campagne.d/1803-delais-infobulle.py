# #1803 : l'infobulle rend des délais non nuls (TTM_GETDELAYTIME), sans quoi
# WinForms (Paint.NET, Tab dans la barre d'outils) plante sur un minuteur à 0 ;
# les délais clavier/menus de SystemParametersInfo ne restent plus à 0.
comctl_src = (runtime_dlls / "controls" / "comctl.rs").read_text(encoding="utf-8")
ok("fn tooltip_proc(" in comctl_src and "0x0415 => tooltip_delay(hwnd, wp)" in comctl_src
   and "fn infobulle_delais_par_defaut_et_reglages" in (runtime_dlls / "controls" / "tests.rs").read_text(encoding="utf-8"),
   "comctl32 : délais d'infobulle par défaut et réglables (#1803)")
ok("fn delais_clavier_menus_et_messages" in (runtime_dlls / "user32.rs").read_text(encoding="utf-8"),
   "user32 : délais clavier, menus et messages de SystemParametersInfo non nuls (#1803)")

# Un cran de molette fait défiler la liste de l'Explorateur tout de suite, et
# la boucle de gui_demo n'accélère son tic que pendant un défilement, une
# vignette ou une copie ; au repos il reste à 500 ms (#3155). Le comportement
# est vérifié par explorer_regression.exe (wheel_tick_checks).
explorer_3155 = (ROOT / "testexe/src/shell/explorer.rs").read_text(encoding="utf-8")
print("#3155 : wheel_tick_checks appelé par regression_checks :",
      "    wheel_tick_checks();" in explorer_3155)
ok("unsafe fn wheel_tick_checks()" in explorer_3155 and "    wheel_tick_checks();" in explorer_3155
   and "un cran de molette fait defiler la liste tout de suite (#3155)" in explorer_3155
   and "tic a 500 ms au repos, accelere seulement pendant un defilement (#3155)" in explorer_3155,
   "Explorateur : un cran de molette défile tout de suite, tic de 500 ms au repos (#3155)")

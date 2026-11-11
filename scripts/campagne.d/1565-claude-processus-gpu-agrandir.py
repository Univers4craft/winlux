# #1565 : le processus GPU de Chromium mourait (appel à 0 dans Mesa) après
# Agrandir. Les connexions X des contextes GL détruits restent ouvertes
# (l'écran GLX de Mesa vit avec elles), ResizeBuffers crée le cadre du
# tampon arrière dans le contexte de la chaîne, et la fin d'une chaîne
# laisse au thread le contexte du périphérique.
dxgi = (ROOT / "runtime/src/dlls/dxgi.rs").read_text(encoding="utf-8")
drop = dxgi[dxgi.index("impl Drop for GlContext"):]
drop = drop[:drop.index("\n}\n")]
ok("static SPARE_DISPLAYS" in dxgi
   and "XCloseDisplay" not in drop
   and "fn make_own_context_current(" in dxgi
   and "fn vue_liberee_apres_la_fin_de_sa_chaine_x11()" in dxgi
   and "fn tampon_redimensionne_sous_un_autre_contexte_reste_presente_x11()" in dxgi,
   "Claude agrandi : le processus GPU survit au redimensionnement (#1565)")

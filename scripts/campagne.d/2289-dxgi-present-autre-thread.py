# DXGI : Present depuis un autre thread que le créateur de la chaîne, aperçu d'OBS Studio (#2289).
# Le comportement est vérifié sous Xvfb par present_depuis_un_autre_thread_que_le_createur_x11.
_d = (ROOT / "runtime/src/dlls/dxgi.rs").read_text()
ok("fn release_to_creator" in _d and "fn present_depuis_un_autre_thread_que_le_createur_x11" in _d,
   "la chaîne d'échange rend au thread créateur son contexte GL, Present marche depuis le thread de rendu (#2289)")

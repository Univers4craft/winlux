# DXGI : Present depuis un autre thread que le créateur de la chaîne, aperçu d'OBS Studio (#2289).
_d = (ROOT / "runtime/src/dlls/dxgi.rs").read_text()
ok("fn release_to_creator" in _d and "ctx.release_to_creator(creator);" in _d,
   "la chaîne d'échange rend au thread créateur son contexte GL, Present marche depuis le thread de rendu (#2289)")

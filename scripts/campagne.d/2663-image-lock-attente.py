# Le test image_lock attend, borné, que le verrou tombe après la fermeture
# du descripteur : un enfant entre fork et exec le tient encore (#2663).
_s = (ROOT / "runtime/src/image_lock.rs").read_text()
_t = _s[_s.index("fn image_chargee_refuse_l_ecriture"):]
ok("while in_use(&moved)" in _t,
   "test image_lock : attente bornée du verrou levé (#2663)")

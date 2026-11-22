# Le test image_lock attend, borné, que le verrou tombe après la fermeture
# du descripteur : un enfant entre fork et exec le tient encore (#2663).
# Ce contrôle vérifie le nom du test qui porte ce comportement.
_s = (ROOT / "runtime/src/image_lock.rs").read_text(encoding="utf-8")
ok("fn image_chargee_refuse_l_ecriture(" in _s,
   "test image_lock : attente bornée du verrou levé (#2663)")

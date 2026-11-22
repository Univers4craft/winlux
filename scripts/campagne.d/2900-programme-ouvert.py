# Un installeur ne réécrit jamais l'exe d'un programme ouvert : il demande de le fermer, ou s'arrête (#2900).
_m = (ROOT / "runtime/src/setup/mod.rs").read_text()
ok("fn programme_ouvert_jamais_reecrit" in _m,
   "un programme ouvert n'est jamais réécrit sur place par l'installation (#2900)")

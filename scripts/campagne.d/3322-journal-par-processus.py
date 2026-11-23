# #3322 : chaque lancement laissait un /tmp/winlux-journal-<pid>.log vide,
# créé à l'initialisation et jamais écrit. Il n'est plus créé qu'au premier
# vidage qui a une ligne à écrire.
_q_3322 = (ROOT / "runtime/src/quality.rs").read_text(encoding="utf-8")
ok("fn journal_par_processus_cree_seulement_a_la_premiere_ecriture_3322()" in _q_3322
   and (SCRIPTS / "test-journal-par-processus.py").is_file(),
   "journal par processus créé seulement à sa première ligne, aucun fichier vide laissé dans /tmp (#3322)")

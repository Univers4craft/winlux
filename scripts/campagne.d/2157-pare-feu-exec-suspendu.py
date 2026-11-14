# Pare-feu : l'exec d'un programme filtré en sortie attend son rattachement au cgroup (fanotify), aucune socket hors filtre (#2157).
_fw = (ROOT / "runtime/src/firewall.rs").read_text(encoding="utf-8")
ok("fn guard_execs(" in _fw and "FAN_OPEN_EXEC_PERM" in _fw, "le service suspend l'exec des programmes filtrés (fanotify) (#2157)")
ok("fn exec_suspendu_jusqu_a_la_reponse(" in _fw, "test réel : l'exec attend la réponse du service (#2157)")
ok("FAN_DENY" not in _fw, "le pare-feu n'empêche jamais un programme de démarrer (#2157)")
ok("fn follow_execs(" in _fw, "sans fanotify, le connecteur proc reste en secours (#2157)")

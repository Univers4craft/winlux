# Pare-feu : un programme filtré en sortie est rattaché à son cgroup dès l'exec, avant son premier octet (#2157).
_fw = (ROOT / "runtime/src/firewall.rs").read_text(encoding="utf-8")
_pr = (ROOT / "runtime/src/main.rs").read_text(encoding="utf-8")
ok("fn watch_execs(" in _fw and "fn exec_events_give_the_new_program_tgid(" in _fw and "PROC_CN_MCAST_LISTEN" in _fw, "le service rattache au cgroup dès l'exec (connecteur proc) (#2157)")
ok("pub fn wait_for_cgroup()" in _fw and "firewall::wait_for_cgroup();" in _pr, "un .exe filtré attend son cgroup avant son propre code (#2157)")
ok("fn publish_targets(" in _fw and "fn published_targets_round_trip(" in _fw and "0o644" in _fw, "liste des programmes filtrés lisible par les comptes (#2157)")

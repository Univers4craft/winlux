# Une compilation qui dévore la RAM ne fige plus tout le bureau (#2132).
_i = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
ok("echo 1000 >/sys/kernel/mm/lru_gen/min_ttl_ms" in _i, "init : le noyau garde la dernière seconde de travail en mémoire, sinon il tue (#2132)")
ok("echo -900 >/proc/self/oom_score_adj" in _i and "/usr/bin/Xorg \"$@\"" in _i, "init : Xorg épargné par le tueur mémoire du noyau (#2132)")
ok(_i.count("oom_score_adj") == 1, "init : aucun réglage oom_score_adj hérité par la session ou ses applications (#2132)")

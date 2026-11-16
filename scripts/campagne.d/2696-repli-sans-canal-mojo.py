# #2696 : un programme qui se relance lui-même en `--type=renderer` sans
# être Chromium (pas de canal Mojo) n'est pas relancé avec `--no-sandbox`.
# Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/sandbox_fallback.rs").read_text(encoding="utf-8")
ok("fn programme_qui_se_relance_sans_etre_chromium_2696" in t, "repli sans bac à sable réservé aux enfants de contenu de Chromium (#2696)")

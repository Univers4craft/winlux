# #2562 : la ligne du repli #1733 nomme l'enfant terminé, son type et le
# processus qui l'a terminé. Le comportement est gardé par le test nommé.
t = (ROOT / "runtime/src/sandbox_fallback.rs").read_text(encoding="utf-8")
ok("fn cause_du_repli_journalisee" in t, "repli #1733 : cause journalisée (#2562)")

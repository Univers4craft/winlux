# #2566 : la relance #1733 (execv, même pid) n'écrase plus le journal
# d'API d'avant la relance. Le comportement est gardé par le test nommé.
t = (ROOT / "runtime/src/thunks.rs").read_text(encoding="utf-8")
ok("fn journal_d_api_relance_sans_ecraser" in t, "WINLUX_APITRACE : relance sans écraser le journal (#2566)")

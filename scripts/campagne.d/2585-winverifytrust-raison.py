# #2585 : un refus de WinVerifyTrust est journalisé avec le fichier et la
# raison, sans variable de diagnostic. Gardé par le test unitaire nommé.
t = (runtime_dlls / "wintrust.rs").read_text(encoding="utf-8")
ok("fn refus_journalise_avec_sa_raison" in t, "WinVerifyTrust : refus journalisé avec sa raison (#2585)")

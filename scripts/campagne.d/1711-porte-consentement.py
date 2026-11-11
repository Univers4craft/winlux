# #1711 : une fenêtre de consentement qui attend arrête toute nouvelle saisie simulée.
saisie = (runtime_dlls / "saisie.rs").read_text(encoding="utf-8")
consentement = [(ROOT / "scripts" / n).read_text(encoding="utf-8") for n in ("winlux-permission.py", "winlux-uac.py")]
garde = saisie[saisie.index("pub fn consent_guard()"):]
garde = garde[:garde.index("\n}\n")]
ok('shared_lock(&path.with_extension("porte"))?' in garde and garde.index('with_extension("porte")') < garde.index("shared_lock(&path);"),
   "saisie simulée : la porte est prise avant le verrou principal, refus si une fenêtre l'occupe (#1711)")
ok(all('os.path.splitext(path)[0] + ".porte"' in t and "def hold_consent_lock(timeout=10.0):" in t
       and t.index('".porte", timeout)') < t.index("main = _lock_exclusive(path, timeout)") for t in consentement),
   "fenêtres de consentement : porte exclusive d'abord, puis attente des envois en cours (#1711)")

# #1696 : une saisie simulée ne peut jamais répondre à une fenêtre de consentement.
saisie = (runtime_dlls / "saisie.rs").read_text(encoding="utf-8")
consentement = [(ROOT / "scripts" / n).read_text(encoding="utf-8") for n in ("winlux-permission.py", "winlux-uac.py")]
ok("libc::LOCK_SH | libc::LOCK_NB" in saisie and "custom_flags(libc::O_NOFOLLOW)" in saisie and "let Some(_consent) = consent_guard() else" in saisie
   and saisie.index("consent_guard() else") < saisie.index("inj.apply(events)"),
   "saisie simulée : verrou partagé tenu du contrôle jusqu'à la fin de l'envoi (#1696)")
ok(all("fcntl.LOCK_EX" in t and 'result["lock"] = hold_consent_lock()' in t
       and t.index("hold_consent_lock()\n") < t.index("window.show_all()", t.index("hold_consent_lock()\n")) for t in consentement),
   "fenêtres d'autorisation et d'élévation : verrou exclusif pris avant de s'afficher (#1696)")
ok(all('f"/tmp/winlux-consent-{number}.lock"' in t for t in consentement)
   and 'format!("/tmp/winlux-consent-{number}.lock")' in saisie,
   "saisie simulée et fenêtres de consentement partagent le même verrou par affichage (#1696)")
ok(all("os.O_NOFOLLOW" in t and "st_uid not in (os.getuid(), 0)" in t for t in consentement)
   and "meta.uid() != me && meta.uid() != 0" in saisie,
   "verrou de consentement : ni lien symbolique ni fichier posé par un autre compte (#1696)")

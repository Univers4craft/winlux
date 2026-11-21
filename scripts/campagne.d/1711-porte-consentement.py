# #1711 : une fenêtre de consentement qui attend arrête toute nouvelle saisie simulée.
saisie = (runtime_dlls / "saisie.rs").read_text(encoding="utf-8")
consentement = [(ROOT / "scripts" / n).read_text(encoding="utf-8") for n in ("winlux-permission.py", "winlux-uac.py")]
# Noms : consent_guard, son test verrou_des_fenetres_de_consentement et le
# suffixe « porte » du fichier de verrou (#2711).
ok("pub fn consent_guard()" in saisie and "fn verrou_des_fenetres_de_consentement" in saisie
   and '"porte"' in saisie,
   "saisie simulée : la porte est prise avant le verrou principal, refus si une fenêtre l'occupe (#1711)")
# Noms : hold_consent_lock et le suffixe « .porte » dans les deux fenêtres (#2711).
ok(all("def hold_consent_lock(" in t and '".porte"' in t and "def _lock_exclusive(" in t for t in consentement),
   "fenêtres de consentement : porte exclusive d'abord, puis attente des envois en cours (#1711)")

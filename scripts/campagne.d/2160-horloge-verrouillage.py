# Écran de verrouillage : l'horloge et l'invite finissent au-dessus du prisme
# du fond Aurore par défaut (sommet vers la ligne 303 en 1920x1080, #2160).
_l = (ROOT / "scripts/winlux-lock.py").read_text(encoding="utf-8")
_t = (ROOT / "scripts/test-lock-wake.py").read_text(encoding="utf-8")
ok("LOGO_TOP_1080 = 300" in _l and "+ LOGO_TOP_1080 * cover" in _l, "horloge réduite sous le sommet du prisme Aurore (#2160)")
ok("def wallpaper_apex" in _t and "367 * cover" not in _t, "test : sommet du prisme mesuré dans le fond par défaut (#2160)")

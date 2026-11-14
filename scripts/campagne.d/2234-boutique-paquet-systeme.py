# #2234 : la Boutique simule la purge sans droits avant la demande
# d'autorisation et explique pourquoi un composant de Winlux reste installé.
_s = (ROOT / "scripts/winlux-store.py").read_text(encoding="utf-8")
_r = _s[_s.index("    def removal_blocker(self, ref):"):_s.index("class Flatpak(Source):")]
ok('"apt-get", "-s"' in _r and _r.index("self.removal_blocker(ref)") < _r.index('self.apt(["purge"'),
   "Boutique : simulation sans droits avant la purge apt (#2234)")
ok(_s.count('"system_part":') == 4, "Boutique : message « fait partie de Winlux » en fr/en/es/de (#2234)")
ok("test-store-paquet-systeme.py" in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
   "CI : test de la Boutique sur un paquet nécessaire au système (#2234)")

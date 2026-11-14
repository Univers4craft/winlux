# #2235 : les sorties d'apt analysées par la Boutique sont demandées en
# LC_ALL=C ; LANG=C seul cédait au LC_ALL de la session.
_s = (ROOT / "scripts/winlux-store.py").read_text(encoding="utf-8")
_a = _s[_s.index("class Apt(Source):"):_s.index("    def refresh_lists(self):")]
ok('"LANG": "C"' not in _a and _a.count('"LC_ALL": "C"') >= 3,
   "Boutique : apt list, apt-cache policy et rdepends analysés en LC_ALL=C (#2235)")
ok("test-store-locale-session.py" in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
   "CI : Boutique sous une session LC_ALL française (#2235)")

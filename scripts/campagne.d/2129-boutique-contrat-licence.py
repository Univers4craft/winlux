# #2129 : la Boutique fait accepter le contrat de licence d'un installateur
# Windows avant de l'installer sans interface ; comportement vérifié par
# test-store-licence.py (CI), la page elle-même par test-licence-installateurs.sh.
_ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
ok((ROOT / "scripts/test-store-licence.py").is_file() and "test-store-licence.py" in _ci,
   "Boutique : contrat de licence accepté avant une installation silencieuse, testé en CI (#2129)")

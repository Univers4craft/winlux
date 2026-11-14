# #2184 : sur un système neuf, la Boutique télécharge les listes apt d'elle-même.
# Comportement vérifié par test-store-listes-neuves.py (CI) : hors ligne rien,
# listes absentes un apt-get update par le relais sans autorisation, puis rien.
_ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
ok((ROOT / "scripts/test-store-listes-neuves.py").is_file() and "test-store-listes-neuves.py" in _ci,
   "Boutique : listes apt d'un système neuf testées en CI (#2184)")

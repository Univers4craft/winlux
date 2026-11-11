# Fenêtre « Autoriser … ? » : une commande lancée d'un terminal (pip) dit sur stderr qu'elle attend une réponse (#1592).
ok("def notice(" in (ROOT / "scripts/winlux-permission.py").read_text(encoding="utf-8")
   and "    notice(cap, name)" in (ROOT / "scripts/winlux-permission.py").read_text(encoding="utf-8"),
   "Autorisations : le terminal est prévenu qu'une fenêtre attend une réponse (#1592)")

# #2757 : le rapport de plantage nomme le binaire winlux qui a tourné (sa
# version et le début de son sha256), pas seulement l'image installée. Le
# comportement est gardé par scripts/test-report.py (faux binaire posé).
rapport = (ROOT / "scripts" / "winlux-report.py").read_text(encoding="utf-8")
essai = (ROOT / "scripts" / "test-report.py").read_text(encoding="utf-8")
ok("def winlux_binary" in rapport and "image installée" in essai and "#2757" in essai,
   "rapport : version et empreinte du binaire qui a planté (#2757)")

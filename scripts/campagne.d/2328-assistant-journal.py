# Assistant d'installation : la sortie du runtime est écrite au fil de l'eau dans last-launch.log (#2328).
_t = (ROOT / "scripts/test-assistant-journal.sh").read_text(encoding="utf-8")
ok("#2328" in _t and "scripts/test-assistant-journal.sh" in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
   "assistant : journal écrit pendant l'installation, trace gardée si l'assistant est tué, vérifié sous Xvfb (#2328)")

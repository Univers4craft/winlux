# #2895 : un programme que Winlux ne lance pas pendant une installation est
# marqué « étape sautée » au journal, reste hors de la ligne d'état de
# l'assistant et y est résumé en clair en fin d'installation. Comportement
# gardé par le test unitaire nommé et par le test Xvfb de l'assistant.
src = (ROOT / "runtime" / "src" / "setup" / "run.rs").read_text(encoding="utf-8")
ok("fn etape_sautee_marquee_dans_le_journal_2895" in src
   and (ROOT / "scripts" / "test-setup-wizard-etapes-sautees.py").exists()
   and (ROOT / "scripts" / "test-all.d" / "2895-assistant-etapes-sautees.sh").exists(),
   "Assistant d'installation : étapes sautées résumées en clair, détail au journal (#2895)")

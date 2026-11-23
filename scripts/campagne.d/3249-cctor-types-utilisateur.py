# #3249 : le constructeur statique d'un type du programme fait attendre les
# autres fils jusqu'à sa fin, et son échec est relevé à chaque accès sous
# forme de la même TypeInitializationException (ECMA-335 §II.10.5.3.3). Le
# comportement est gardé par le témoin nommé, lancé par scripts/test-bcl.sh.
t = ROOT / "bcl" / "temoins" / "InitialisationEchouee.cs"
texte = t.read_text(encoding="utf-8") if t.exists() else ""
ok("Code de sortie attendu : 200." in texte, "témoin InitialisationEchouee : constructeur statique attendu et échec relevé (#3249)")

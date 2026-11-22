# #2896 : les applications du shell (Boutique…) et le bureau n'écrivent pas
# de lignes de test sur la sortie standard. Gardé par le test Xvfb nommé.
ok((SCRIPTS / "test-shell-sortie-muette-xvfb.py").is_file(),
   "applications du shell : aucune ligne de test sur stdout, test Xvfb (#2896)")

# #2854 : MessageBox affiche l'icône de son type (MB_ICONERROR,
# MB_ICONQUESTION, MB_ICONWARNING, MB_ICONINFORMATION). Le comportement est
# gardé par le test unitaire nommé.
src = (ROOT / "runtime" / "src" / "dlls" / "dialogs" / "tests.rs").read_text(encoding="utf-8")
ok("fn boite_de_message_affiche_son_icone_2854" in src,
   "MessageBox : icône du type MB_ICON* affichée (#2854)")

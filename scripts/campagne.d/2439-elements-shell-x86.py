# Éléments du shell (IShellItem, IShellItemArray) décrits pour le code x86
# (#2439, lot A de #2428). Le comportement est vérifié par les tests
# unitaires nommés ci-dessous ; ce contrôle vérifie qu'ils existent.
import re as _re
_s = (ROOT / "runtime/src/wow32/com_shell.rs").read_text()
ok(_re.search(r"fn elements_du_shell_decrits_pour_le_code_x86\(\)", _s) is not None,
   "test : IShellItem et IShellItemArray décrits pour le x86 (#2439)")
ok(_re.search(r"fn fonctions_du_lot_a_decrites_a_la_taille_de_stdcall\(\)", _s) is not None,
   "test : fonctions shell32 du lot A à la taille de stdcall.tsv (#2439)")

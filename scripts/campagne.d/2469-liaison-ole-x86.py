# Contexte de liaison, table des objets actifs et monikers rendus au code
# x86 par un mandataire (#2469, lot C de #2428). Le comportement est
# vérifié par les tests unitaires nommés ci-dessous ; ce contrôle vérifie
# qu'ils existent.
import re as _re
_o = (ROOT / "runtime/src/wow32/com_ole.rs").read_text()
ok(_re.search(r"fn contexte_de_liaison_decrit_pour_le_code_x86\(\)", _o) is not None,
   "test : IBindCtx et IRunningObjectTable décrits pour le x86 (#2469)")
ok(_re.search(r"fn fonctions_du_lot_c_decrites_a_la_taille_de_stdcall\(\)", _o) is not None,
   "test : fonctions ole32 du lot C à la taille de stdcall.tsv (#2469)")

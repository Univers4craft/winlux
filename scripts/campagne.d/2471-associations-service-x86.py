# Associations, objet de données et service d'un objet x86 passent par un
# mandataire (#2471, lot E de #2428). Le comportement est vérifié par le
# test unitaire nommé ci-dessous ; ce contrôle vérifie qu'il existe.
import re as _re
_s = (ROOT / "runtime/src/wow32/com_shell.rs").read_text()
ok(_re.search(r"fn fonctions_du_lot_e_decrites_a_la_taille_de_stdcall\(\)", _s) is not None,
   "test : SHCreateDataObject, AssocCreate, IUnknown_QueryService décrites pour le x86 (#2471)")

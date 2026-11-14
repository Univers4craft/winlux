# Magasins de propriétés rendus au code x86 par un mandataire (#2451, lot B
# de #2428). Le comportement est vérifié par le test unitaire nommé
# ci-dessous ; ce contrôle vérifie qu'il existe.
import re as _re
_c = (ROOT / "runtime/src/wow32/com.rs").read_text()
ok(_re.search(r"fn magasins_de_proprietes_decrits\(\)", _c) is not None,
   "test : magasins de propriétés décrits pour le x86 (#2451)")

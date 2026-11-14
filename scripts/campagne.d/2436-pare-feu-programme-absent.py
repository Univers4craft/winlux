# Pare-feu : « status » dit si chaque programme filtré est présent et
# surveillé, ou absent avec sa règle en attente (#2436). Le comportement
# est vérifié par le test unitaire nommé ci-dessous.
import re as _re
_f = (ROOT / "runtime/src/firewall.rs").read_text()
ok(_re.search(r"fn status_distingue_programme_absent\(\)", _f) is not None,
   "test : status distingue un programme filtré absent (#2436)")

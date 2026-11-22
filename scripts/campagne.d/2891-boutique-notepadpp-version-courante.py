# #2891 : « Testés sur Winlux » propose Notepad++ dans la version courante de
# l'éditeur (8.9.9.1, testée par le banc le 22 novembre), à l'adresse
# officielle et avec l'empreinte publiée par l'éditeur.
import json
import re

_valides = json.loads((ROOT / "integration/store-windows-valides.json").read_text(encoding="utf-8"))
_npp = next((a for a in _valides["apps"] if a["id"] == "notepadplusplus"), {})
_inst = _npp.get("installer", {})
ok(tuple(int(x) for x in _npp.get("version", "0").split(".")) >= (8, 9, 9, 1)
   and _inst.get("url", "").startswith("https://github.com/notepad-plus-plus/notepad-plus-plus/releases/download/v"
                                       + _npp.get("version", "") + "/")
   and re.fullmatch(r"[0-9a-f]{64}", _inst.get("sha256", "")) is not None,
   "Boutique : Notepad++ de « Testés sur Winlux » dans la version courante de l'éditeur (#2891)")

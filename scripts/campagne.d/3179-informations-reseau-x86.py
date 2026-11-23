# Un programme x86 reçoit NetworkInformation, ses profils de connexion et
# leurs collections par des mandataires au lieu d'un refus (outil de
# déploiement d'Office, #3179) ; GetMany rend un tableau d'interfaces
# converti élément par élément (genre OutArr).
import re as _re
_rt = (ROOT / "runtime" / "src" / "wow32" / "com_winrt.rs").read_text(encoding="utf-8")
_com = (ROOT / "runtime" / "src" / "wow32" / "com.rs").read_text(encoding="utf-8")
_noms = ["INetworkInformationStatics", "IConnectionProfile", "IConnectionProfile2", "IConnectionCost",
         "IConnectionCost2", "IWlanConnectionProfileDetails", "IVectorView<ConnectionProfile>",
         "IIterable<ConnectionProfile>", "IIterator<ConnectionProfile>"]
_absents = [n for n in _noms if f'name: "{n}"' not in _rt]
_liste = _re.search(r"static ALL: &\[&Iface\] = &\[(.*?)\];", _rt, _re.S)
print(f"#3179 : interfaces absentes {_absents}, statics dans ALL : "
      f"{bool(_liste and '&INETWORKINFORMATIONSTATICS,' in _liste.group(1))}")
ok(not _absents and bool(_liste) and "&INETWORKINFORMATIONSTATICS," in _liste.group(1)
   and "OutArr(&'static Iface)" in _com and "fn tableau_d_interfaces_rendu_3179" in _com
   and "fn informations_reseau_decrites_3179" in _rt,
   "NetworkInformation et ses profils de connexion sont décrits pour les programmes x86 (#3179)")

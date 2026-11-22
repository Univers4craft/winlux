# Audacity (module réseau de Qt) demande IConnectionProfile2 au profil de
# GetInternetConnectionProfile ; refusée, elle faisait planter l'appli (#3031).
_ni = (ROOT / "runtime" / "src" / "dlls" / "network_information.rs").read_text(encoding="utf-8")
ok("0xe204_5145, 0x4c9f, 0x400c" in _ni and "static PROFILE2_VTBL" in _ni
   and "fn profil2_wlan_et_filaire_3031" in _ni,
   "le profil de connexion sert IConnectionProfile2, détails WLAN compris (#3031)")

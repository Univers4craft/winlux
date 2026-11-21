# Licence d'un bundle Burn dont BootstrapperApplicationData.xml est en UTF-16 (vc_redist, #2126).
_bu = (ROOT / "runtime/src/setup/burn.rs").read_text(encoding="utf-8")
# Test unitaire contrat_de_l_interface_standard : BootstrapperApplicationData.xml de vc_redist en UTF-16 LE/BE.
ok("fn xml_text(" in _bu and "fn contrat_de_l_interface_standard(" in _bu and "Encoding::Utf16Le" in _bu,
   "Burn : BootstrapperApplicationData.xml lu en UTF-16 comme en UTF-8 pour trouver la licence (#2126)")

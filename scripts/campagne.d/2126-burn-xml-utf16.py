# Licence d'un bundle Burn dont BootstrapperApplicationData.xml est en UTF-16 (vc_redist, #2126).
_bu = (ROOT / "runtime/src/setup/burn.rs").read_text(encoding="utf-8")
ok('let ba = xml_text(by_path("BootstrapperApplicationData.xml")?)' in _bu and "Encoding::Utf16Le" in _bu,
   "Burn : BootstrapperApplicationData.xml lu en UTF-16 comme en UTF-8 pour trouver la licence (#2126)")

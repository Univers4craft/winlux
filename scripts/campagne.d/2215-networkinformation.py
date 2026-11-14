# #2215 : Windows.Networking.Connectivity.NetworkInformation servie
# (Teams s'arrêtait sur son hresult_class_not_registered non rattrapé).
ro = (ROOT / "runtime" / "src" / "dlls" / "electron_win32.rs").read_text(encoding="utf-8")
ni = (ROOT / "runtime" / "src" / "dlls" / "network_information.rs").read_text(encoding="utf-8")
ok("network_information::activation_factory" in ro
   and '"Windows.Networking.Connectivity.NetworkInformation"' in ni
   and "fn st_internet_profile" in ni,
   "WinRT : NetworkInformation a sa fabrique et ses profils de connexion (#2215)")

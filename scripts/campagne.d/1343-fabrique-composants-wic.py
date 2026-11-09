# #1343 : IWICComponentFactory et CreateColorContext dans la fabrique WIC.
ok("IID_COMPONENT_FACTORY" in (runtime_dlls / "windowscodecs.rs").read_text(encoding="utf-8")
   and 'unsupported!("CreateColorContext")' not in (runtime_dlls / "windowscodecs.rs").read_text(encoding="utf-8"),
   "Fabrique de composants WIC et contextes de couleur (#1343)")

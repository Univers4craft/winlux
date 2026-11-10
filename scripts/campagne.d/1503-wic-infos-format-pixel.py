# #1503 : CreateComponentInfo rend IWICPixelFormatInfo2 pour un format de pixel (Paint.NET).
src = (runtime_dlls / "windowscodecs.rs").read_text(encoding="utf-8")
ok("PIXEL_FORMAT_INFO_VT" in src and "IID_PIXEL_FORMAT_INFO2" in src,
   "WIC : informations de format de pixel (#1503)")

# #1868 : GetEncoderInfo rend un IWICBitmapEncoderInfo (Paint.NET, Copier).
w = (runtime_dlls / "windowscodecs.rs").read_text(encoding="utf-8")
e = (runtime_dlls / "windowscodecs_encoder.rs").read_text(encoding="utf-8")
ok("IID_BITMAP_ENCODER_INFO" in w and "static ENCODER_INFO_VT" in w
   and "encoder_info_for_container(&CONTAINER_PNG, out)" in e,
   "l'encodeur PNG rend des informations d'encodeur, pas de décodeur (#1868)")

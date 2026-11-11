# #1868 : encodeur WIC PNG (Copier de Paint.NET : « Une erreur s'est
# produite en copiant vers le Presse-papiers »).
src = (runtime_dlls / "windowscodecs.rs").read_text(encoding="utf-8")
enc = (runtime_dlls / "windowscodecs_encoder.rs").read_text(encoding="utf-8")
ok("encoder::create_encoder as *const ()" in src and 'unsupported!("CreateEncoder")' not in src,
   "WIC : CreateEncoder fournit un encodeur (#1868)")
ok("fn frame_write_source" in enc and "fn frame_write_pixels" in enc and "png::Encoder" in enc,
   "WIC : encodeur PNG, WritePixels et WriteSource (#1868)")

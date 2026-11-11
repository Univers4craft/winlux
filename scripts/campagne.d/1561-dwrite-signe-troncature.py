# #1561 : IDWriteFactory::CreateEllipsisTrimmingSign rend un IDWriteInlineObject (barre d'état de Paint.NET).
src = (runtime_dlls / "dwrite.rs").read_text(encoding="utf-8")
ok("ellipsis_trimming_sign as *const ()" in src and "static SIGN_VT" in src,
   "DirectWrite : signe de troncature « … » (#1561)")

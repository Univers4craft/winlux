# #1669 : une cible WIC rend un bitmap par GetTarget et SetTarget le rétablit (empreinte du pinceau de Paint.NET).
src = (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
ok("Output::Hwnd(_) | Output::Dc { .. } | Output::Wic(_) =>" in src,
   "Direct2D : GetTarget d'une cible WIC n'est pas nul (#1669)")

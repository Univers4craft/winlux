# #2721 : ID2D1SvgDocument::GetViewportSize rend le pointeur de sa zone de
# retour (x64 MSVC). Le comportement est gardé par le test unitaire nommé.
sv = (runtime_dlls / "d2d1_svg.rs").read_text(encoding="utf-8")
ok("fn viewport_svg_par_rax_2721" in sv, "GetViewportSize rendue par RAX (#2721)")

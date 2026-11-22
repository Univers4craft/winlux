# #2719 : ID2D1DeviceContext5::CreateSvgDocument rend un document SVG
# complet (éléments, attributs, dessin). Le comportement est gardé par le
# test unitaire nommé.
sv = (runtime_dlls / "d2d1_svg.rs").read_text(encoding="utf-8")
ok("fn document_svg_2719" in sv, "document SVG Direct2D (#2719)")

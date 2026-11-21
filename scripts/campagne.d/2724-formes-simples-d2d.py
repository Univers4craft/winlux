# #2724 : ID2D1RectangleGeometry::GetRect, ID2D1RoundedRectangleGeometry::
# GetRoundedRect et ID2D1EllipseGeometry::GetEllipse ont leur propre table et
# ne tombent plus sur Open du chemin. Comportement gardé par le test
# dlls::d2d1::tests::formes_simples_rendent_leur_forme.
d2d_2724 = (ROOT / "runtime" / "src" / "dlls" / "d2d1.rs").read_text(encoding="utf-8")
ok("geometry_vtable!(RECT_VT, 18" in d2d_2724
   and "geometry_vtable!(ELLIPSE_VT, 18" in d2d_2724
   and "fn formes_simples_rendent_leur_forme" in d2d_2724,
   "Direct2D : GetRect, GetRoundedRect et GetEllipse rendent la forme de création (#2724)")

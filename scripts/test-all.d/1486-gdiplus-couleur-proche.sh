# #1486 : GdipGetNearestColor, appelé par les menus WinForms.
step "GDI+ : GdipGetNearestColor (#1486)" cargo test -q -p winlux -- couleur_la_plus_proche

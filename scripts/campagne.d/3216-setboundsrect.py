# Acrobat s'arrêtait sur gdi32!SetBoundsRect non implémentée, juste après
# avoir ouvert un PDF passé en ligne de commande (#3216).
_b = (ROOT / "runtime" / "src" / "dlls" / "gdi32_bounds.rs").read_text(encoding="utf-8")
_m = (ROOT / "runtime" / "src" / "dlls" / "mod.rs").read_text(encoding="utf-8")
print("exports :", '"SetBoundsRect"' in _b, '"GetBoundsRect"' in _b, "| branché :", "gdi32_bounds::exports()" in _m)
ok('"SetBoundsRect" => fn set_bounds_rect' in _b and "gdi32_bounds::exports()" in _m
   and "fn rectangle_d_accumulation_3216" in _b,
   "gdi32 sert SetBoundsRect et GetBoundsRect (#3216)")

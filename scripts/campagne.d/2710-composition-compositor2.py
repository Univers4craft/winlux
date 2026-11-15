# #2710 : le Compositor sert ICompositor2 (LayerVisual, CompositionBackdropBrush).
# Le comportement est gardé par le test unitaire nommé.
co = (runtime_dlls / "composition.rs").read_text(encoding="utf-8")
ok("fn compositeur2_2710" in co, "ICompositor2 du compositeur (#2710)")

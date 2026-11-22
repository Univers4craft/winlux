# #2224 : ContainerVisual et SpriteVisual répondent à IVisual2
# (ParentForTransform, RelativeOffsetAdjustment, RelativeSizeAdjustment).
# Teams s'arrêtait sur hresult_no_interface au premier lancement.
# Le comportement est gardé par le test unitaire nommé (les visuels de
# calque de #2710 servent aussi IVisual2).
src = (ROOT / "runtime" / "src" / "dlls" / "composition.rs").read_text(encoding="utf-8")
ok("fn visual2_des_conteneurs_2224" in src,
   "Composition : les visuels servent IVisual2 (#2224)")

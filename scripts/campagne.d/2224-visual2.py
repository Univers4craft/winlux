# #2224 : ContainerVisual et SpriteVisual répondent à IVisual2
# (ParentForTransform, RelativeOffsetAdjustment, RelativeSizeAdjustment).
# Teams s'arrêtait sur hresult_no_interface au premier lancement.
src = (ROOT / "runtime" / "src" / "dlls" / "composition.rs").read_text(encoding="utf-8")
ok('const IID_VISUAL2: Guid = iid("3052b611-56c3-4c3e-8bf3-f6e1ad473f06");' in src
   and src.count("(IID_VISUAL2, VISUAL2_VT.ptr())") == 2,
   "Composition : les visuels servent IVisual2 (#2224)")

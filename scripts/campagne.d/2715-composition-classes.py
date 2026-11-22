# #2715 : chaque création de ICompositor2 rend un objet de sa classe.
# Le comportement est gardé par le test unitaire nommé.
cl = (runtime_dlls / "composition_classes.rs").read_text(encoding="utf-8")
ok("fn classes_du_compositeur2_2715" in cl, "classes créées par ICompositor2 (#2715)")

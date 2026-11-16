# #2723 : les méthodes Direct2D qui rendent une structure par valeur rendent
# le pointeur caché dans RAX (x64 MSVC). Le test unitaire nommé appelle chaque
# emplacement servi par sa table et impose la forme des fonctions.
fx = (runtime_dlls / "d2d1_effects.rs").read_text(encoding="utf-8")
ok("fn structures_par_valeur_rendues_par_rax" in fx, "structures rendues par RAX (#2723)")

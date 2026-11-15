# #2696 : le repli sans bac à sable ne relance qu'un navigateur dont l'enfant
# tué est son propre exécutable (Acrobat lisait « --no-sandbox » comme un
# document). Le comportement est gardé par le test unitaire nommé.
src = (runtime_dlls.parent / "sandbox_fallback.rs").read_text(encoding="utf-8")
ok("fn moteur_heberge_dans_un_autre_executable_sans_relance_2696" in src,
   "repli sans bac à sable limité aux enfants du même exécutable (#2696)")

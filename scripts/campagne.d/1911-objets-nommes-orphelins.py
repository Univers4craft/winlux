# Objets nommés d'un processus tué : balayés à la première ouverture (#1911).
src = (ROOT / "runtime/src/dlls/named_objects.rs").read_text()
ok("fn sweep_orphans(" in src and "SWEPT.call_once(|| sweep_orphans(&dir))" in src,
   "objets nommés orphelins supprimés à la première ouverture (#1911)")
ok("fn orphelins_balayes_les_objets_tenus_gardes()" in src,
   "test : orphelins balayés, objets tenus gardés (#1911)")

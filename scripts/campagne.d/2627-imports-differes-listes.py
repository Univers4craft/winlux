# #2627 : une exception de chargement différé non gérée liste toutes les
# fonctions que le module importe en différé de la DLL manquante (lues dans
# l'INT de son descripteur), pour les connaître en un seul passage (#2623).
k = (ROOT / "runtime/src/dlls/kernel32_misc.rs").read_text(encoding="utf-8")
ok("fn delay_import_names" in k and "fn imports_differes_listes" in k,
   "Chargement différé : imports de la DLL manquante listés au journal (#2627)")

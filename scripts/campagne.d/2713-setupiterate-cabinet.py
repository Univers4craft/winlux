# #2713 : setupapi sert SetupIterateCabinetW (parcours et extraction d'un
# cabinet avec rappel), dont l'outil de déploiement d'Office 32 bits a besoin.
f = (ROOT / "runtime/src/dlls/setupapi_cab.rs").read_text(encoding="utf-8")
ok("fn cabinet_parcouru_et_extrait_2713" in f and "fn disposition_x86_des_structures_2713" in f,
   "setupapi : SetupIterateCabinetW parcourt et extrait un cabinet (#2713)")

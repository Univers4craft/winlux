# #2409 : CoInternetCreateSecurityManager rend à un programme x86 un
# mandataire IInternetSecurityManager ; sans lui, l'enfant RELAUNCHED de
# l'outil de déploiement d'Office fautait sur MapUrlToZone.
com = (ROOT / "runtime/src/wow32/com.rs").read_text(encoding="utf-8")
ok('name: "IInternetSecurityManager"' in com
   and "fn gestionnaire_de_zones_decrit" in com,
   "wow32 : IInternetSecurityManager décrite pour le code 32 bits (#2409)")

# #2459 : le NetworkListManager du runtime est rendu aux programmes x86 par
# un mandataire (l'outil de déploiement d'Office le crée) ; com32 l'appelle.
nlm = (ROOT / "runtime/src/wow32/com_nlm.rs").read_text(encoding="utf-8")
c32 = (ROOT / "testc/wow32/com32.c").read_text(encoding="utf-8")
ok('name: "INetworkListManager"' in nlm
   and "fn gestionnaire_de_reseaux_decrit" in nlm
   and "INetworkListManager::GetConnectivity" in c32,
   "wow32 : NetworkListManager servi aux programmes 32 bits (#2459)")

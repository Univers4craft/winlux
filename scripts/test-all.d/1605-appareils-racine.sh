# #1605 : appareil racine créé par l'installeur (SetupDiCreateDeviceInfo, DIF_REGISTERDEVICE).
step "Appareils racine : GUID de classe au registre (#1605)" cargo test -q -p winlux -- devnodes::tests::guid_de_classe_aller_retour

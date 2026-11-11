# #1625 : outils d'installation de pilotes (lecture des .inf, modèles NTamd64, DriverVer).
step "Outils de pilotes : .inf, modèles et DriverVer (#1625)" cargo test -q -p winlux -- setupinf::

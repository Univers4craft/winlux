# #2234 : désinstaller un composant de Winlux (espeak-ng) est refusé avant toute
# autorisation, avec un message clair en quatre langues au lieu de « Échec (100) ».
step "Boutique : paquet nécessaire au système expliqué (#2234)" python3 -B scripts/test-store-paquet-systeme.py

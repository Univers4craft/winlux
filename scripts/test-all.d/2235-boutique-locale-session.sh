# #2235 : sous LC_ALL français (comptes créés après l'installation), la Boutique
# garde le catalogue Ubuntu ; apt écrivait « Candidat : » malgré LANG=C.
step "Boutique : catalogue Ubuntu sous LC_ALL français (#2235)" python3 -B scripts/test-store-locale-session.py

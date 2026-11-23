# #3126 : le relevé d'état ne tourne plus toutes les 3 s pour rien ; il est
# fait à l'ouverture de session, puis à la demande d'une fenêtre qui le lit.
step "Relevé d'état à la demande, bureau immobile sans programme lancé (#3126)" python3 -B scripts/test-releve-a-la-demande.py

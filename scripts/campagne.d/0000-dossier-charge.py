# Contrôle témoin : test-campaign.py charge bien ce dossier, avec ses noms.
# Modèle pour un nouveau contrôle : copier ce fichier sous « NNNN-sujet.py »
# (NNNN = numéro de l'issue), et n'écrire que des appels ok(condition, libellé).
ok(callable(ok) and (SCRIPTS / "test-campaign.py").is_file() and runtime_dlls.is_dir(),
   "scripts/campagne.d : les contrôles déposés voient ok, SCRIPTS et runtime_dlls")

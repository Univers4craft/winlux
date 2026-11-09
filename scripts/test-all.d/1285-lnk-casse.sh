# Raccourci du menu Démarrer vers …\audacity.exe alors que le fichier est
# Audacity.exe (#1285) : la cible est retrouvée sans tenir compte de la casse.
step "raccourci .lnk insensible à la casse (#1285)" python3 -I scripts/test-lnk-casse.py

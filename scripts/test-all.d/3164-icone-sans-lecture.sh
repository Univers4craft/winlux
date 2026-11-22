# #3164 : l'icône d'un gros .exe s'extrait sans lire tout le fichier.
step "Icône d'un .exe : en-têtes et ressources seulement, sans pic de mémoire (#3164)" /usr/bin/python3 -B scripts/test-icone-sans-lecture.py

# Gain #3316 : contrôle parental au repos, service sans tour chaque seconde
# quand aucun compte n'est surveillé, supervision sans tout le programme.
step "Contrôle parental au repos : service endormi, supervision allégée (Gain #3316)" python3 -B scripts/test-parental-repos.py

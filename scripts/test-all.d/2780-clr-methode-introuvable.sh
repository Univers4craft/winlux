# #2780 : une méthode .NET que le moteur ne sert pas est journalisée avec son
# appelant et le type réel de `this`, même rattrapée par l'appli.
if [ "$quick" = 0 ]; then
    step "CLR : méthode introuvable journalisée avec l'appelant et le type de this (#2780)" sh scripts/test-clr-methode-introuvable.sh
else
    skip "CLR : méthode introuvable journalisée avec l'appelant et le type de this (#2780)" "mode rapide"
fi

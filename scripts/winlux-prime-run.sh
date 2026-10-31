#!/bin/sh
# Lance une commande sur le GPU dédié (ou intégré avec --integrated) d'une
# machine hybride. Sur une machine à un seul GPU, la commande est lancée
# telle quelle : rien à choisir, donc rien à casser.
#   winlux-prime-run [--integrated|--dedicated] commande [arguments...]
# Pour un .exe, préférez Paramètres > Exécutable > GPU : le choix est
# mémorisé et appliqué à chaque lancement.
set -eu
preference=dedicated
case "${1:-}" in
    --integrated) preference=integrated; shift ;;
    --dedicated) preference=dedicated; shift ;;
    --help|-h|'') echo 'usage : winlux-prime-run [--integrated|--dedicated] commande [arguments...]' >&2; exit 2 ;;
esac
runtime=${WINLUX_BIN:-winlux}
if command -v "$runtime" >/dev/null 2>&1; then
    # Le runtime ne produit que des lignes CLE=valeur sûres (lettres,
    # chiffres, _ - .), validées ici avant export.
    env_lines=$("$runtime" --gpu-env "$preference" 2>/dev/null || true)
    old_ifs=$IFS
    IFS='
'
    for line in $env_lines; do
        case "$line" in
            *[!A-Za-z0-9_=.-]*|=*|'') continue ;;
            [A-Z_]*=*) export "$line" ;;
        esac
    done
    IFS=$old_ifs
fi
exec "$@"

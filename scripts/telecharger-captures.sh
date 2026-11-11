#!/bin/sh
# Rapatrie les captures d'écran prises par la CI (#1779).
#
#   sh scripts/telecharger-captures.sh [NOMBRE] [DOSSIER] [BRANCHE]
#
# Télécharge, pour les NOMBRE dernières exécutions réussies du workflow CI sur
# BRANCHE (10 et master par défaut ; « toutes » pour toutes les branches et les
# PR), les artefacts captures-ui et captures-ui-diff dans
# DOSSIER/AAAA-MM-JJ-<commit court>-<exécution>/ (~/winlux-captures-ci par
# défaut), puis écrit un index.html dans chacun (les exécutions antérieures à
# #1779 n'en ont pas) et un index.html général dans DOSSIER.
#
# Il faut gh connecté (« gh auth login ») : GitHub exige un compte pour
# télécharger un artefact, même d'un dépôt public. Une exécution déjà
# rapatriée n'est pas retéléchargée ; un artefact expiré est sauté.
set -eu
nombre=${1:-10}
dossier=${2:-$HOME/winlux-captures-ci}
branche=${3:-master}
depot=${WINLUX_DEPOT:-Univers4craft/winlux}
ici=$(cd "$(dirname "$0")" && pwd)
command -v gh >/dev/null || { echo "gh absent : https://cli.github.com" >&2; exit 2; }
mkdir -p "$dossier"
filtre="--branch $branche"
test "$branche" = toutes && filtre=""
# shellcheck disable=SC2086
gh run list -R "$depot" --workflow ci.yml $filtre --status success --limit "$nombre" \
    --json databaseId,headSha,createdAt,headBranch \
    -q '.[] | [.databaseId, .headSha[0:7], .createdAt[0:10], .headBranch] | @tsv' |
while IFS="$(printf '\t')" read -r run sha date br; do
    cible="$dossier/$date-$sha-$run"
    if test -d "$cible/captures-ui"; then
        echo "déjà là : $cible"
        continue
    fi
    mkdir -p "$cible"
    if gh run download "$run" -R "$depot" -n captures-ui -D "$cible/captures-ui" 2>/dev/null; then
        gh run download "$run" -R "$depot" -n captures-ui-diff -D "$cible/captures-ui-diff" 2>/dev/null || true
        rapport=""
        test -f "$cible/captures-ui-diff/rapport.json" && rapport="--rapport $cible/captures-ui-diff/rapport.json"
        # shellcheck disable=SC2086
        GITHUB_SHA=$sha GITHUB_REF_NAME=$br GITHUB_RUN_ID=$run GITHUB_REPOSITORY=$depot \
            python3 -I "$ici/index-captures.py" "$cible/captures-ui" $rapport \
            --titre "Captures $sha ($br)" >/dev/null
        if test -d "$cible/captures-ui-diff"; then
            GITHUB_SHA=$sha GITHUB_REF_NAME=$br GITHUB_RUN_ID=$run GITHUB_REPOSITORY=$depot \
                python3 -I "$ici/index-captures.py" "$cible/captures-ui-diff" $rapport \
                --titre "Pixels changés $sha ($br)" >/dev/null
        fi
        echo "ok : $cible"
    else
        rmdir "$cible" 2>/dev/null || true
        echo "sans captures (expirées ou absentes) : exécution $run du $date"
    fi
done
# Index général : une ligne par exécution rapatriée, la plus récente en haut.
{
    printf '<!doctype html><html lang="fr"><head><meta charset="utf-8"><title>Captures de la CI Winlux</title>'
    printf '<style>body{font:15px/1.5 system-ui,sans-serif;padding:16px;background:#f6f5fa;color:#1c1b22}a{color:#6A3DF0}'
    printf '@media (prefers-color-scheme:dark){body{background:#16151b;color:#ecebf2}a{color:#B69CFF}}</style></head><body>'
    printf '<h1>Captures de la CI Winlux</h1><ul>'
    for d in $(ls -1r "$dossier"); do
        test -f "$dossier/$d/captures-ui/index.html" || continue
        printf '<li><a href="%s/captures-ui/index.html">%s</a>' "$d" "$d"
        test -f "$dossier/$d/captures-ui-diff/index.html" &&
            printf ' · <a href="%s/captures-ui-diff/index.html">pixels changés</a>' "$d"
        printf '</li>'
    done
    printf '</ul></body></html>\n'
} >"$dossier/index.html"
echo "Ouvrir : $dossier/index.html"

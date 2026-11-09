#!/bin/bash
# Compile la BCL Winlux écrite en C# (bcl/mscorlib) et ses témoins (étape 7
# du moteur .NET Framework).
#
#   scripts/build-bcl.sh [--temoins]
#
# Compilateur : Roslyn (Microsoft.Net.Compilers.Toolset, licence MIT), à
# version épinglée et empreinte SHA-256 vérifiée, utilisé à la construction
# seulement. Il tourne sur le runtime .NET libre d'Ubuntu (paquet
# dotnet-runtime-10.0, compilé par Canonical). Rien de cela n'entre dans
# l'ISO : seule la mscorlib.dll produite y est copiée (/opt/winlux/bcl).
#
# Sortie : build/bcl/mscorlib.dll ; avec --temoins, build/bcl-temoins/*.exe.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
roslyn_version=5.0.0
roslyn_sha256=e3f0207e7a8caf2cf79ad8f276eb20b2b880b3a7bb0e8ad27b4d3183d0a81627
roslyn_url="https://api.nuget.org/v3-flatcontainer/microsoft.net.compilers.toolset/$roslyn_version/microsoft.net.compilers.toolset.$roslyn_version.nupkg"

if ! command -v dotnet >/dev/null; then
    echo "build-bcl : « dotnet » absent ; installer le paquet Ubuntu dotnet-runtime-10.0" >&2
    exit 2
fi

# Paquet téléchargé = donnée non fiable : dossier à lui, empreinte vérifiée
# avant toute extraction.
cache="$root/build/roslyn/$roslyn_version"
csc="$cache/tasks/netcore/bincore/csc.dll"
if [ ! -f "$csc" ]; then
    rm -rf "$cache"
    mkdir -p "$cache"
    nupkg="$root/build/roslyn/roslyn-$roslyn_version.nupkg"
    if [ ! -f "$nupkg" ] || ! echo "$roslyn_sha256  $nupkg" | sha256sum --check --status; then
        curl --fail --silent --show-error --location --max-time 300 -o "$nupkg.partiel" "$roslyn_url"
        mv "$nupkg.partiel" "$nupkg"
    fi
    if ! echo "$roslyn_sha256  $nupkg" | sha256sum --check --status; then
        echo "build-bcl : empreinte SHA-256 de Roslyn $roslyn_version inattendue, paquet refusé" >&2
        rm -f "$nupkg"
        exit 1
    fi
    unzip -q -o "$nupkg" 'tasks/netcore/bincore/*' -d "$cache"
fi

compile() {
    DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1 \
        dotnet "$csc" -nologo -noconfig -nostdlib -deterministic -warnaserror+ "$@"
}

mkdir -p "$root/build/bcl"
compile -unsafe -runtimemetadataversion:v4.0.30319 -nowarn:0169 -target:library \
    -out:"$root/build/bcl/mscorlib.dll" "$root"/bcl/mscorlib/*.cs

if [ "${1:-}" = "--temoins" ]; then
    mkdir -p "$root/build/bcl-temoins"
    for src in "$root"/bcl/temoins/*.cs; do
        compile -r:"$root/build/bcl/mscorlib.dll" \
            -out:"$root/build/bcl-temoins/$(basename "$src" .cs).exe" "$src"
    done
fi

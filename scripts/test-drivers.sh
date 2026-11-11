#!/bin/sh
# Installation virtualisée d'un pilote constructeur (#1532) : driver_msvc.exe
# installe un pilote d'essai (.inf écrit ici, .sys jamais fourni ni chargé),
# démarre son service, ouvre l'appareil virtuel qui le remplace, puis vérifie
# qu'un pilote sans traduction reste arrêté (ERROR_DRIVER_BLOCKED) et que la
# personne en est prévenue.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/driver_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/driver_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/c" "$work/run"
cat >"$work/essai.inf" <<'INF'
; Pilote d'essai de Winlux (#1532) : aucun fichier .sys n'existe.
[Version]
Signature = "$WINDOWS NT$"
Class     = HIDClass
ClassGUID = {745a17a0-74d3-11d0-b6fe-00a0c90f57da}
Provider  = %Mfg%
DriverVer = 07/14/2017,3.3.0.0

[Manufacturer]
%Mfg% = Modeles, NTamd64

[Modeles.NTamd64]
%Clavier% = Essai, ROOT\WinluxEssai

[Essai.NTamd64.Services]
AddService = WinluxEssai, 0x00000002, Essai_Service

[Essai_Service]
DisplayName   = %SvcDesc%
ServiceType   = 1
StartType     = 3
ErrorControl  = 1
ServiceBinary = %12%\winluxessai.sys

[Strings]
SvcDesc = "Pilote d'essai de Winlux"
Mfg     = "Winlux Essais"
Clavier = "Clavier d'essai"
INF

cd "$work"
set +e
env WINLUX_GUI=headless WINLUX_VDEV_ESSAI=1 WINLUX_LANG=fr_FR.UTF-8 \
    WINLUX_REGISTRY_PATH="$work/registre.tsv" WINLUX_DRIVE_C="$work/c" HOME="$work" TMPDIR="$work/run" \
    timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
status=$?
set -e
test "$status" -eq 0 || {
    tail -30 "$work/out.txt"
    echo "driver_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
grep -q "Le pilote Windows de Winlux Essais a été remplacé par Winlux" "$work/out.txt" || {
    echo "notification « pilote remplacé » absente" >&2
    exit 1
}
grep -q "Le pilote Windows inconnu.sys ne peut pas encore être remplacé" "$work/out.txt" || {
    echo "notification « pilote non remplacé » absente" >&2
    exit 1
}
if find "$work" -iname '*.sys' | grep -q .; then
    echo "un fichier .sys a été créé" >&2
    exit 1
fi
echo "driver_msvc : .inf dans le magasin, service pilote remplacé par un appareil virtuel, pilote inconnu bloqué et signalé, aucun .sys chargé"

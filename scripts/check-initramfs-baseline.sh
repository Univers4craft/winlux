#!/bin/sh
# Garde-fou #2146 : le petit initramfs doit tourner sur tout processeur x86-64,
# même sans AVX (Atom, Celeron/Pentium Silver, Core 1re génération).
#
#   sh scripts/check-initramfs-baseline.sh DOSSIER_INITRAMFS
#
# Chaque exécutable ELF du dossier est lancé sous qemu-x86_64 avec un processeur
# Nehalem (SSE4.2, sans AVX), avec les bibliothèques du dossier lui-même
# (-L). Une instruction illégale (signal 4) fait échouer la construction : le
# binaire a été compilé pour x86-64-v3 (variante amd64v3 d'Ubuntu, par exemple).
# WINLUX_SKIP_BASELINE_CHECK=1 saute la vérification (machine sans qemu-user).
set -eu
dir=${1:?dossier de l initramfs requis}
test -d "$dir" || { echo "dossier absent : $dir" >&2; exit 1; }
if ! command -v qemu-x86_64 >/dev/null 2>&1; then
    if test "${WINLUX_SKIP_BASELINE_CHECK:-0}" = 1; then
        echo 'qemu-x86_64 absent : vérification sans AVX sautée (WINLUX_SKIP_BASELINE_CHECK=1)' >&2
        exit 0
    fi
    echo 'qemu-x86_64 requis pour vérifier que l initramfs démarre sans AVX (paquet qemu-user)' >&2
    exit 1
fi
cpu=${WINLUX_BASELINE_CPU:-Nehalem}
failed=0
checked=0
run() {
    # $1 : binaire relatif au dossier, puis ses arguments.
    binary=$1; shift
    set +e
    output=$(qemu-x86_64 -cpu "$cpu" -L "$dir" "$dir/$binary" "$@" 2>&1 </dev/null)
    status=$?
    set -e
    checked=$((checked + 1))
    case "$status:$output" in
        132:*|*'signal 4'*|*'Illegal instruction'*)
            echo "initramfs : $binary $* plante sans AVX (processeur $cpu, code $status)" >&2
            failed=1 ;;
    esac
}
tab=$(printf '\t')
find "$dir" -type f ! -name '*.ko' | LC_ALL=C sort | while IFS= read -r path; do
    magic=$(od -An -c -N4 "$path" | tr -d " $tab")
    test "$magic" = '177ELF' || continue
    # Exécutables seulement : les bibliothèques sont chargées par eux.
    case "$path" in *.so|*.so.*) continue ;; esac
    test -x "$path" || continue
    printf '%s\n' "${path#"$dir"/}"
done >"${TMPDIR:-/tmp}/winlux-initramfs-elf.$$"
while IFS= read -r binary; do
    case "$binary" in
        bin/busybox|*/busybox)
            # Applettes de /init : la variante amd64v3 plantait dès leur appel.
            run "$binary" sh -c 'echo ok'
            for applet in mount mdev insmod switch_root findfs; do run "$binary" "$applet" --help; done ;;
        *) run "$binary" --version ;;
    esac
done <"${TMPDIR:-/tmp}/winlux-initramfs-elf.$$"
rm -f "${TMPDIR:-/tmp}/winlux-initramfs-elf.$$"
test "$checked" -gt 0 || { echo "initramfs : aucun exécutable ELF trouvé dans $dir" >&2; exit 1; }
test "$failed" = 0 || exit 1
echo "initramfs : $checked appels sans AVX ($cpu) réussis"

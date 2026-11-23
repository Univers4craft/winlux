#!/bin/sh
# Diagnostic de #3199 (à retirer d'un bloc avec son workflow) : la panique
# uefi32 du mode mixte EFI disparaît-elle avec nokaslr ?
#
#   sh scripts/diag-3199-nokaslr.sh ISO NOMBRE
#
# Fait une copie de l'ISO dont seul /boot/grub/grub.cfg change (nokaslr
# ajouté après $winlux_efi_args, réécrit sur place à taille égale), puis démarre en alternance l'ISO d'origine
# et la copie, NOMBRE fois chacune, en UEFI 32 bits sous QEMU, avec les
# options de scripts/test-live-iso-qemu.sh. Chaque démarrage s'arrête dès que
# le noyau a paniqué (« NX-protected ») ou atteint l'espace utilisateur.
#
# Lecture fixée d'avance (#3199) : si la ligne actuelle panique dans ce lot
# et que nokaslr ne panique jamais en NOMBRE démarrages, le placement physique
# du noyau (KASLR) est en cause ; une seule panique sous nokaslr écarte ce
# contournement.
set -eu
iso=$1
count=${2:-20}
work=${WINLUX_DIAG_DIR:-build/diag-3199}
mkdir -p "$work"
ovmf=${WINLUX_OVMF32:?WINLUX_OVMF32 requis}
ovmf_vars=${WINLUX_OVMF32_VARS:?WINLUX_OVMF32_VARS requis}

osirrox -indev "$iso" -extract /boot/grub/grub.cfg "$work/grub.cfg" 2>/dev/null
chmod u+w "$work/grub.cfg"
# xorriso ne sait pas réécrire cette image (GPT recouvrant) : la copie est
# modifiée sur place, dans l'étendue de grub.cfg, à taille égale. nokaslr
# est ajouté après $winlux_efi_args et l'indentation des lignes compense.
lba=$(xorriso -indev "$iso" -find /boot/grub/grub.cfg -exec report_lba 2>/dev/null |
    awk '/File data lba/ {print $6; exit}')
test -n "$lba" || { echo "grub.cfg : étendue introuvable" >&2; exit 1; }
python3 -I - "$work/grub.cfg" "$work/grub-nokaslr.cfg" <<'PY'
import sys
old = open(sys.argv[1], "rb").read()
new = old.replace(b"$winlux_efi_args", b"$winlux_efi_args nokaslr")
assert new != old, "$winlux_efi_args introuvable"
lines = new.split(b"\n")
excess = len(new) - len(old)
for i, line in enumerate(lines):
    while excess > 0 and line.startswith(b" "):
        line = line[1:]
        excess -= 1
    lines[i] = line
assert excess == 0, "indentation insuffisante pour compenser"
new = b"\n".join(lines)
assert len(new) == len(old)
open(sys.argv[2], "wb").write(new)
PY
cp "$iso" "$work/nokaslr.iso"
dd if="$work/grub-nokaslr.cfg" of="$work/nokaslr.iso" bs=2048 seek="$lba" conv=notrunc status=none
osirrox -indev "$work/nokaslr.iso" -extract /boot/grub/grub.cfg "$work/grub-relu.cfg" 2>/dev/null
cmp "$work/grub-nokaslr.cfg" "$work/grub-relu.cfg"
# Seul grub.cfg diffère entre les deux images.
diff "$work/grub.cfg" "$work/grub-nokaslr.cfg" || true

echo "QEMU : $(qemu-system-x86_64 --version | head -n 1) ; -cpu max ; -accel tcg"

boot() { # $1 image, $2 journal ; écrit panique, ok ou indetermine
    cp "$ovmf_vars" "$2.vars"
    (
        for attempt in $(seq 1 400); do printf d || exit 0; sleep 0.1; done
    ) | qemu-system-x86_64 -m 2048 -smp 2 -cpu max -accel tcg -display none -nic none \
        -monitor none -serial stdio -no-reboot -cdrom "$1" -machine q35 \
        -drive "if=pflash,format=raw,unit=0,readonly=on,file=$ovmf" \
        -drive "if=pflash,format=raw,unit=1,file=$2.vars" >"$2" 2>&1 &
    pid=$!
    for s in $(seq 1 300); do
        sleep 1
        grep -aqE 'NX-protected|Kernel panic|Run /init|Freeing unused kernel' "$2" && break
        kill -0 "$pid" 2>/dev/null || break
    done
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    rm -f "$2.vars"
    if grep -aq 'NX-protected' "$2"; then
        echo panique
    elif grep -aqE 'Run /init|Freeing unused kernel' "$2"; then
        echo ok
    else
        echo indetermine
    fi
}

pa=0; oa=0; ia=0; pb=0; ob=0; ib=0
for i in $(seq 1 "$count"); do
    a=$(boot "$iso" "$work/actuelle-$i.log")
    b=$(boot "$work/nokaslr.iso" "$work/nokaslr-$i.log")
    echo "démarrage $i : actuelle $a ; nokaslr $b"
    case $a in panique) pa=$((pa + 1)) ;; ok) oa=$((oa + 1)) ;; *) ia=$((ia + 1)) ;; esac
    case $b in panique) pb=$((pb + 1)) ;; ok) ob=$((ob + 1)) ;; *) ib=$((ib + 1)) ;; esac
done
# Preuve que chaque image démarre avec sa ligne.
grep -ah 'Command line' "$work/actuelle-1.log" "$work/nokaslr-1.log" || true
# Carte mémoire EFI (efi=debug) d'une panique et d'un démarrage réussi de la
# ligne actuelle : la région qui couvre le bloc de 2 Mo du trampoline.
first_panic=$(grep -al 'NX-protected' "$work"/actuelle-*.log | head -n 1 || true)
first_ok=$(grep -aLE 'NX-protected' "$work"/actuelle-*.log | head -n 1 || true)
for f in $first_panic $first_ok; do
    echo "== carte EFI de $f"
    grep -aE 'efi: mem[0-9]+:|RIP:|PMD|Kernel command line|Command line' "$f" || true
done
for f in "$work"/*.log; do
    grep -aq 'NX-protected' "$f" && { echo "== $f"; grep -aE 'Command line|RIP:|PMD|RDI' "$f" | head -8; }
done
summary="| ligne | paniques | atteint /init | indéterminés |
|---|---|---|---|
| actuelle | $pa | $oa | $ia |
| nokaslr | $pb | $ob | $ib |"
echo "$summary"
test -n "${GITHUB_STEP_SUMMARY:-}" && echo "$summary" >>"$GITHUB_STEP_SUMMARY"
exit 0

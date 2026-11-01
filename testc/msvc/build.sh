#!/bin/sh
# Construit eh_msvc.exe avec clang (cible MSVC) et lld-link, sans CRT :
# les bibliothèques d'importation viennent de llvm-dlltool.
set -eu
src=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
out=$1
mkdir -p "$out/msvc"
tmp="$out/msvc"
for d in clang lld-link llvm-dlltool; do
    command -v "$d" >/dev/null 2>&1 || { echo "msvc : $d absent, test ignoré" >&2; exit 0; }
done
printf 'LIBRARY vcruntime140.dll\nEXPORTS\n_CxxThrowException\n__CxxFrameHandler3\n__C_specific_handler\n__std_terminate\n' >"$tmp/vcruntime140.def"
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nRaiseException\n' >"$tmp/kernel32.def"
llvm-dlltool -m i386:x86-64 -d "$tmp/vcruntime140.def" -l "$tmp/vcruntime140.lib"
llvm-dlltool -m i386:x86-64 -d "$tmp/kernel32.def" -l "$tmp/kernel32.lib"
# Table virtuelle de type_info : seul son adresse compte ici.
printf '  .data\n  .globl "??_7type_info@@6B@"\n"??_7type_info@@6B@":\n  .quad 0\n' >"$tmp/typeinfo.s"
clang --target=x86_64-pc-windows-msvc -c "$tmp/typeinfo.s" -o "$tmp/typeinfo.obj"
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -fexceptions -fcxx-exceptions \
    -c "$src/eh_msvc.cpp" -o "$tmp/eh_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/eh_msvc.obj" "$tmp/typeinfo.obj" \
    "$tmp/vcruntime140.lib" "$tmp/kernel32.lib" /out:"$out/eh_msvc.exe"

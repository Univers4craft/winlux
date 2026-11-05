#!/bin/sh
# Construit les programmes Windows 32 bits (x86) de test de wow32 (#232)
# avec clang (cible i686-pc-windows-msvc), lld-link et llvm-dlltool, sans CRT.
set -eu
src=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
out=$1
for d in clang lld-link llvm-dlltool; do
    command -v "$d" >/dev/null 2>&1 || { echo "wow32 : $d absent, test ignoré" >&2; exit 0; }
done
tmp="$out/wow32"
mkdir -p "$tmp"
# Noms décorés stdcall (@n) dans la bibliothèque, importés sans décoration (-k).
cat >"$tmp/kernel32.def" <<'DEF'
LIBRARY kernel32.dll
EXPORTS
GetStdHandle@4
WriteFile@20
ExitProcess@4
GetCommandLineA@0
lstrlenA@4
GetTickCount@0
GetTickCount64@0
VerSetConditionMask@16
SetLastError@4
GetLastError@0
GetProcessHeap@0
HeapAlloc@12
HeapFree@12
VirtualAlloc@16
GetModuleHandleA@4
QueryPerformanceFrequency@4
GetCurrentProcessId@0
GetCurrentProcess@0
GetEnvironmentVariableA@12
DEF
llvm-dlltool -m i386 -k -d "$tmp/kernel32.def" -l "$tmp/kernel32.lib"
clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/hello32.c" -o "$tmp/hello32.obj"
lld-link /nologo /machine:x86 /entry:entry /subsystem:console /nodefaultlib /safeseh:no \
    "$tmp/hello32.obj" "$tmp/kernel32.lib" /out:"$out/hello32.exe"
# Adresse préférée au-delà de 2 Go, interdite sans LARGEADDRESSAWARE :
# le programme est déplacé (relocations HIGHLOW).
lld-link /nologo /machine:x86 /entry:entry /subsystem:console /nodefaultlib /safeseh:no \
    /base:0x90000000 "$tmp/hello32.obj" "$tmp/kernel32.lib" /out:"$out/hello32_reloc.exe"

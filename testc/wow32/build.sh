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
# Exceptions (#232, étape 2) : __try/__except/__finally de clang, gérés par
# msvcrt!_except_handler3 ; RaiseException et SetUnhandledExceptionFilter.
cat >"$tmp/kernel32_seh.def" <<'DEF'
LIBRARY kernel32.dll
EXPORTS
RaiseException@16
ExitProcess@4
GetStdHandle@4
WriteFile@20
GetCommandLineA@0
SetUnhandledExceptionFilter@4
DEF
printf 'LIBRARY msvcrt.dll\nEXPORTS\n_except_handler3\n' >"$tmp/msvcrt_seh.def"
llvm-dlltool -m i386 -k -d "$tmp/kernel32_seh.def" -l "$tmp/kernel32_seh.lib"
llvm-dlltool -m i386 -d "$tmp/msvcrt_seh.def" -l "$tmp/msvcrt_seh.lib"
clang --target=i686-pc-windows-msvc -O1 -fms-extensions -fasync-exceptions -fno-builtin -fno-stack-protector \
    -c "$src/seh32.c" -o "$tmp/seh32.obj"
lld-link /nologo /machine:x86 /entry:entry /subsystem:console /nodefaultlib /safeseh:no \
    "$tmp/seh32.obj" "$tmp/kernel32_seh.lib" "$tmp/msvcrt_seh.lib" /out:"$out/seh32.exe"
# Personnalité WOW64 (#232, étape 7) : kernel32 et advapi32 sans CRT.
cat >"$tmp/kernel32_wow64.def" <<'DEF'
LIBRARY kernel32.dll
EXPORTS
GetStdHandle@4
WriteFile@20
ExitProcess@4
GetCommandLineA@0
lstrlenA@4
GetCurrentProcess@0
IsWow64Process@8
IsWow64Process2@12
GetSystemInfo@4
GetNativeSystemInfo@4
VirtualQuery@12
GetStartupInfoA@4
GlobalMemoryStatus@4
InitializeCriticalSection@4
InitializeCriticalSectionAndSpinCount@8
EnterCriticalSection@4
LeaveCriticalSection@4
DeleteCriticalSection@4
InitializeSRWLock@4
AcquireSRWLockExclusive@4
ReleaseSRWLockExclusive@4
InitOnceInitialize@4
InitOnceBeginInitialize@16
InitOnceComplete@12
InitializeSListHead@4
InterlockedPushEntrySList@8
InterlockedPopEntrySList@4
QueryDepthSList@4
CreatePipe@16
CloseHandle@4
GetEnvironmentVariableA@12
GetSystemWow64DirectoryA@8
Wow64DisableWow64FsRedirection@4
Wow64RevertWow64FsRedirection@4
CreateFileA@28
CreateProcessA@40
WaitForSingleObject@8
GetExitCodeProcess@8
GetModuleFileNameA@12
DEF
cat >"$tmp/advapi32_wow64.def" <<'DEF'
LIBRARY advapi32.dll
EXPORTS
RegCreateKeyExA@36
RegOpenKeyExA@20
RegSetValueExA@24
RegCloseKey@4
RegDeleteKeyA@8
DEF
llvm-dlltool -m i386 -k -d "$tmp/kernel32_wow64.def" -l "$tmp/kernel32_wow64.lib"
llvm-dlltool -m i386 -k -d "$tmp/advapi32_wow64.def" -l "$tmp/advapi32_wow64.lib"
clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/wow64.c" -o "$tmp/wow64.obj"
lld-link /nologo /machine:x86 /entry:entry /subsystem:console /nodefaultlib /safeseh:no \
    "$tmp/wow64.obj" "$tmp/kernel32_wow64.lib" "$tmp/advapi32_wow64.lib" /out:"$out/wow64.exe"
# CRT 32 bits (#232, étape 3) : vrai programme MinGW i686 lié à msvcrt.dll,
# avec le printf de MinGW puis avec celui de msvcrt. Facultatif : sans
# i686-w64-mingw32-gcc, le test le saute.
if command -v i686-w64-mingw32-gcc >/dev/null 2>&1; then
    i686-w64-mingw32-gcc -O1 "$src/crt32.c" -o "$out/crt32.exe"
    i686-w64-mingw32-gcc -O1 -D__USE_MINGW_ANSI_STDIO=0 "$src/crt32.c" -o "$out/crt32_msvcrt.exe"
else
    echo "wow32 : i686-w64-mingw32-gcc absent, crt32.exe non construit" >&2
fi
# Threads, TLS, DLL x86, LoadLibrary (#232, étape 4) : clang en cible
# i686-w64-mingw32 (TLS statique natif) avec les bibliothèques de MinGW,
# et g++ pour le C++.
if command -v i686-w64-mingw32-gcc >/dev/null 2>&1; then
    i686-w64-mingw32-gcc -O1 "$src/thread32.c" -o "$out/thread32_gcc.exe"
    mingw_clang="clang --target=i686-w64-mingw32 -O1 --sysroot=/usr -fuse-ld=lld"
    $mingw_clang "$src/thread32.c" -o "$out/thread32.exe"
    # Bibliothèque d'import à part : $tmp contient aussi kernel32.lib (sans
    # CRT), que l'éditeur de liens prendrait avant celle de MinGW.
    mkdir -p "$tmp/dll32"
    $mingw_clang -shared "$src/dll32.c" -o "$out/dll32.dll" -Wl,--out-implib,"$tmp/dll32/libdll32.a"
    cp "$out/dll32.dll" "$out/dll32b.dll"
    $mingw_clang "$src/load32.c" -o "$out/load32.exe" -L"$tmp/dll32" -ldll32
fi
if command -v i686-w64-mingw32-g++ >/dev/null 2>&1; then
    i686-w64-mingw32-g++ -O1 -static "$src/cpp32.cpp" -o "$out/cpp32.exe"
fi

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
GetCommandLineW@0
lstrlenW@4
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
GetPhysicallyInstalledSystemMemory@4
LoadLibraryA@4
GetProcAddress@8
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
RtlUnwind@16
EnumCalendarInfoW@16
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
GetCurrentProcessId@0
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
InterlockedIncrement@4
InterlockedDecrement@4
InterlockedExchange@8
InterlockedCompareExchange@12
GlobalAddAtomA@4
GlobalFindAtomA@4
GlobalDeleteAtom@4
SetHandleCount@4
LoadLibraryA@4
GetProcAddress@8
GetLogicalProcessorInformation@8
EnumCalendarInfoW@16
K32EnumProcessModules@16
GetModuleHandleA@4
FindResourceW@12
LoadResource@8
SizeofResource@8
LockResource@4
CreateFileW@28
DeleteFileW@4
SetLastError@4
GetLastError@0
DEF
printf 'LIBRARY msvcrt.dll\nEXPORTS\n_adjust_fdiv DATA\n_controlfp\n_ftol\n' >"$tmp/msvcrt_wow64.def"
llvm-dlltool -m i386 -d "$tmp/msvcrt_wow64.def" -l "$tmp/msvcrt_wow64.lib"
cat >"$tmp/advapi32_wow64.def" <<'DEF'
LIBRARY advapi32.dll
EXPORTS
RegCreateKeyExA@36
RegOpenKeyExA@20
RegSetValueExA@24
RegCloseKey@4
RegDeleteKeyA@8
RegSetKeySecurity@12
DEF
llvm-dlltool -m i386 -k -d "$tmp/kernel32_wow64.def" -l "$tmp/kernel32_wow64.lib"
llvm-dlltool -m i386 -k -d "$tmp/advapi32_wow64.def" -l "$tmp/advapi32_wow64.lib"
printf 'LIBRARY oleaut32.dll\nEXPORTS\nSysAllocString@4\nSysStringLen@4\nVariantCopy@8\nVariantClear@4\n' >"$tmp/oleaut32_wow64.def"
llvm-dlltool -m i386 -k -d "$tmp/oleaut32_wow64.def" -l "$tmp/oleaut32_wow64.lib"
printf 'LIBRARY ws2_32.dll\nEXPORTS\nWSAStartup@8\ngethostbyname@4\ngetaddrinfo@16\nfreeaddrinfo@4\ngetservbyname@8\nsocket@12\nclosesocket@4\nselect@20\nWSAPoll@12\n' >"$tmp/ws2_32_wow64.def"
printf 'LIBRARY winhttp.dll\nEXPORTS\nWinHttpGetIEProxyConfigForCurrentUser@4\nWinHttpGetDefaultProxyConfiguration@4\n' >"$tmp/winhttp_wow64.def"
llvm-dlltool -m i386 -k -d "$tmp/winhttp_wow64.def" -l "$tmp/winhttp_wow64.lib"
llvm-dlltool -m i386 -k -d "$tmp/ws2_32_wow64.def" -l "$tmp/ws2_32_wow64.lib"
printf 'LIBRARY user32.dll\nEXPORTS\nLoadIconW@8\nGetIconInfo@8\nCreatePopupMenu@0\nInsertMenuItemW@16\nGetMenuItemInfoW@16\n' >"$tmp/user32_wow64.def"
printf 'LIBRARY gdi32.dll\nEXPORTS\nCreateCompatibleDC@4\nCreateCompatibleBitmap@12\nSelectObject@8\n' >"$tmp/gdi32_wow64.def"
llvm-dlltool -m i386 -k -d "$tmp/gdi32_wow64.def" -l "$tmp/gdi32_wow64.lib"
llvm-dlltool -m i386 -k -d "$tmp/user32_wow64.def" -l "$tmp/user32_wow64.lib"
clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/wow64.c" -o "$tmp/wow64.obj"
# Ressource RT_RCDATA nommée « WINLUX » (« TPF0winlux ») écrite à la main
# au format .res, sans llvm-rc : vérification 78.
python3 -B - "$tmp/wow64.res" <<'PY'
import struct, sys
def entry(typ, name, data):
    hdr = struct.pack("<HH", 0xFFFF, typ) + "".join(name).encode("utf-16-le") + b"\0\0"
    hdr += b"\0" * (-len(hdr) % 4)
    hdr += struct.pack("<IHHII", 0, 0x30, 0x0409, 0, 0)
    out = struct.pack("<II", len(data), 8 + len(hdr)) + hdr + data
    return out + b"\0" * (-len(out) % 4)
empty = struct.pack("<IIHHHHIHHII", 0, 32, 0xFFFF, 0, 0xFFFF, 0, 0, 0, 0, 0, 0)
open(sys.argv[1], "wb").write(empty + entry(10, "WINLUX", b"TPF0winlux"))
PY
lld-link /nologo /machine:x86 /entry:entry /subsystem:console /nodefaultlib /safeseh:no \
    "$tmp/wow64.obj" "$tmp/kernel32_wow64.lib" "$tmp/advapi32_wow64.lib" "$tmp/msvcrt_wow64.lib" \
    "$tmp/oleaut32_wow64.lib" "$tmp/ws2_32_wow64.lib" "$tmp/user32_wow64.lib" \
    "$tmp/gdi32_wow64.lib" "$tmp/winhttp_wow64.lib" "$tmp/wow64.res" /out:"$out/wow64.exe"
# Exceptions C++ MSVC x86 (#232, étape 7) : tables de clang en cible
# i686-pc-windows-msvc, msvcrt!__CxxFrameHandler3 et _CxxThrowException.
cat >"$tmp/kernel32_cxx.def" <<'DEF'
LIBRARY kernel32.dll
EXPORTS
GetStdHandle@4
WriteFile@20
ExitProcess@4
lstrlenA@4
RaiseException@16
DEF
printf 'LIBRARY msvcrt.dll\nEXPORTS\n__CxxFrameHandler3\n_CxxThrowException@8\n_except_handler3\n' >"$tmp/msvcrt_cxx.def"
llvm-dlltool -m i386 -k -d "$tmp/kernel32_cxx.def" -l "$tmp/kernel32_cxx.lib"
llvm-dlltool -m i386 -k -d "$tmp/msvcrt_cxx.def" -l "$tmp/msvcrt_cxx.lib"
clang++ --target=i686-pc-windows-msvc -O1 -fexceptions -fcxx-exceptions -fno-builtin -fno-stack-protector \
    -fno-rtti -c "$src/cxx32.cpp" -o "$tmp/cxx32.obj"
clang++ --target=i686-pc-windows-msvc -O1 -fexceptions -fcxx-exceptions -fasync-exceptions -fms-extensions -fno-builtin \
    -fno-stack-protector -fno-rtti -c "$src/cxx32_seh.cpp" -o "$tmp/cxx32_seh.obj"
lld-link /nologo /machine:x86 /entry:entry /subsystem:console /nodefaultlib /safeseh:no \
    "$tmp/cxx32.obj" "$tmp/cxx32_seh.obj" "$tmp/kernel32_cxx.lib" "$tmp/msvcrt_cxx.lib" /out:"$out/cxx32.exe"
# Fenêtres (#232, étape 5) : user32 et gdi32 en x86, toujours sans CRT.
cat >"$tmp/kernel32_gui.def" <<'DEF'
LIBRARY kernel32.dll
EXPORTS
GetStdHandle@4
WriteFile@20
ExitProcess@4
GetCommandLineA@0
lstrlenA@4
GetModuleHandleA@4
Sleep@4
DEF
cat >"$tmp/user32.def" <<'DEF'
LIBRARY user32.dll
EXPORTS
RegisterClassExA@4
CreateWindowExA@48
DefWindowProcA@16
ShowWindow@8
UpdateWindow@4
GetMessageA@16
PeekMessageA@20
TranslateMessage@4
DispatchMessageA@4
PostMessageA@16
SendMessageA@16
PostQuitMessage@4
DestroyWindow@4
BeginPaint@8
EndPaint@8
FillRect@12
GetClientRect@8
GetWindowLongA@8
SetWindowLongA@12
CallWindowProcA@20
GetWindowTextA@12
SetTimer@16
KillTimer@8
InvalidateRect@12
LoadCursorA@8
DialogBoxIndirectParamA@20
EndDialog@8
IsWindow@4
CharUpperBuffW@8
CharUpperBuffA@8
SendMessageTimeoutA@28
GetScrollBarInfo@12
DEF
cat >"$tmp/gdi32.def" <<'DEF'
LIBRARY gdi32.dll
EXPORTS
CreateSolidBrush@4
CreateBrushIndirect@4
DeleteObject@4
SelectObject@8
TextOutA@20
Rectangle@20
SetTextColor@8
SetBkMode@8
CreateCompatibleDC@4
CreateCompatibleBitmap@12
DeleteDC@4
CreateDIBSection@24
GetObjectA@12
GetPixel@12
GdiFlush@0
CreatePenIndirect@4
SetDCBrushColor@8
DEF
for lib in kernel32_gui user32 gdi32; do
    llvm-dlltool -m i386 -k -d "$tmp/$lib.def" -l "$tmp/$lib.lib"
done
clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/gui32.c" -o "$tmp/gui32.obj"
lld-link /nologo /machine:x86 /entry:entry /subsystem:windows /nodefaultlib /safeseh:no \
    "$tmp/gui32.obj" "$tmp/kernel32_gui.lib" "$tmp/user32.lib" "$tmp/gdi32.lib" /out:"$out/gui32.exe"
# Anciens jeux (#237, #777) : DirectDraw, MIDI, timeSetEvent et waveOut,
# en x86 (jeux32.exe) et en x64 (jeux64.exe) pour les deux dispositions.
cat >"$tmp/jeux.def" <<'DEF'
LIBRARY kernel32.dll
EXPORTS
GetStdHandle@4
WriteFile@20
ExitProcess@4
Sleep@4
GetTickCount@0
GetModuleHandleA@4
CreateEventA@16
WaitForSingleObject@8
GetEnvironmentVariableA@12
DEF
cat >"$tmp/jeux_user32.def" <<'DEF'
LIBRARY user32.dll
EXPORTS
RegisterClassExA@4
CreateWindowExA@48
DefWindowProcA@16
FillRect@12
DEF
cat >"$tmp/jeux_gdi32.def" <<'DEF'
LIBRARY gdi32.dll
EXPORTS
CreateSolidBrush@4
DeleteObject@4
DEF
cat >"$tmp/jeux_ddraw.def" <<'DEF'
LIBRARY ddraw.dll
EXPORTS
DirectDrawCreate@12
DirectDrawCreateEx@16
DirectDrawEnumerateA@8
DEF
cat >"$tmp/jeux_d3d8.def" <<'DEF'
LIBRARY d3d8.dll
EXPORTS
Direct3DCreate8@4
DEF
cat >"$tmp/jeux_dsound.def" <<'DEF'
LIBRARY dsound.dll
EXPORTS
DirectSoundCreate8@12
DirectSoundEnumerateA@8
DEF
cat >"$tmp/jeux_dplayx.def" <<'DEF'
LIBRARY dplayx.dll
EXPORTS
DirectPlayCreate@12
DirectPlayLobbyCreateA@20
DEF
cat >"$tmp/jeux_msacm32.def" <<'DEF'
LIBRARY msacm32.dll
EXPORTS
acmMetrics@12
acmFormatSuggest@20
acmFormatTagDetailsA@12
acmStreamOpen@32
acmStreamSize@16
acmStreamPrepareHeader@12
acmStreamUnprepareHeader@12
acmStreamConvert@12
acmStreamClose@8
DEF
cat >"$tmp/jeux_dpnet.def" <<'DEF'
LIBRARY dpnet.dll
EXPORTS
DirectPlay8Create@12
DirectPlay8AddressCreate@12
DEF
cat >"$tmp/jeux_ole32.def" <<'DEF'
LIBRARY ole32.dll
EXPORTS
CoInitialize@4
CoCreateInstance@20
DEF
cat >"$tmp/jeux_d3d9.def" <<'DEF'
LIBRARY d3d9.dll
EXPORTS
Direct3DCreate9@4
DEF
cat >"$tmp/jeux_d3dx9.def" <<'DEF'
LIBRARY d3dx9_43.dll
EXPORTS
D3DXMatrixRotationZ@8
D3DXMatrixTranslation@16
D3DXMatrixScaling@16
D3DXMatrixMultiply@12
D3DXMatrixInverse@12
D3DXMatrixDeterminant@4
D3DXFloat32To16Array@12
D3DXCreateTextureFromFileInMemory@16
D3DXCreateTextureFromFileInMemoryEx@60
D3DXCreateSprite@8
D3DXCreateFontA@48
D3DXCreateBuffer@8
D3DXAssembleShader@28
D3DXGetShaderConstantTable@8
D3DXGetShaderInputSemantics@12
D3DXCreateBox@24
D3DXCreateSphere@24
D3DXComputeNormals@8
D3DXLoadMeshFromXInMemory@36
D3DXCompileShader@40
D3DXCreateEffect@36
DEF
cat >"$tmp/jeux_winmm.def" <<'DEF'
LIBRARY winmm.dll
EXPORTS
midiOutGetNumDevs@0
midiOutGetDevCapsA@12
midiOutOpen@20
midiOutShortMsg@8
midiOutClose@4
midiOutPrepareHeader@12
midiOutUnprepareHeader@12
midiStreamOpen@24
midiStreamOut@12
midiStreamRestart@4
midiStreamProperty@12
midiStreamPosition@12
midiStreamClose@4
midiInGetNumDevs@0
midiInGetDevCapsA@12
midiInOpen@20
midiInPrepareHeader@12
midiInUnprepareHeader@12
midiInAddBuffer@12
midiInStart@4
midiInStop@4
midiInReset@4
midiInClose@4
timeBeginPeriod@4
timeSetEvent@20
timeKillEvent@4
waveOutOpen@24
waveOutPrepareHeader@12
waveOutUnprepareHeader@12
waveOutWrite@12
waveOutClose@4
DEF
mkdir -p "$tmp/jeux64"
for lib in jeux jeux_user32 jeux_gdi32 jeux_ddraw jeux_d3d8 jeux_d3d9 jeux_d3dx9 jeux_dsound jeux_ole32 jeux_dplayx jeux_dpnet jeux_msacm32 jeux_winmm; do
    llvm-dlltool -m i386 -k -d "$tmp/$lib.def" -l "$tmp/$lib.lib"
    sed 's/@[0-9]*$//' "$tmp/$lib.def" >"$tmp/jeux64/$lib.def"
    llvm-dlltool -m i386:x86-64 -d "$tmp/jeux64/$lib.def" -l "$tmp/jeux64/$lib.lib"
done
clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/jeux32.c" -o "$tmp/jeux32.obj"
lld-link /nologo /machine:x86 /entry:entry /subsystem:windows /nodefaultlib /safeseh:no \
    "$tmp/jeux32.obj" "$tmp/jeux.lib" "$tmp/jeux_user32.lib" "$tmp/jeux_gdi32.lib" \
    "$tmp/jeux_ddraw.lib" "$tmp/jeux_d3d8.lib" "$tmp/jeux_d3d9.lib" "$tmp/jeux_d3dx9.lib" "$tmp/jeux_dsound.lib" "$tmp/jeux_ole32.lib" "$tmp/jeux_dplayx.lib" "$tmp/jeux_dpnet.lib" "$tmp/jeux_msacm32.lib" "$tmp/jeux_winmm.lib" /out:"$out/jeux32.exe"
clang --target=x86_64-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/jeux32.c" -o "$tmp/jeux64/jeux64.obj"
lld-link /nologo /machine:x64 /entry:entry /subsystem:windows /nodefaultlib \
    "$tmp/jeux64/jeux64.obj" "$tmp/jeux64/jeux.lib" "$tmp/jeux64/jeux_user32.lib" \
    "$tmp/jeux64/jeux_gdi32.lib" "$tmp/jeux64/jeux_ddraw.lib" "$tmp/jeux64/jeux_d3d8.lib" "$tmp/jeux64/jeux_d3d9.lib" "$tmp/jeux64/jeux_d3dx9.lib" "$tmp/jeux64/jeux_dsound.lib" "$tmp/jeux64/jeux_ole32.lib" "$tmp/jeux64/jeux_dplayx.lib" "$tmp/jeux64/jeux_dpnet.lib" "$tmp/jeux64/jeux_msacm32.lib" "$tmp/jeux64/jeux_winmm.lib" \
    /out:"$out/jeux64.exe"
# WASAPI (banc pygame32) : wasapi32.exe (x86) et wasapi64.exe (x64).
cat >"$tmp/wasapi_k32.def" <<'DEF'
LIBRARY kernel32.dll
EXPORTS
GetStdHandle@4
WriteFile@20
ExitProcess@4
DEF
cat >"$tmp/wasapi_ole32.def" <<'DEF'
LIBRARY ole32.dll
EXPORTS
CoInitializeEx@8
CoCreateInstance@20
CoTaskMemFree@4
DEF
mkdir -p "$tmp/wasapi64"
for lib in wasapi_k32 wasapi_ole32; do
    llvm-dlltool -m i386 -k -d "$tmp/$lib.def" -l "$tmp/$lib.lib"
    sed 's/@[0-9]*$//' "$tmp/$lib.def" >"$tmp/wasapi64/$lib.def"
    llvm-dlltool -m i386:x86-64 -d "$tmp/wasapi64/$lib.def" -l "$tmp/wasapi64/$lib.lib"
done
clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/wasapi32.c" -o "$tmp/wasapi32.obj"
lld-link /nologo /machine:x86 /entry:entry /subsystem:console /nodefaultlib /safeseh:no \
    "$tmp/wasapi32.obj" "$tmp/wasapi_k32.lib" "$tmp/wasapi_ole32.lib" /out:"$out/wasapi32.exe"
clang --target=x86_64-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/wasapi32.c" -o "$tmp/wasapi64/wasapi64.obj"
lld-link /nologo /machine:x64 /entry:entry /subsystem:console /nodefaultlib \
    "$tmp/wasapi64/wasapi64.obj" "$tmp/wasapi64/wasapi_k32.lib" "$tmp/wasapi64/wasapi_ole32.lib" \
    /out:"$out/wasapi64.exe"
# XAudio2 2.9 (#2428) : xaudio32.exe (x86) et xaudio64.exe (x64).
cat >"$tmp/xaudio_k32.def" <<'DEF'
LIBRARY kernel32.dll
EXPORTS
GetStdHandle@4
WriteFile@20
ExitProcess@4
Sleep@4
GetTickCount@0
DEF
cat >"$tmp/xaudio_xa29.def" <<'DEF'
LIBRARY xaudio2_9.dll
EXPORTS
XAudio2Create@12
CreateAudioVolumeMeter@4
DEF
mkdir -p "$tmp/xaudio64"
for lib in xaudio_k32 xaudio_xa29; do
    llvm-dlltool -m i386 -k -d "$tmp/$lib.def" -l "$tmp/$lib.lib"
    sed 's/@[0-9]*$//' "$tmp/$lib.def" >"$tmp/xaudio64/$lib.def"
    llvm-dlltool -m i386:x86-64 -d "$tmp/xaudio64/$lib.def" -l "$tmp/xaudio64/$lib.lib"
done
clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/xaudio32.c" -o "$tmp/xaudio32.obj"
lld-link /nologo /machine:x86 /entry:entry /subsystem:console /nodefaultlib /safeseh:no \
    "$tmp/xaudio32.obj" "$tmp/xaudio_k32.lib" "$tmp/xaudio_xa29.lib" /out:"$out/xaudio32.exe"
clang --target=x86_64-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/xaudio32.c" -o "$tmp/xaudio64/xaudio64.obj"
lld-link /nologo /machine:x64 /entry:entry /subsystem:console /nodefaultlib \
    "$tmp/xaudio64/xaudio64.obj" "$tmp/xaudio64/xaudio_k32.lib" "$tmp/xaudio64/xaudio_xa29.lib" \
    /out:"$out/xaudio64.exe"
# Direct3D 11 et DXGI (#1090) : d3d1132.exe (x86) et d3d1164.exe (x64).
cat >"$tmp/d3d11_k32.def" <<'DEF'
LIBRARY kernel32.dll
EXPORTS
GetStdHandle@4
WriteFile@20
ExitProcess@4
GetModuleHandleA@4
DEF
cat >"$tmp/d3d11_user32.def" <<'DEF'
LIBRARY user32.dll
EXPORTS
RegisterClassExA@4
CreateWindowExA@48
DefWindowProcA@16
DEF
cat >"$tmp/d3d11_d3d11.def" <<'DEF'
LIBRARY d3d11.dll
EXPORTS
D3D11CreateDevice@40
D3D11CreateDeviceAndSwapChain@48
DEF
cat >"$tmp/d3d11_dxgi.def" <<'DEF'
LIBRARY dxgi.dll
EXPORTS
CreateDXGIFactory@8
DEF
mkdir -p "$tmp/d3d1164"
for lib in d3d11_k32 d3d11_user32 d3d11_d3d11 d3d11_dxgi; do
    llvm-dlltool -m i386 -k -d "$tmp/$lib.def" -l "$tmp/$lib.lib"
    sed 's/@[0-9]*$//' "$tmp/$lib.def" >"$tmp/d3d1164/$lib.def"
    llvm-dlltool -m i386:x86-64 -d "$tmp/d3d1164/$lib.def" -l "$tmp/d3d1164/$lib.lib"
done
clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/d3d1132.c" -o "$tmp/d3d1132.obj"
lld-link /nologo /machine:x86 /entry:entry /subsystem:windows /nodefaultlib /safeseh:no \
    "$tmp/d3d1132.obj" "$tmp/d3d11_k32.lib" "$tmp/d3d11_user32.lib" "$tmp/d3d11_d3d11.lib" \
    "$tmp/d3d11_dxgi.lib" /out:"$out/d3d1132.exe"
clang --target=x86_64-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/d3d1132.c" -o "$tmp/d3d1164/d3d1164.obj"
lld-link /nologo /machine:x64 /entry:entry /subsystem:windows /nodefaultlib \
    "$tmp/d3d1164/d3d1164.obj" "$tmp/d3d1164/d3d11_k32.lib" "$tmp/d3d1164/d3d11_user32.lib" \
    "$tmp/d3d1164/d3d11_d3d11.lib" "$tmp/d3d1164/d3d11_dxgi.lib" /out:"$out/d3d1164.exe"
# Direct3D 10 et 10.1 sur Direct3D 11 (#1090, étape 2) : d3d1032.exe (x86)
# et d3d1064.exe (x64).
cat >"$tmp/d3d10_d3d10.def" <<'DEF'
LIBRARY d3d10.dll
EXPORTS
D3D10CreateDevice@24
D3D10CreateDeviceAndSwapChain@32
DEF
cat >"$tmp/d3d10_d3d10_1.def" <<'DEF'
LIBRARY d3d10_1.dll
EXPORTS
D3D10CreateDevice1@28
DEF
mkdir -p "$tmp/d3d1064"
for lib in d3d10_d3d10 d3d10_d3d10_1; do
    llvm-dlltool -m i386 -k -d "$tmp/$lib.def" -l "$tmp/$lib.lib"
    sed 's/@[0-9]*$//' "$tmp/$lib.def" >"$tmp/d3d1064/$lib.def"
    llvm-dlltool -m i386:x86-64 -d "$tmp/d3d1064/$lib.def" -l "$tmp/d3d1064/$lib.lib"
done
clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/d3d1032.c" -o "$tmp/d3d1032.obj"
lld-link /nologo /machine:x86 /entry:entry /subsystem:windows /nodefaultlib /safeseh:no \
    "$tmp/d3d1032.obj" "$tmp/d3d11_k32.lib" "$tmp/d3d11_user32.lib" "$tmp/d3d10_d3d10.lib" \
    "$tmp/d3d10_d3d10_1.lib" /out:"$out/d3d1032.exe"
clang --target=x86_64-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src/d3d1032.c" -o "$tmp/d3d1064/d3d1064.obj"
lld-link /nologo /machine:x64 /entry:entry /subsystem:windows /nodefaultlib \
    "$tmp/d3d1064/d3d1064.obj" "$tmp/d3d1164/d3d11_k32.lib" "$tmp/d3d1164/d3d11_user32.lib" \
    "$tmp/d3d1064/d3d10_d3d10.lib" "$tmp/d3d1064/d3d10_d3d10_1.lib" /out:"$out/d3d1064.exe"
# CRT 32 bits (#232, étape 3) : vrai programme MinGW i686 lié à msvcrt.dll,
# avec le printf de MinGW puis avec celui de msvcrt. Facultatif : sans
# i686-w64-mingw32-gcc, le test le saute.
if command -v i686-w64-mingw32-gcc >/dev/null 2>&1; then
    i686-w64-mingw32-gcc -O1 "$src/crt32.c" -o "$out/crt32.exe"
    i686-w64-mingw32-gcc -O1 -D__USE_MINGW_ANSI_STDIO=0 "$src/crt32.c" -o "$out/crt32_msvcrt.exe"
    # Winsock x86 (#1408) : AcceptEx, ConnectEx, port de complétion.
    # « large address aware » : mémoire x86 jusqu'à 4 Go (#1438).
    i686-w64-mingw32-gcc -O1 "$src/net32.c" -o "$out/net32.exe" -lws2_32 -Wl,--large-address-aware
else
    echo "wow32 : i686-w64-mingw32-gcc absent, crt32.exe non construit" >&2
fi
# Threads, TLS, DLL x86, LoadLibrary (#232, étape 4) : clang en cible
# i686-w64-mingw32 (TLS statique natif) avec les bibliothèques de MinGW,
# et g++ pour le C++.
if command -v i686-w64-mingw32-gcc >/dev/null 2>&1; then
    i686-w64-mingw32-gcc -O1 "$src/thread32.c" -o "$out/thread32_gcc.exe"
    # Fibres x86 (étape 7, WinSCP).
    i686-w64-mingw32-gcc -O1 "$src/fiber32.c" -o "$out/fiber32.exe"
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
# COM, contrôles communs et boîtes communes (#232, étape 6) : vrais
# en-têtes MinGW, structures dans leur disposition x86.
if command -v i686-w64-mingw32-gcc >/dev/null 2>&1; then
    i686-w64-mingw32-gcc -O1 "$src/com32.c" -o "$out/com32.exe" -lole32 -loleaut32 -lshell32 -lcomctl32 -luuid
    i686-w64-mingw32-gcc -O1 "$src/dlg32.c" -o "$out/dlg32.exe" -lcomctl32 -lcomdlg32 -lshell32 -lole32 -lgdi32 -luuid
    # MSXML intégré vu d'un programme x86 (#1084) : même source qu'en 64 bits.
    i686-w64-mingw32-gcc -O1 "$src/../msxml_suite.c" -o "$out/msxml32.exe" -lole32 -loleaut32 -luuid -lmsxml2
    # AccPropServices vu d'un programme x86 (#1114) : MSAAPROPID et VARIANT
    # passés par valeur sur la pile.
    i686-w64-mingw32-gcc -O1 "$src/../accprop_suite.c" -o "$out/accprop32.exe" -lole32 -loleaut32 -luuid
    # WMI vu d'un programme x86 (#1198) : même source qu'en 64 bits.
    i686-w64-mingw32-gcc -O1 "$src/../wmi_suite.c" -o "$out/wmi32.exe" -lole32 -loleaut32 -lwbemuuid
fi

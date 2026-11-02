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
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nRaiseException\nAddVectoredExceptionHandler\nRemoveVectoredExceptionHandler\nVirtualAlloc\nCreateThread\nWaitForSingleObject\nGetExitCodeThread\nGetCommandLineA\n' >"$tmp/kernel32.def"
llvm-dlltool -m i386:x86-64 -d "$tmp/vcruntime140.def" -l "$tmp/vcruntime140.lib"
llvm-dlltool -m i386:x86-64 -d "$tmp/kernel32.def" -l "$tmp/kernel32.lib"
# Table virtuelle de type_info : seul son adresse compte ici.
printf '  .data\n  .globl "??_7type_info@@6B@"\n"??_7type_info@@6B@":\n  .quad 0\n' >"$tmp/typeinfo.s"
clang --target=x86_64-pc-windows-msvc -c "$tmp/typeinfo.s" -o "$tmp/typeinfo.obj"
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -fexceptions -fcxx-exceptions \
    -c "$src/eh_msvc.cpp" -o "$tmp/eh_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/eh_msvc.obj" "$tmp/typeinfo.obj" \
    "$tmp/vcruntime140.lib" "$tmp/kernel32.lib" /out:"$out/eh_msvc.exe"
# Fautes matérielles en exceptions SEH (#269) : __try autour d'un accès à
# NULL, d'une division par zéro, d'int3, d'ud2, d'une page de garde…
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -fasync-exceptions -c "$src/hwexc_msvc.c" -o "$tmp/hwexc_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/hwexc_msvc.obj" \
    "$tmp/vcruntime140.lib" "$tmp/kernel32.lib" /out:"$out/hwexc_msvc.exe"
# Manettes et périphériques HID (#271) : setupapi, hid.dll, ReadFile.
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nCreateFileW\nReadFile\nWriteFile\nGetOverlappedResult\nCreateEventW\nCloseHandle\nGetLastError\n' >"$tmp/kernel32_hid.def"
printf 'LIBRARY setupapi.dll\nEXPORTS\nSetupDiGetClassDevsW\nSetupDiEnumDeviceInterfaces\nSetupDiGetDeviceInterfaceDetailW\nSetupDiGetDeviceRegistryPropertyW\nSetupDiDestroyDeviceInfoList\n' >"$tmp/setupapi.def"
printf 'LIBRARY hid.dll\nEXPORTS\nHidD_GetHidGuid\nHidD_GetAttributes\nHidD_GetProductString\nHidD_GetPreparsedData\nHidD_FreePreparsedData\nHidP_GetCaps\n' >"$tmp/hid.def"
for d in kernel32_hid setupapi hid; do
    llvm-dlltool -m i386:x86-64 -d "$tmp/$d.def" -l "$tmp/$d.lib"
done
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/hid_msvc.c" -o "$tmp/hid_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/hid_msvc.obj" \
    "$tmp/kernel32_hid.lib" "$tmp/setupapi.lib" "$tmp/hid.lib" /out:"$out/hid_msvc.exe"
# DeviceIoControl sur volumes, disques et fichiers creux (#271).
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nCreateFileW\nDeviceIoControl\nWriteFile\nReadFile\nSetFilePointer\nCloseHandle\nGetLastError\n' >"$tmp/kernel32_devio.def"
llvm-dlltool -m i386:x86-64 -d "$tmp/kernel32_devio.def" -l "$tmp/kernel32_devio.lib"
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/devio_msvc.c" -o "$tmp/devio_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/devio_msvc.obj" \
    "$tmp/kernel32_devio.lib" /out:"$out/devio_msvc.exe"

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
# Entrée brute : souris relative et clavier des jeux de tir (#271).
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nCreateFileW\nWriteFile\nCloseHandle\nSleep\nGetTickCount64\n' >"$tmp/kernel32_raw.def"
printf 'LIBRARY user32.dll\nEXPORTS\nCreateWindowExW\nPeekMessageW\nRegisterRawInputDevices\nGetRawInputData\nGetRawInputDeviceList\nGetRawInputDeviceInfoW\n' >"$tmp/user32_raw.def"
for d in kernel32_raw user32_raw; do
    llvm-dlltool -m i386:x86-64 -d "$tmp/$d.def" -l "$tmp/$d.lib"
done
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/rawinput_msvc.c" -o "$tmp/rawinput_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/rawinput_msvc.obj" \
    "$tmp/kernel32_raw.lib" "$tmp/user32_raw.lib" /out:"$out/rawinput_msvc.exe"
# Ports série : recherche du port, DCB par texte, WaitCommEvent (#271).
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nCreateFileW\nReadFile\nWriteFile\nCloseHandle\nQueryDosDeviceW\nBuildCommDCBW\nGetCommState\nSetCommState\nSetCommTimeouts\nSetCommMask\nWaitCommEvent\nTransmitCommChar\nGetCommProperties\n' >"$tmp/kernel32_serial.def"
printf 'LIBRARY advapi32.dll\nEXPORTS\nRegOpenKeyExW\nRegQueryValueExW\n' >"$tmp/advapi32_serial.def"
for d in kernel32_serial advapi32_serial; do
    llvm-dlltool -m i386:x86-64 -d "$tmp/$d.def" -l "$tmp/$d.lib"
done
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/serial_msvc.c" -o "$tmp/serial_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/serial_msvc.obj" \
    "$tmp/kernel32_serial.lib" "$tmp/advapi32_serial.lib" /out:"$out/serial_msvc.exe"
# Arbre des périphériques : ports COM comme pyserial, cfgmgr32 (#271).
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nCreateFileW\nReadFile\nWriteFile\nCloseHandle\nSetCommTimeouts\nGetCommState\nSetCommState\n' >"$tmp/kernel32_devnodes.def"
printf 'LIBRARY advapi32.dll\nEXPORTS\nRegQueryValueExW\nRegCloseKey\n' >"$tmp/advapi32_devnodes.def"
printf 'LIBRARY setupapi.dll\nEXPORTS\nSetupDiClassGuidsFromNameW\nSetupDiGetClassDevsW\nSetupDiEnumDeviceInfo\nSetupDiOpenDevRegKey\nSetupDiGetDeviceInstanceIdW\nSetupDiGetDeviceRegistryPropertyW\nSetupDiDestroyDeviceInfoList\n' >"$tmp/setupapi_devnodes.def"
printf 'LIBRARY cfgmgr32.dll\nEXPORTS\nCM_Get_Parent\nCM_Get_Child\nCM_Get_Sibling\nCM_Locate_DevNodeW\nCM_Get_Device_IDW\nCM_Get_DevNode_Registry_PropertyW\nCM_Get_Device_Interface_List_SizeW\nCM_Get_Device_Interface_ListW\n' >"$tmp/cfgmgr32_devnodes.def"
printf 'LIBRARY user32.dll\nEXPORTS\nGetDC\n' >"$tmp/user32_devnodes.def"
printf 'LIBRARY gdi32.dll\nEXPORTS\nCreateCompatibleBitmap\nGetDIBits\n' >"$tmp/gdi32_devnodes.def"
for d in kernel32_devnodes advapi32_devnodes setupapi_devnodes cfgmgr32_devnodes user32_devnodes gdi32_devnodes; do
    llvm-dlltool -m i386:x86-64 -d "$tmp/$d.def" -l "$tmp/$d.lib"
done
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/devnodes_msvc.c" -o "$tmp/devnodes_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/devnodes_msvc.obj" \
    "$tmp/kernel32_devnodes.lib" "$tmp/advapi32_devnodes.lib" "$tmp/setupapi_devnodes.lib" \
    "$tmp/cfgmgr32_devnodes.lib" "$tmp/user32_devnodes.lib" "$tmp/gdi32_devnodes.lib" /out:"$out/devnodes_msvc.exe"
# Héritage des handles entre processus (crashpad de Chromium et d'Electron).
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nCreateEventW\nSetEvent\nWaitForSingleObject\nCreateNamedPipeW\nConnectNamedPipe\nWaitNamedPipeW\nCreatePipe\nDuplicateHandle\nGetCurrentProcess\nGetCurrentProcessId\nGetProcessId\nGetHandleInformation\nSetHandleInformation\nInitializeProcThreadAttributeList\nUpdateProcThreadAttribute\nCreateProcessW\nGetExitCodeProcess\nGetModuleFileNameW\nGetCommandLineW\nCreateFileW\nReadFile\nWriteFile\nCloseHandle\nSleep\n' >"$tmp/kernel32_inherit.def"
llvm-dlltool -m i386:x86-64 -d "$tmp/kernel32_inherit.def" -l "$tmp/kernel32_inherit.lib"
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/inherit_msvc.c" -o "$tmp/inherit_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/inherit_msvc.obj" \
    "$tmp/kernel32_inherit.lib" /out:"$out/inherit_msvc.exe"
# DuplicateHandle et mémoire entre processus, enfant suspendu (#236).
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nCreateEventW\nSetEvent\nWaitForSingleObject\nDuplicateHandle\nGetCurrentProcess\nGetCurrentProcessId\nOpenProcess\nCreateProcessW\nResumeThread\nGetExitCodeProcess\nGetModuleFileNameW\nGetModuleHandleW\nGetCommandLineW\nCreateFileMappingW\nMapViewOfFile\nReadProcessMemory\nWriteProcessMemory\nCloseHandle\nTerminateProcess\n' >"$tmp/kernel32_dupx.def"
printf 'LIBRARY ntdll.dll\nEXPORTS\nNtQueryInformationProcess\n' >"$tmp/ntdll_dupx.def"
for d in kernel32_dupx ntdll_dupx; do
    llvm-dlltool -m i386:x86-64 -d "$tmp/$d.def" -l "$tmp/$d.lib"
done
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/dupx_msvc.c" -o "$tmp/dupx_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/dupx_msvc.obj" \
    "$tmp/kernel32_dupx.lib" "$tmp/ntdll_dupx.lib" /out:"$out/dupx_msvc.exe"
# WebView2 maison sur WebKitGTK (#236).
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nGetTickCount64\nSleep\nLoadLibraryW\nGetProcAddress\nGetCommandLineW\n' >"$tmp/kernel32_wv2.def"
printf 'LIBRARY user32.dll\nEXPORTS\nRegisterClassW\nCreateWindowExW\nDefWindowProcW\nPeekMessageW\nTranslateMessage\nDispatchMessageW\n' >"$tmp/user32_wv2.def"
printf 'LIBRARY ole32.dll\nEXPORTS\nCoTaskMemFree\n' >"$tmp/ole32_wv2.def"
for d in kernel32_wv2 user32_wv2 ole32_wv2; do
    llvm-dlltool -m i386:x86-64 -d "$tmp/$d.def" -l "$tmp/$d.lib"
done
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -fshort-wchar -c "$src/webview2_msvc.c" -o "$tmp/webview2_msvc.obj"
lld-link /nologo /entry:entry /subsystem:windows /nodefaultlib "$tmp/webview2_msvc.obj" \
    "$tmp/kernel32_wv2.lib" "$tmp/user32_wv2.lib" "$tmp/ole32_wv2.lib" /out:"$out/webview2_msvc.exe"
# Fonctions vérifiées au démarrage par Chromium et Electron (#236).
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nLoadLibraryA\nLoadLibraryExA\nGetProcAddress\nGetCurrentProcess\n' >"$tmp/kernel32_chromium.def"
llvm-dlltool -m i386:x86-64 -d "$tmp/kernel32_chromium.def" -l "$tmp/kernel32_chromium.lib"
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/chromium_msvc.c" -o "$tmp/chromium_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/chromium_msvc.obj" \
    "$tmp/kernel32_chromium.lib" /out:"$out/chromium_msvc.exe"
# Rendu Direct3D 11 (#137) : shaders DXBC traduits en GLSL, relecture.
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\n' >"$tmp/kernel32_d3d11.def"
printf 'LIBRARY d3d11.dll\nEXPORTS\nD3D11CreateDevice\n' >"$tmp/d3d11.def"
for d in kernel32_d3d11 d3d11; do
    llvm-dlltool -m i386:x86-64 -d "$tmp/$d.def" -l "$tmp/$d.lib"
done
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/d3d11_msvc.c" -o "$tmp/d3d11_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/d3d11_msvc.obj" \
    "$tmp/kernel32_d3d11.lib" "$tmp/d3d11.lib" /out:"$out/d3d11_msvc.exe"
# Rendu Direct3D 11 depuis du HLSL source (D3DCompile, #463).
printf 'LIBRARY d3dcompiler_47.dll\nEXPORTS\nD3DCompile\n' >"$tmp/d3dcompiler.def"
llvm-dlltool -m i386:x86-64 -d "$tmp/d3dcompiler.def" -l "$tmp/d3dcompiler.lib"
printf 'LIBRARY dxgi.dll\nEXPORTS\nCreateDXGIFactory1\n' >"$tmp/dxgi.def"
llvm-dlltool -m i386:x86-64 -d "$tmp/dxgi.def" -l "$tmp/dxgi.lib"
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/hlsl_msvc.c" -o "$tmp/hlsl_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/hlsl_msvc.obj" \
    "$tmp/kernel32_d3d11.lib" "$tmp/d3d11.lib" "$tmp/d3dcompiler.lib" "$tmp/dxgi.lib" /out:"$out/hlsl_msvc.exe"
# winusb : appareil USB trouvé par setupapi, décrit par WinUSB (#271).
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nGetLastError\nCreateFileW\nCloseHandle\n' >"$tmp/kernel32_winusb.def"
printf 'LIBRARY setupapi.dll\nEXPORTS\nSetupDiGetClassDevsW\nSetupDiEnumDeviceInterfaces\nSetupDiGetDeviceInterfaceDetailW\nSetupDiDestroyDeviceInfoList\n' >"$tmp/setupapi_winusb.def"
printf 'LIBRARY winusb.dll\nEXPORTS\nWinUsb_Initialize\nWinUsb_Free\nWinUsb_QueryDeviceInformation\nWinUsb_GetDescriptor\nWinUsb_QueryInterfaceSettings\nWinUsb_QueryPipe\nWinUsb_SetPipePolicy\nWinUsb_GetPipePolicy\nWinUsb_ControlTransfer\nWinUsb_ReadPipe\nWinUsb_GetAssociatedInterface\n' >"$tmp/winusb_winusb.def"
for d in kernel32_winusb setupapi_winusb winusb_winusb; do
    llvm-dlltool -m i386:x86-64 -d "$tmp/$d.def" -l "$tmp/$d.lib"
done
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/winusb_msvc.c" -o "$tmp/winusb_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/winusb_msvc.obj" \
    "$tmp/kernel32_winusb.lib" "$tmp/setupapi_winusb.lib" "$tmp/winusb_winusb.lib" /out:"$out/winusb_msvc.exe"
# Définition d'écran et rampe gamma des jeux (#271).
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\n' >"$tmp/kernel32_display.def"
printf 'LIBRARY user32.dll\nEXPORTS\nEnumDisplaySettingsW\nChangeDisplaySettingsExW\n' >"$tmp/user32_display.def"
printf 'LIBRARY gdi32.dll\nEXPORTS\nGetDeviceGammaRamp\n' >"$tmp/gdi32_display.def"
for d in kernel32_display user32_display gdi32_display; do
    llvm-dlltool -m i386:x86-64 -d "$tmp/$d.def" -l "$tmp/$d.lib"
done
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/display_msvc.c" -o "$tmp/display_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/display_msvc.obj" \
    "$tmp/kernel32_display.lib" "$tmp/user32_display.lib" "$tmp/gdi32_display.lib" /out:"$out/display_msvc.exe"
# Annulation des E/S, handles et tampons invalides (#18).
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nCreateNamedPipeW\nCreateFileW\nReadFile\nWriteFile\nGetOverlappedResult\nCancelIo\nCancelIoEx\nCancelSynchronousIo\nGetCurrentThread\nCreateEventW\nWaitForSingleObject\nCloseHandle\nGetLastError\nVirtualAlloc\nDeleteFileW\n' >"$tmp/kernel32_io.def"
llvm-dlltool -m i386:x86-64 -d "$tmp/kernel32_io.def" -l "$tmp/kernel32_io.lib"
clang --target=x86_64-pc-windows-msvc -O1 -fms-extensions -c "$src/io_msvc.c" -o "$tmp/io_msvc.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/io_msvc.obj" \
    "$tmp/kernel32_io.lib" /out:"$out/io_msvc.exe"

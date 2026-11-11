# Mesure des imports Winlux — 11 novembre 2026, lot D (sortie brute de scripts/compat-imports.py)

Même corpus et même option que la [mesure du 11 novembre](mesure-2026-11-11.md)
(`--fourni=python3*.dll,shiboken6*.dll,pyside6*.dll`), prise avec les lots B
(#1510) et D (#1512 : advapi32, user32, shell32, gdi32, secur32, dsound,
cabinet, uxtheme) sur master 70e78b9.

## Avant et après le lot D

- Fonctions du lot D manquantes : 206 avant (relevé de #1512), 0 après.
- Fonctions distinctes manquantes dans les DLL intégrées : 769 avant, 558 après
  (le lot D et SetSysColors, compté dans user32).
- DLL absentes en import direct : 26.
- Reste le lot C (ole32, oleaut32 : 52 fonctions, #1511), pris par un autre
  fil, et wxPython (#1409).
- Les numéros d'ordre de cabinet (10 à 14), dsound (1, 2, 3, 6, 7, 8),
  uxtheme (47) et shell32 (28, 192, 232, 256) sont lus dans les bibliothèques
  d'importation du paquet NuGet public Microsoft.Windows.SDK.CPP.x64
  10.0.26100.4654 (sha256 81d8682f…2aad5), comme pour odbc32 (#1546).

| Logiciel | PE lus | 32 bits | DLL absentes (directes) | fonctions manquantes |
|---|---|---|---|---|
| 7zip-bin_5.2.0 | 1 | 1 | 0 | 0 |
| _ffmpeg-installer_win32-x64_4.1.0 | 1 | 0 | 0 | 3 |
| _img_sharp-win32-x64_0.35.5 | 3 | 0 | 0 | 5 |
| dotnetcore2 | 220 | 1 | 0 | 13 |
| electron-winstaller | 21 | 19 | 0 | 10 |
| esbuild | 1 | 0 | 0 | 0 |
| imageio_ffmpeg | 1 | 0 | 0 | 4 |
| jdk4py | 97 | 0 | 1 | 26 |
| nodejs | 1 | 0 | 0 | 2 |
| numpy | 21 | 0 | 0 | 0 |
| panda3d | 253 | 0 | 3 | 33 |
| pygame | 52 | 0 | 0 | 0 |
| pyinstaller | 4 | 0 | 0 | 0 |
| pyqt6_webengine_qt6 | 7 | 0 | 11 | 164 |
| pywin32 | 57 | 0 | 2 | 221 |
| qt6 | 415 | 0 | 10 | 53 |
| raylib | 2 | 0 | 0 | 0 |
| wxpython | 36 | 0 | 0 | 123 |
| x86-numpy | 19 | 19 | 0 | 1 |
| x86-pygame | 52 | 52 | 0 | 0 |
| x86-pyinstaller | 4 | 4 | 0 | 0 |

26 DLL absentes en import direct, 558 fonctions distinctes manquantes dans les DLL intégrées.

## DLL absentes, par nombre de logiciels touchés (import direct)

- mswsock.dll : 2
- foundation.dll : 1
- openmaya.dll : 1
- openmayaanim.dll : 1
- qt6quick.dll : 1
- qt6qml.dll : 1
- qt6core.dll : 1
- qt6webchannelquick.dll : 1
- qt6gui.dll : 1
- qt6network.dll : 1
- qt6webchannel.dll : 1
- qt6positioning.dll : 1
- qt6quickwidgets.dll : 1
- qt6printsupport.dll : 1
- qt6widgets.dll : 1
- mfc140u.dll : 1
- icuuc.dll : 1
- qt6pdfquick.dll : 1
- oci.dll : 1
- libpq.dll : 1
- fbclient.dll : 1
- mimapi64.dll : 1
- qt6pdf.dll : 1
- qt6webenginewidgets.dll : 1
- qt6webenginequick.dll : 1
- qt6webenginecore.dll : 1

## DLL absentes en import différé

- node.exe : 1

## Fonctions manquantes dans les DLL intégrées (top 300)

- ole32!OleLoadFromStream : 3
- ole32!OleSaveToStream : 3
- oleaut32!OleCreatePropertyFrame : 2
- bcrypt!BCryptGenerateSymmetricKey : 2
- bcrypt!BCryptSetProperty : 2
- ole32!CoUnmarshalInterface : 2
- ole32!CoMarshalInterface : 2
- ole32!CoReleaseMarshalData : 2
- ole32!CoRegisterInitializeSpy : 2
- ole32!CoRevokeInitializeSpy : 2
- oleaut32!SafeArrayGetRecordInfo : 2
- oleaut32!GetRecordInfoFromTypeInfo : 2
- crypt32!CryptFindOIDInfo : 2
- iphlpapi!GetUnicastIpAddressTable : 2
- iphlpapi!ConvertInterfaceNameToLuidW : 2
- winmm!joyGetNumDevs : 2
- winmm!joyGetPosEx : 2
- propsys!PSCreateMemoryPropertyStore : 2
- d3d12!#101 : 2
- msvcp140!?read@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@PEAD_J@Z : 2
- msvcp140!?_Init@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXPEAPEAD0PEAH001@Z : 2
- msvcp140!?epptr@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ : 2
- msvcp140!?egptr@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ : 2
- msvcp140!?pptr@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ : 2
- msvcp140!?gptr@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ : 2
- msvcp140!?eback@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ : 2
- msvcp140!?sputc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEAAHD@Z : 2
- msvcp140!?sputn@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEAA_JPEBD_J@Z : 2
- msvcp140!?setstate@?$basic_ios@DU?$char_traits@D@std@@@std@@QEAAXH_N@Z : 2
- msvcp140!?_Osfx@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAXXZ : 2
- msvcp140!?flush@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@XZ : 2
- msvcp140!??0?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAA@XZ : 2
- msvcp140!??1?$basic_streambuf@DU?$char_traits@D@std@@@std@@UEAA@XZ : 2
- msvcp140!?_Pninc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAPEADXZ : 2
- msvcp140!??1?$basic_ios@DU?$char_traits@D@std@@@std@@UEAA@XZ : 2
- msvcp140!??0?$basic_ios@DU?$char_traits@D@std@@@std@@IEAA@XZ : 2
- msvcp140!??0?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAA@PEAV?$basic_streambuf@DU?$char_traits@D@std@@@1@_N@Z : 2
- msvcp140!??1?$basic_ostream@DU?$char_traits@D@std@@@std@@UEAA@XZ : 2
- msvcp140!?imbue@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAXAEBVlocale@2@@Z : 2
- msvcp140!?sync@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAHXZ : 2
- msvcp140!?setbuf@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAPEAV12@PEAD_J@Z : 2
- msvcp140!?xsputn@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAA_JPEBD_J@Z : 2
- msvcp140!?xsgetn@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAA_JPEAD_J@Z : 2
- msvcp140!?uflow@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAHXZ : 2
- msvcp140!?showmanyc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAA_JXZ : 2
- msvcp140!?_Unlock@?$basic_streambuf@DU?$char_traits@D@std@@@std@@UEAAXXZ : 2
- msvcp140!?_Lock@?$basic_streambuf@DU?$char_traits@D@std@@@std@@UEAAXXZ : 2
- msvcp140!?write@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@PEBD_J@Z : 2
- msvcp140!??0_Lockit@std@@QEAA@H@Z : 2
- msvcp140!??1_Lockit@std@@QEAA@XZ : 2
- msvcp140!?_Getgloballocale@locale@std@@CAPEAV_Locimp@12@XZ : 2
- msvcp140!??0?$basic_istream@DU?$char_traits@D@std@@@std@@QEAA@PEAV?$basic_streambuf@DU?$char_traits@D@std@@@1@_N@Z : 2
- msvcp140!??1?$basic_istream@DU?$char_traits@D@std@@@std@@UEAA@XZ : 2
- msvcp140!?_Id_cnt@id@locale@std@@0HA : 2
- msvcp140!?always_noconv@codecvt_base@std@@QEBA_NXZ : 2
- msvcp140!?in@?$codecvt@DDU_Mbstatet@@@std@@QEBAHAEAU_Mbstatet@@PEBD1AEAPEBDPEAD3AEAPEAD@Z : 2
- msvcp140!?out@?$codecvt@DDU_Mbstatet@@@std@@QEBAHAEAU_Mbstatet@@PEBD1AEAPEBDPEAD3AEAPEAD@Z : 2
- msvcp140!?unshift@?$codecvt@DDU_Mbstatet@@@std@@QEBAHAEAU_Mbstatet@@PEAD1AEAPEAD@Z : 2
- msvcp140!?_Getcat@?$codecvt@DDU_Mbstatet@@@std@@SA_KPEAPEBVfacet@locale@2@PEBV42@@Z : 2
- msvcp140!?getloc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEBA?AVlocale@2@XZ : 2
- msvcp140!?_Gninc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAPEADXZ : 2
- msvcp140!?_Init@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXXZ : 2
- msvcp140!?_Fiopen@std@@YAPEAU_iobuf@@PEBDHH@Z : 2
- msvcp140!?id@?$codecvt@DDU_Mbstatet@@@std@@2V0locale@2@A : 2
- msvcp140!?rdbuf@?$basic_ios@DU?$char_traits@D@std@@@std@@QEAAPEAV?$basic_streambuf@DU?$char_traits@D@std@@@2@PEAV32@@Z : 2
- msvcp140!?seekpos@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAA?AV?$fpos@U_Mbstatet@@@2@V32@H@Z : 2
- msvcp140!?seekoff@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAA?AV?$fpos@U_Mbstatet@@@2@_JHH@Z : 2
- msvcp140!?underflow@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAHXZ : 2
- msvcp140!?pbackfail@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAHH@Z : 2
- msvcp140!?overflow@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAHH@Z : 2
- msvcp140!_Strcoll : 2
- msvcp140!_Strxfrm : 2
- msvcp140!??0_Locinfo@std@@QEAA@PEBD@Z : 2
- msvcp140!??1_Locinfo@std@@QEAA@XZ : 2
- msvcp140!?_Getcoll@_Locinfo@std@@QEBA?AU_Collvec@@XZ : 2
- msvcp140!??0facet@locale@std@@IEAA@_K@Z : 2
- msvcp140!??1facet@locale@std@@MEAA@XZ : 2
- msvcp140!?_Init@locale@std@@CAPEAV_Locimp@12@_N@Z : 2
- msvcp140!?tolower@?$ctype@D@std@@QEBADD@Z : 2
- msvcp140!?_Getcat@?$ctype@D@std@@SA_KPEAPEBVfacet@locale@2@PEBV42@@Z : 2
- msvcp140!?_Xregex_error@std@@YAXW4error_type@regex_constants@1@@Z : 2
- msvcp140!?_Incref@facet@locale@std@@UEAAXXZ : 2
- msvcp140!?_Decref@facet@locale@std@@UEAAPEAV_Facet_base@3@XZ : 2
- msvcp140!?id@?$ctype@D@std@@2V0locale@2@A : 2
- msvcp140!?id@?$collate@D@std@@2V0locale@2@A : 2
- msvcp140!??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEAN@Z : 2
- msvcp140!?cerr@std@@3V?$basic_ostream@DU?$char_traits@D@std@@@1@A : 2
- msvcp140!?cout@std@@3V?$basic_ostream@DU?$char_traits@D@std@@@1@A : 2
- msvcp140!?sputc@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@QEAAG_W@Z : 2
- msvcp140!?sputn@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@QEAA_JPEB_W_J@Z : 2
- msvcp140!?setstate@?$basic_ios@_WU?$char_traits@_W@std@@@std@@QEAAXH_N@Z : 2
- msvcp140!?_Osfx@?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAXXZ : 2
- msvcp140!?flush@?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV12@XZ : 2
- mfplat!MFFrameRateToAverageTimePerFrame : 2
- oleaut32!SafeArrayUnlock : 2
- oleaut32!SafeArrayLock : 2
- propsys!PropVariantToStringAlloc : 2
- crypt32!CryptEncodeObjectEx : 2
- bcrypt!BCryptDeriveKeyPBKDF2 : 1
- iphlpapi!NotifyRouteChange2 : 1
- shlwapi!StrRetToStrW : 1
- ole32!CoGetMarshalSizeMax : 1
- oleaut32!SafeArrayAllocData : 1
- oleaut32!VarCyFromDec : 1
- oleaut32!SafeArrayAllocDescriptorEx : 1
- oleaut32!SafeArraySetRecordInfo : 1
- ole32!RoGetParameterizedTypeInstanceIID : 1
- crypt32!CryptHashPublicKeyInfo : 1
- msi!#219 : 1
- shlwapi!PathIsUNCW : 1
- crypt32!CryptDecodeObject : 1
- crypt32!CertGetValidUsages : 1
- crypt32!CryptExportPublicKeyInfoEx : 1
- crypt32!CertComparePublicKeyInfo : 1
- wintrust!WTHelperProvDataFromStateData : 1
- wintrust!WTHelperGetProvSignerFromChain : 1
- ws2_32!WSAStringToAddressA : 1
- iphlpapi!IcmpCreateFile : 1
- iphlpapi!IcmpCloseHandle : 1
- iphlpapi!IcmpSendEcho : 1
- iphlpapi!IcmpSendEcho2Ex : 1
- iphlpapi!Icmp6CreateFile : 1
- iphlpapi!Icmp6SendEcho2 : 1
- iphlpapi!GetIfEntry2 : 1
- iphlpapi!GetIfTable2 : 1
- iphlpapi!GetAnycastIpAddressTable : 1
- iphlpapi!ConvertLengthToIpv4Mask : 1
- ole32!CoGetCurrentProcess : 1
- ole32!CoFileTimeNow : 1
- shlwapi!StrTrimW : 1
- winspool!FindClosePrinterChangeNotification : 1
- winspool!FindNextPrinterChangeNotification : 1
- winspool!FindFirstPrinterChangeNotification : 1
- winmm!mixerSetControlDetails : 1
- winmm!mixerGetControlDetailsA : 1
- winmm!mixerGetLineControlsA : 1
- winmm!mixerGetLineInfoA : 1
- winmm!mixerClose : 1
- winmm!mixerOpen : 1
- winmm!mixerGetDevCapsW : 1
- winmm!mixerGetNumDevs : 1
- iphlpapi!ConvertInterfaceLuidToNameA : 1
- propsys!VariantToString : 1
- msi!#48 : 1
- msi!#151 : 1
- msi!#8 : 1
- msi!#149 : 1
- msi!#17 : 1
- msi!#116 : 1
- msi!#125 : 1
- msi!#20 : 1
- msi!#118 : 1
- msi!#32 : 1
- msi!#121 : 1
- msi!#115 : 1
- msi!#160 : 1
- msi!#159 : 1
- msi!#152 : 1
- msi!#163 : 1
- msi!#153 : 1
- msi!#170 : 1
- msi!#148 : 1
- msi!#92 : 1
- msi!#166 : 1
- msi!#77 : 1
- msi!#167 : 1
- msi!#158 : 1
- msi!#123 : 1
- ws2_32!getprotobyname : 1
- iphlpapi!GetIfTable2Ex : 1
- imm32!ImmIsUIMessageA : 1
- setupapi!CM_Get_DevNode_Registry_PropertyA : 1
- winmm!joyGetDevCapsA : 1
- winspool!GetPrinterDriverW : 1
- ole32!RoGetAgileReference : 1
- setupapi!SetupDiCreateDeviceInfoList : 1
- setupapi!SetupDiOpenDeviceInfoW : 1
- winhttp!WinHttpGetProxyForUrlEx : 1
- winhttp!WinHttpGetProxyResult : 1
- winhttp!WinHttpFreeProxyResult : 1
- winhttp!WinHttpCreateProxyResolver : 1
- mmdevapi!#17 : 1
- bthprops.cpl!BluetoothSdpGetContainerElementData : 1
- bthprops.cpl!BluetoothSdpGetAttributeValue : 1
- msvcp140!?sync_with_stdio@ios_base@std@@SA_N_N@Z : 1
- msvcp140!?swap@?$basic_ostream@DU?$char_traits@D@std@@@std@@IEAAXAEAV12@@Z : 1
- msvcp140!?swap@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXAEAV12@@Z : 1
- msvcp140!?pbase@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ : 1
- msvcp140!?widen@?$ctype@D@std@@QEBADD@Z : 1
- msvcp140!??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEAH@Z : 1
- msvcp140!?seekg@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@_JH@Z : 1
- msvcp140!?copyfmt@?$basic_ios@DU?$char_traits@D@std@@@std@@QEAAAEAV12@AEBV12@@Z : 1
- msvcp140!??0?$basic_ios@DU?$char_traits@D@std@@@std@@QEAA@PEAV?$basic_streambuf@DU?$char_traits@D@std@@@1@@Z : 1
- msvcp140!??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEAI@Z : 1
- msvcp140!??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEAM@Z : 1
- msvcp140!?imbue@?$basic_ios@DU?$char_traits@D@std@@@std@@QEAA?AVlocale@2@AEBV32@@Z : 1
- msvcp140!?clear@?$basic_ios@DU?$char_traits@D@std@@@std@@QEAAXH_N@Z : 1
- msvcp140!?tellp@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAA?AV?$fpos@U_Mbstatet@@@2@XZ : 1
- msvcp140!?in_avail@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEAA_JXZ : 1
- msvcp140!?setprecision@std@@YA?AU?$_Smanip@_J@1@_J@Z : 1
- msvcp140!?_Lock@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@UEAAXXZ : 1
- msvcp140!?_Unlock@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@UEAAXXZ : 1
- msvcp140!?showmanyc@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAA_JXZ : 1
- msvcp140!?_Raise_handler@std@@3P6AXAEBVexception@stdext@@@ZEA : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@_N@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@H@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@_K@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@PEBX@Z : 1
- msvcp140!??0?$basic_iostream@DU?$char_traits@D@std@@@std@@QEAA@PEAV?$basic_streambuf@DU?$char_traits@D@std@@@1@@Z : 1
- msvcp140!??1?$basic_iostream@DU?$char_traits@D@std@@@std@@UEAA@XZ : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@I@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@P6AAEAVios_base@1@AEAV21@@Z@Z : 1
- msvcp140!?setw@std@@YA?AU?$_Smanip@_J@1@_J@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@_J@Z : 1
- msvcp140!?getloc@ios_base@std@@QEBA?AVlocale@2@XZ : 1
- msvcp140!?_Ipfx@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAA_N_N@Z : 1
- msvcp140!?tellg@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAA?AV?$fpos@U_Mbstatet@@@2@XZ : 1
- msvcp140!?sbumpc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEAAHXZ : 1
- msvcp140!?sgetc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEAAHXZ : 1
- msvcp140!?snextc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEAAHXZ : 1
- msvcp140!?widen@?$basic_ios@DU?$char_traits@D@std@@@std@@QEBADD@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@M@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@N@Z : 1
- msvcp140!?put@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@D@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@P6AAEAV01@AEAV01@@Z@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@F@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@G@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@J@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@K@Z : 1
- msvcp140!??_D?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAXXZ : 1
- msvcp140!_Lock_shared_ptr_spin_lock : 1
- msvcp140!_Unlock_shared_ptr_spin_lock : 1
- msvcp140!?tolower@?$ctype@D@std@@QEBAPEBDPEADPEBD@Z : 1
- msvcp140!??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@PEAV?$basic_streambuf@DU?$char_traits@D@std@@@1@@Z : 1
- msvcp140!?classic@locale@std@@SAAEBV12@XZ : 1
- msvcp140!??0?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEAA@XZ : 1
- msvcp140!??1?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@UEAA@XZ : 1
- msvcp140!?_Pninc@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEAAPEA_WXZ : 1
- msvcp140!??1?$basic_ios@_WU?$char_traits@_W@std@@@std@@UEAA@XZ : 1
- msvcp140!??0?$basic_ios@_WU?$char_traits@_W@std@@@std@@IEAA@XZ : 1
- msvcp140!??0?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAA@PEAV?$basic_streambuf@_WU?$char_traits@_W@std@@@1@_N@Z : 1
- msvcp140!??1?$basic_ostream@_WU?$char_traits@_W@std@@@std@@UEAA@XZ : 1
- msvcp140!??6?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV01@P6AAEAVios_base@1@AEAV21@@Z@Z : 1
- msvcp140!??6?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV01@PEBX@Z : 1
- msvcp140!?imbue@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAAXAEBVlocale@2@@Z : 1
- msvcp140!?sync@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAAHXZ : 1
- msvcp140!?setbuf@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAAPEAV12@PEA_W_J@Z : 1
- msvcp140!?xsputn@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAA_JPEB_W_J@Z : 1
- msvcp140!?xsgetn@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAA_JPEA_W_J@Z : 1
- msvcp140!?uflow@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAAGXZ : 1
- mfplat!MFCreateDXGISurfaceBuffer : 1
- mfplat!MFCreateStreamDescriptor : 1
- mfplat!MFCreatePresentationDescriptor : 1
- mfplat!MFCreateEventQueue : 1
- mfplat!MFUnlockDXGIDeviceManager : 1
- mfplat!MFLockDXGIDeviceManager : 1
- mfplat!MFSerializeAttributesToStream : 1
- mfplat!MFCreateMediaEvent : 1
- oleaut32!LHashValOfNameSys : 1
- oleaut32!UnRegisterTypeLib : 1
- oleaut32!GetRecordInfoFromGuids : 1
- oleaut32!SafeArrayCreateEx : 1
- ole32!GetRunningObjectTable : 1
- ole32!CoSetCancelObject : 1
- ole32!CreateItemMoniker : 1
- ole32!CreateFileMoniker : 1
- ole32!CreatePointerMoniker : 1
- ole32!CoGetCancelObject : 1
- ole32!CoDisableCallCancellation : 1
- ole32!CoGetObject : 1
- ole32!CoGetCallContext : 1
- ole32!CoTreatAsClass : 1
- ole32!CoEnableCallCancellation : 1
- ole32!FmtIdToPropStgName : 1
- ole32!PropStgNameToFmtId : 1
- ole32!MkParseDisplayName : 1
- oleaut32!OleLoadPicturePath : 1
- oleaut32!OleLoadPicture : 1
- shlwapi!#515 : 1
- shlwapi!AssocCreate : 1
- urlmon!CoInternetIsFeatureEnabled : 1
- urlmon!CoInternetSetFeatureEnabled : 1
- propsys!StgDeserializePropVariant : 1
- propsys!PSUnregisterPropertySchema : 1
- propsys!PSGetNamedPropertyFromPropertyStorage : 1
- propsys!PSRegisterPropertySchema : 1
- propsys!PSGetPropertySystem : 1
- propsys!PSGetNameFromPropertyKey : 1
- propsys!PSGetPropertyFromPropertyStorage : 1
- propsys!PSGetPropertyDescription : 1
- propsys!StgSerializePropVariant : 1
- propsys!PSCreatePropertyChangeArray : 1
- propsys!PSGetItemPropertyHandler : 1
- propsys!PSCreateSimplePropertyChange : 1
- propsys!PropVariantChangeType : 1
- propsys!PSLookupPropertyHandlerCLSID : 1
- winspool!DeletePrinterDriverW : 1
- winspool!EnumPrintProcessorDatatypesW : 1
- winspool!EnumMonitorsW : 1
- winspool!GetFormW : 1

## Détail par logiciel

### 7zip-bin_5.2.0

- 32 bits : 1 fichiers (ex. package/win/ia32/7za.exe)
- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### _ffmpeg-installer_win32-x64_4.1.0

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : ole32 2, oleaut32 1

### _img_sharp-win32-x64_0.35.5

- DLL absentes : aucune
- DLL absentes (différées) : node.exe
- Fonctions manquantes par DLL : bcrypt 3, iphlpapi 1, shlwapi 1

### dotnetcore2

- 32 bits : 1 fichiers (ex. dotnetcore2/bin/shared/Microsoft.NETCore.App/3.1.23/mscorlib.dll)
- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : ole32 7, oleaut32 6

### electron-winstaller

- 32 bits : 19 fichiers (ex. package/vendor/wconsole.dll, package/vendor/winterop.dll, package/vendor/7z-arm64.exe)
- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : crypt32 6, wintrust 2, msi 1, shlwapi 1

### esbuild

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### imageio_ffmpeg

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : ole32 2, oleaut32 1, ws2_32 1

### jdk4py

- DLL absentes : mswsock.dll (1)
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : iphlpapi 12, winmm 8, winspool 3, ole32 2, shlwapi 1

### nodejs

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : iphlpapi 2

### numpy

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### panda3d

- DLL absentes : foundation.dll (45), openmaya.dll (45), openmayaanim.dll (45)
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : msi 25, winmm 3, propsys 1, ws2_32 1, iphlpapi 1, imm32 1, setupapi 1

### pygame

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### pyinstaller

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### pyqt6_webengine_qt6

- DLL absentes : qt6core.dll (7), qt6qml.dll (5), qt6quick.dll (4), qt6gui.dll (3), qt6network.dll (2), qt6webchannelquick.dll (1), qt6webchannel.dll (1), qt6positioning.dll (1), qt6quickwidgets.dll (1), qt6printsupport.dll (1), qt6widgets.dll (1)
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : msvcp140 140, mfplat 9, winhttp 4, ole32 3, setupapi 2, bthprops.cpl 2, winspool 1, propsys 1, mmdevapi 1, d3d12 1

### pywin32

- DLL absentes : mfc140u.dll (4), mswsock.dll (1)
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : netapi32 58, crypt32 40, wininet 21, ole32 19, winspool 19, propsys 16, wtsapi32 14, pdh 13, oleaut32 10, userenv 7, shlwapi 2, urlmon 2

### qt6

- DLL absentes : icuuc.dll (1), qt6pdfquick.dll (1), oci.dll (1), libpq.dll (1), fbclient.dll (1), mimapi64.dll (1), qt6pdf.dll (1), qt6webenginewidgets.dll (1), qt6webenginequick.dll (1), qt6webenginecore.dll (1)
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : ole32 8, propsys 5, crypt32 5, mf 5, ws2_32 4, mfplat 4, bcrypt 4, dnsapi 3, concrt140 2, d3dcompiler_47 2, d3d12 2, iphlpapi 2, oleaut32 2, ntdll 1, d3d11 1, dxva2 1, mfreadwrite 1, d2d1 1

### raylib

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### wxpython

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : msvcp140 114, winmm 6, oleaut32 3

### x86-numpy

- 32 bits : 19 fichiers (ex. numpy/random/_generator.cp312-win32.pyd, numpy/random/_bounded_integers.cp312-win32.pyd, numpy/random/bit_generator.cp312-win32.pyd)
- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : msvcp140 1

### x86-pygame

- 32 bits : 52 fichiers (ex. pygame/base.cp312-win32.pyd, pygame/pypm.cp312-win32.pyd, pygame/SDL2.dll)
- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### x86-pyinstaller

- 32 bits : 4 fichiers (ex. PyInstaller/bootloader/Windows-32bit-intel/runw_d.exe, PyInstaller/bootloader/Windows-32bit-intel/runw.exe, PyInstaller/bootloader/Windows-32bit-intel/run.exe)
- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune


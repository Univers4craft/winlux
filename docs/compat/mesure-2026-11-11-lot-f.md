# Mesure des imports Winlux — 11 novembre 2026, lot F (sortie brute de scripts/compat-imports.py)

Même corpus et même option que la [mesure du lot E](mesure-2026-11-11-lot-e.md)
(`--fourni=python3*.dll,shiboken6*.dll,pyside6*.dll`), prise avec le lot F
(#1615 : propsys, mfplat, mf, mfreadwrite, winmm, bcrypt, crypt32, d3d12,
urlmon, concrt140, d3dcompiler_47, imm32, mmdevapi, d3d11, dxva2, d2d1).

## Avant et après le lot F

- Fonctions du lot F manquantes : 113 avant (relevé de #1615), 0 après.
- Fonctions distinctes manquantes dans les DLL intégrées : 299 après le lot E,
  181 avec le lot F repris sur master 0dc3683 (après la fusion de #1646).
- Les numéros d'ordre (d3d12 101 et 102, d2d1 1, mmdevapi 17) sont lus dans
  les bibliothèques d'importation du paquet NuGet public
  Microsoft.Windows.SDK.CPP.x64 10.0.26100.4654 (sha256 81d8682f…2aad5).
- Servies mais en échec documenté (E_NOTIMPL tracé, sortie à NULL), faute de
  chaîne Media Foundation complète ou d'interopérabilité WinRT de Direct3D :
  MFCreateAggregateSource, MFCreateAudioRenderer(Activate), MFCreateTopoLoader,
  MFCreateVideoRendererActivate, MFCreateSinkWriterFromMediaSink,
  MFCreateDXGISurfaceBuffer, D3DCreateLinker, D3DLoadModule,
  CreateDirect3D11DeviceFromDXGIDevice, et quatre fonctions PS* de propsys.

# Mesure des imports Winlux

| Logiciel | PE lus | 32 bits | DLL absentes (directes) | fonctions manquantes |
|---|---|---|---|---|
| 7zip-bin_5.2.0 | 1 | 1 | 0 | 0 |
| _ffmpeg-installer_win32-x64_4.1.0 | 1 | 0 | 0 | 0 |
| _img_sharp-win32-x64_0.35.5 | 3 | 0 | 0 | 0 |
| dotnetcore2 | 220 | 1 | 0 | 0 |
| electron-winstaller | 21 | 19 | 0 | 0 |
| esbuild | 1 | 0 | 0 | 0 |
| imageio_ffmpeg | 1 | 0 | 0 | 0 |
| jdk4py | 97 | 0 | 1 | 0 |
| nodejs | 1 | 0 | 0 | 0 |
| numpy | 21 | 0 | 0 | 0 |
| panda3d | 253 | 0 | 3 | 0 |
| pygame | 52 | 0 | 0 | 0 |
| pyinstaller | 4 | 0 | 0 | 0 |
| pyqt6_webengine_qt6 | 7 | 0 | 11 | 140 |
| pywin32 | 57 | 0 | 2 | 0 |
| qt6 | 415 | 0 | 10 | 0 |
| raylib | 2 | 0 | 0 | 0 |
| wxpython | 36 | 0 | 0 | 114 |
| x86-numpy | 19 | 19 | 0 | 1 |
| x86-pygame | 52 | 52 | 0 | 0 |
| x86-pyinstaller | 4 | 4 | 0 | 0 |

26 DLL absentes en import direct, 181 fonctions distinctes manquantes dans les DLL intégrées.

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
- msvcp140!__crtLCMapStringW : 1
- msvcp140!__crtLCMapStringA : 1
- msvcp140!?id@?$collate@_W@std@@2V0locale@2@A : 1
- msvcp140!?id@?$ctype@_W@std@@2V0locale@2@A : 1
- msvcp140!?_Getcat@?$ctype@_W@std@@SA_KPEAPEBVfacet@locale@2@PEBV42@@Z : 1
- msvcp140!?tolower@?$ctype@_W@std@@QEBA_W_W@Z : 1
- msvcp140!?is@?$ctype@_W@std@@QEBA_NF_W@Z : 1
- msvcp140!?c_str@?$_Yarn@D@std@@QEBAPEBDXZ : 1
- msvcp140!_Wcsxfrm : 1
- msvcp140!_Wcscoll : 1
- msvcp140!?gcount@?$basic_istream@DU?$char_traits@D@std@@@std@@QEBA_JXZ : 1
- msvcp140!?_Pnavail@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBA_JXZ : 1
- msvcp140!?pbump@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXH@Z : 1
- msvcp140!?_Gnavail@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBA_JXZ : 1
- msvcp140!?_Gndec@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAPEADXZ : 1
- msvcp140!?setg@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXPEAD00@Z : 1
- msvcp140!?gbump@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXH@Z : 1
- msvcp140!?bad@ios_base@std@@QEBA_NXZ : 1
- msvcp140!?eof@ios_base@std@@QEBA_NXZ : 1
- msvcp140!??7ios_base@std@@QEBA_NXZ : 1
- msvcp140!?rdbuf@?$basic_ios@DU?$char_traits@D@std@@@std@@QEBAPEAV?$basic_streambuf@DU?$char_traits@D@std@@@2@XZ : 1
- msvcp140!?fill@?$basic_ios@_WU?$char_traits@_W@std@@@std@@QEBA_WXZ : 1
- msvcp140!?rdbuf@?$basic_ios@_WU?$char_traits@_W@std@@@std@@QEBAPEAV?$basic_streambuf@_WU?$char_traits@_W@std@@@2@XZ : 1
- msvcp140!?tie@?$basic_ios@_WU?$char_traits@_W@std@@@std@@QEBAPEAV?$basic_ostream@_WU?$char_traits@_W@std@@@2@XZ : 1
- msvcp140!?fill@?$basic_ios@DU?$char_traits@D@std@@@std@@QEBADXZ : 1
- msvcp140!?tie@?$basic_ios@DU?$char_traits@D@std@@@std@@QEBAPEAV?$basic_ostream@DU?$char_traits@D@std@@@2@XZ : 1
- msvcp140!?width@ios_base@std@@QEAA_J_J@Z : 1
- msvcp140!?width@ios_base@std@@QEBA_JXZ : 1
- msvcp140!?flags@ios_base@std@@QEBAHXZ : 1
- msvcp140!?good@ios_base@std@@QEBA_NXZ : 1
- msvcp140!?init@?$basic_ios@DU?$char_traits@D@std@@@std@@IEAAXPEAV?$basic_streambuf@DU?$char_traits@D@std@@@2@_N@Z : 1
- msvcp140!??0?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAA@AEBV01@@Z : 1
- msvcp140!?__ExceptionPtrRethrow@@YAXPEBX@Z : 1
- msvcp140!?__ExceptionPtrCurrentException@@YAXPEAX@Z : 1
- msvcp140!?__ExceptionPtrToBool@@YA_NPEBX@Z : 1
- msvcp140!?__ExceptionPtrCreate@@YAXPEAX@Z : 1
- msvcp140!?__ExceptionPtrDestroy@@YAXPEAX@Z : 1
- msvcp140!?__ExceptionPtrCopy@@YAXPEAXPEBX@Z : 1
- msvcp140!?__ExceptionPtrAssign@@YAXPEAXPEBX@Z : 1
- msvcp140!??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEAJ@Z : 1
- msvcp140!?_Xlength_error@std@@YAXPBD@Z : 1

## Détail par logiciel

### 7zip-bin_5.2.0

- 32 bits : 1 fichiers (ex. package/win/ia32/7za.exe)
- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### _ffmpeg-installer_win32-x64_4.1.0

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### _img_sharp-win32-x64_0.35.5

- DLL absentes : aucune
- DLL absentes (différées) : node.exe
- Fonctions manquantes par DLL : aucune

### dotnetcore2

- 32 bits : 1 fichiers (ex. dotnetcore2/bin/shared/Microsoft.NETCore.App/3.1.23/mscorlib.dll)
- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### electron-winstaller

- 32 bits : 19 fichiers (ex. package/vendor/wconsole.dll, package/vendor/winterop.dll, package/vendor/7z-arm64.exe)
- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### esbuild

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### imageio_ffmpeg

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### jdk4py

- DLL absentes : mswsock.dll (1)
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### nodejs

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### numpy

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### panda3d

- DLL absentes : foundation.dll (45), openmaya.dll (45), openmayaanim.dll (45)
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

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
- Fonctions manquantes par DLL : msvcp140 140

### pywin32

- DLL absentes : mfc140u.dll (4), mswsock.dll (1)
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### qt6

- DLL absentes : icuuc.dll (1), qt6pdfquick.dll (1), oci.dll (1), libpq.dll (1), fbclient.dll (1), mimapi64.dll (1), qt6pdf.dll (1), qt6webenginewidgets.dll (1), qt6webenginequick.dll (1), qt6webenginecore.dll (1)
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### raylib

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : aucune

### wxpython

- DLL absentes : aucune
- DLL absentes (différées) : aucune
- Fonctions manquantes par DLL : msvcp140 114

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


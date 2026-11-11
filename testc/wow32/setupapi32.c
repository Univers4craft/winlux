// setupapi en x86 (#1656) : les structures de setupapi.h sont en pack(1) et
// leurs champs ULONG_PTR font 4 octets. SP_DEVINFO_DATA et
// SP_DEVICE_INTERFACE_DATA font 28 octets, INFCONTEXT 16 octets,
// SP_DEVINSTALL_PARAMS_W 556 octets. Chaque structure est suivie d'une
// garde : Winlux ne doit jamais écrire au-delà (FInstallerHelperDll.dll d'iCUE
// plantait sur le contrôle /GS après SetupDiEnumDeviceInterfaces).
// Lancé par scripts/test-setupapi32.sh avec un faux sysfs (un port série).
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned int u32;
typedef unsigned char u8;
typedef unsigned short u16;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) u32 __stdcall GetLastError(void);
__declspec(dllimport) void *__stdcall SetupDiGetClassDevsW(const u8 *, const u16 *, void *, u32);
__declspec(dllimport) int __stdcall SetupDiEnumDeviceInfo(void *, u32, void *);
__declspec(dllimport) int __stdcall SetupDiEnumDeviceInterfaces(void *, void *, const u8 *, u32, void *);
__declspec(dllimport) int __stdcall SetupDiGetDeviceInterfaceDetailW(void *, void *, void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall SetupDiGetDeviceInstanceIdW(void *, void *, u16 *, u32, u32 *);
__declspec(dllimport) int __stdcall SetupDiDestroyDeviceInfoList(void *);
__declspec(dllimport) int __stdcall SetupDiGetDeviceInstallParamsW(void *, void *, void *);
__declspec(dllimport) void *__stdcall SetupOpenInfFileW(const u16 *, const u16 *, u32, u32 *);
__declspec(dllimport) int __stdcall SetupFindFirstLineW(void *, const u16 *, const u16 *, void *);
__declspec(dllimport) int __stdcall SetupGetStringFieldW(void *, u32, u16 *, u32, u32 *);
__declspec(dllimport) void __stdcall SetupCloseInfFile(void *);

#define INVALID ((void *)~0u)
#define GUARD 0xA5A5A5A5u

// GUID_DEVINTERFACE_COMPORT {86E0D1E0-8089-11D0-9CE4-08003E301F73}.
static u8 comport[16] = {0xe0, 0xd1, 0xe0, 0x86, 0x89, 0x80, 0xd0, 0x11,
                         0x9c, 0xe4, 0x08, 0x00, 0x3e, 0x30, 0x1f, 0x73};
static u16 inf[] = L"essai32.inf";
static u16 version[] = L"Version";
static u16 class_key[] = L"Class";

static struct { u32 size; u8 guid[16]; u32 devinst; u32 reserved; u32 guard; } dev, dev2;
static struct { u32 size; u8 guid[16]; u32 flags; u32 reserved; u32 guard; } itf;
static struct { u8 ctx[16]; u32 guard; } ctx;
static struct { u32 size; u8 rest[552]; u32 guard; } dip;
static u8 detail[600];
static u16 text[300];

void entry(void) {
    u32 need = 0;
    dev.guard = dev2.guard = itf.guard = ctx.guard = dip.guard = GUARD;

    // 1. Appareils présents : SP_DEVINFO_DATA de 28 octets, garde intacte.
    void *all = SetupDiGetClassDevsW(0, 0, 0, 0x6); // DIGCF_PRESENT | DIGCF_ALLCLASSES
    if (all == INVALID) ExitProcess(1);
    dev.size = 28;
    if (!SetupDiEnumDeviceInfo(all, 0, &dev) || dev.guard != GUARD || dev.reserved != 1) ExitProcess(1);
    if (!SetupDiGetDeviceInstanceIdW(all, &dev, text, 300, &need) || !text[0]) ExitProcess(1);
    SetupDiDestroyDeviceInfoList(all);

    // 2. Interface COMPORT : SP_DEVICE_INTERFACE_DATA de 28 octets, puis le
    //    détail (cbSize 6 en x86) qui rend aussi le SP_DEVINFO_DATA.
    void *ports = SetupDiGetClassDevsW(comport, 0, 0, 0x12); // DIGCF_PRESENT | DIGCF_DEVICEINTERFACE
    if (ports == INVALID) ExitProcess(2);
    itf.size = 28;
    if (!SetupDiEnumDeviceInterfaces(ports, 0, comport, 0, &itf) || itf.guard != GUARD || itf.flags != 1 ||
        itf.reserved != 1)
        ExitProcess(2);
    if (SetupDiEnumDeviceInterfaces(ports, 0, comport, 1, &itf) || GetLastError() != 259 || itf.guard != GUARD)
        ExitProcess(2);
    if (!SetupDiEnumDeviceInterfaces(ports, 0, comport, 0, &itf)) ExitProcess(2);
    *(u32 *)detail = 6;
    dev2.size = 28;
    if (!SetupDiGetDeviceInterfaceDetailW(ports, &itf, detail, sizeof detail, &need, &dev2) || dev2.guard != GUARD ||
        dev2.reserved != 1 || ((u16 *)(detail + 4))[0] != '\\')
        ExitProcess(2);
    SetupDiDestroyDeviceInfoList(ports);

    // 3. INFCONTEXT de 16 octets.
    void *h = SetupOpenInfFileW(inf, 0, 2, 0);
    if (h == INVALID || !SetupFindFirstLineW(h, version, class_key, &ctx) || ctx.guard != GUARD) ExitProcess(3);
    if (!SetupGetStringFieldW(&ctx, 1, text, 300, &need) || need != 6 || text[0] != 'P') ExitProcess(3);
    SetupCloseInfFile(h);

    // 4. SP_DEVINSTALL_PARAMS_W : 556 octets en x86 ; la taille 64 bits est refusée.
    dip.size = 556;
    if (!SetupDiGetDeviceInstallParamsW(0, 0, &dip) || dip.guard != GUARD) ExitProcess(4);
    dip.size = 584;
    if (SetupDiGetDeviceInstallParamsW(0, 0, &dip) || GetLastError() != 1784) ExitProcess(4);
    ExitProcess(0);
}

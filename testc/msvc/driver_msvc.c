// Installation virtualisée d'un pilote constructeur (#1532), au format MSVC
// (clang en mode x86_64-pc-windows-msvc, sans CRT) : le parcours d'un
// installeur (SetupCopyOEMInf, DiInstallDriver, UpdateDriverForPlugAndPlay-
// Devices, service SERVICE_KERNEL_DRIVER) face à Winlux, qui ne charge jamais
// le .sys. Lancé par scripts/test-drivers.sh avec WINLUX_VDEV_ESSAI=1 : le
// pilote d'essai winluxessai.sys a une traduction (appareil virtuel
// « WinluxEssaiPilote »), inconnu.sys n'en a pas.
// Depuis #1605, l'étape 8 crée en plus l'appareil racine comme devcon install.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned char u8;
typedef unsigned short u16;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall CreateFileW(const u16 *, u32, u32, void *, u32, u32, void *);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) u32 __stdcall GetLastError(void);
__declspec(dllimport) int __stdcall DeviceIoControl(void *, u32, void *, u32, void *, u32, u32 *, void *);

__declspec(dllimport) int __stdcall SetupCopyOEMInfW(const u16 *, const u16 *, u32, u32, u16 *, u32, u32 *, u16 **);
__declspec(dllimport) int __stdcall SetupUninstallOEMInfW(const u16 *, u32, void *);
__declspec(dllimport) int __stdcall DiInstallDriverW(void *, const u16 *, u32, int *);
__declspec(dllimport) int __stdcall UpdateDriverForPlugAndPlayDevicesW(void *, const u16 *, const u16 *, u32, int *);
__declspec(dllimport) void *__stdcall SetupDiCreateDeviceInfoList(const u8 *, void *);
__declspec(dllimport) int __stdcall SetupDiCreateDeviceInfoW(void *, const u16 *, const u8 *, const u16 *, void *, u32, void *);
__declspec(dllimport) int __stdcall SetupDiSetDeviceRegistryPropertyW(void *, void *, u32, const void *, u32);
__declspec(dllimport) int __stdcall SetupDiCallClassInstaller(u32, void *, void *);
__declspec(dllimport) int __stdcall SetupDiDestroyDeviceInfoList(void *);
__declspec(dllimport) u32 __stdcall CMP_WaitNoPendingInstallEvents(u32);
__declspec(dllimport) u32 __stdcall CM_Locate_DevNodeW(u32 *, const u16 *, u32);
__declspec(dllimport) u32 __stdcall CM_Request_Device_EjectW(u32, u32 *, u16 *, u32, u32);

__declspec(dllimport) void *__stdcall OpenSCManagerW(const u16 *, const u16 *, u32);
__declspec(dllimport) void *__stdcall OpenServiceW(void *, const u16 *, u32);
__declspec(dllimport) void *__stdcall CreateServiceW(void *, const u16 *, const u16 *, u32, u32, u32, u32, const u16 *,
                                                     const u16 *, u32 *, const u16 *, const u16 *, const u16 *);
__declspec(dllimport) int __stdcall StartServiceW(void *, u32, const u16 **);
__declspec(dllimport) int __stdcall ControlService(void *, u32, void *);
__declspec(dllimport) int __stdcall QueryServiceStatus(void *, void *);
__declspec(dllimport) int __stdcall DeleteService(void *);
__declspec(dllimport) int __stdcall CloseServiceHandle(void *);

typedef struct { u32 type, state, accepted, exit_code, specific, checkpoint, wait; } Status;
typedef struct { u32 size; u8 guid[16]; u32 devinst; u64 reserved; } DevInfo;

#define INVALID ((void *)~0ull)

static u16 inf[] = L"essai.inf";
static u16 device[] = L"\\\\.\\WinluxEssaiPilote";
static u16 service[] = L"WinluxEssai";
static u16 unknown[] = L"WinluxInconnu";
static u16 unknown_sys[] = L"C:\\Windows\\System32\\drivers\\inconnu.sys";
static u16 absent_hw[] = L"USB\\VID_FFFE&PID_FFFE";
static u16 root_name[] = L"WinluxEssai";
static u16 root_desc[] = L"Clavier d'essai";
static u16 root_hw[] = L"ROOT\\WinluxEssai\0";
static u16 root_id[] = L"ROOT\\WINLUXESSAI\\0000";
// GUID_DEVCLASS_HIDCLASS {745a17a0-74d3-11d0-b6fe-00a0c90f57da}.
static u8 hid_class[16] = {0xa0, 0x17, 0x5a, 0x74, 0xd3, 0x74, 0xd0, 0x11,
                           0xb6, 0xfe, 0x00, 0xa0, 0xc9, 0x0f, 0x57, 0xda};
static u16 out[260], again[260];

static int starts(const u16 *s, const char *w) {
    for (; *w; s++, w++)
        if ((*s | 0x20) != (u16)(*w | 0x20)) return 0;
    return 1;
}

static int same(const u16 *a, const u16 *b) {
    for (; *a == *b; a++, b++)
        if (!*a) return 1;
    return 0;
}

static void *open_device(void) { return CreateFileW(device, 0xC0000000, 3, 0, 3, 0, 0); }

void entry(void) {
    u32 need = 0;
    u16 *file = 0;
    int reboot = -1;
    Status st;

    // 1. SetupCopyOEMInf : C:\Windows\INF\oemN.inf, composant « oemN.inf » ;
    //    le même contenu garde son nom ; SP_COPY_NOOVERWRITE refuse alors.
    if (!SetupCopyOEMInfW(inf, 0, 1, 0, out, 260, &need, &file) || !starts(out, "C:\\Windows\\INF\\oem") ||
        !file || !starts(file, "oem") || need < 20)
        ExitProcess(1);
    if (!SetupCopyOEMInfW(inf, 0, 1, 0, again, 260, 0, 0) || !same(out, again)) ExitProcess(1);
    if (SetupCopyOEMInfW(inf, 0, 1, 8, again, 260, 0, 0) || GetLastError() != 80) ExitProcess(1);
    if (SetupCopyOEMInfW(inf, 0, 1, 0, again, 5, &need, 0) || GetLastError() != 122) ExitProcess(1);

    // 2. UpdateDriverForPlugAndPlayDevices sans appareil présent.
    if (UpdateDriverForPlugAndPlayDevicesW(0, absent_hw, inf, 0, &reboot) || GetLastError() != 0xE000020B)
        ExitProcess(2);

    // 3. DiInstallDriver : services du .inf inscrits, pas de redémarrage.
    if (!DiInstallDriverW(0, inf, 0, &reboot) || reboot != 0) ExitProcess(3);
    void *scm = OpenSCManagerW(0, 0, 0xF003F);
    if (!scm) ExitProcess(3);
    void *svc = OpenServiceW(scm, service, 0xF01FF);
    if (!svc || !QueryServiceStatus(svc, &st) || st.type != 1 || st.state != 1) ExitProcess(3);

    // 4. Arrêté : l'appareil virtuel n'existe pas encore.
    if (open_device() != INVALID || GetLastError() != 2) ExitProcess(4);

    // 5. StartService : l'appareil virtuel prend la place du .sys.
    if (!StartServiceW(svc, 0, 0)) ExitProcess(5);
    if (!QueryServiceStatus(svc, &st) || st.state != 4) ExitProcess(5);
    void *h = open_device();
    u8 buf[16];
    u32 n = 0;
    if (h == INVALID) ExitProcess(5);
    if (!DeviceIoControl(h, (0x8000u << 16) | (0x800 << 2), buf, 3, buf, sizeof buf, &n, 0) || n != 8 || buf[2] != 1)
        ExitProcess(5);
    CloseHandle(h);
    if (StartServiceW(svc, 0, 0) || GetLastError() != 1056) ExitProcess(5);

    // 6. Arrêt : l'appareil disparaît.
    if (!ControlService(svc, 1, &st) || st.state != 1) ExitProcess(6);
    if (open_device() != INVALID || GetLastError() != 2) ExitProcess(6);

    // 7. Pilote sans traduction : inscrit, jamais démarré, ERROR_DRIVER_BLOCKED.
    void *other = CreateServiceW(scm, unknown, unknown, 0xF01FF, 1, 3, 1, unknown_sys, 0, 0, 0, 0, 0);
    if (!other) ExitProcess(7);
    if (StartServiceW(other, 0, 0) || GetLastError() != 1275) ExitProcess(7);
    if (!QueryServiceStatus(other, &st) || st.state != 1 || st.exit_code != 1275) ExitProcess(7);

    // 8. Appareil racine créé par l'installeur (#1605), comme devcon
    //    install : l'ensemble, l'identifiant matériel, DIF_REGISTERDEVICE,
    //    puis UpdateDriverForPlugAndPlayDevices trouve l'appareil et démarre
    //    le service ; CM_Request_Device_EjectW le retire.
    if (CMP_WaitNoPendingInstallEvents(5000) != 0) ExitProcess(8);
    void *set = SetupDiCreateDeviceInfoList(hid_class, 0);
    DevInfo dd = {sizeof(DevInfo)};
    if (set == INVALID || !SetupDiCreateDeviceInfoW(set, root_name, hid_class, root_desc, 0, 1, &dd) || !dd.devinst)
        ExitProcess(8);
    if (!SetupDiSetDeviceRegistryPropertyW(set, &dd, 1, root_hw, sizeof root_hw)) ExitProcess(8);
    u32 dn = 0, veto = 9;
    if (CM_Locate_DevNodeW(&dn, root_id, 0) != 0x0D) ExitProcess(8); // pas encore inscrit
    if (!SetupDiCallClassInstaller(0x19, set, &dd)) ExitProcess(8);
    if (CM_Locate_DevNodeW(&dn, root_id, 0) != 0 || dn != dd.devinst) ExitProcess(8);
    reboot = -1;
    if (!UpdateDriverForPlugAndPlayDevicesW(0, root_hw, inf, 0, &reboot) || reboot != 0) ExitProcess(8);
    if (!QueryServiceStatus(svc, &st) || st.state != 4) ExitProcess(8);
    h = open_device();
    if (h == INVALID) ExitProcess(8);
    CloseHandle(h);
    if (!ControlService(svc, 1, &st) || st.state != 1) ExitProcess(8);
    if (CM_Request_Device_EjectW(dn, &veto, 0, 0, 0) != 0 || veto != 0) ExitProcess(8);
    if (CM_Locate_DevNodeW(&dn, root_id, 0) != 0x0D) ExitProcess(8);
    if (UpdateDriverForPlugAndPlayDevicesW(0, root_hw, inf, 0, &reboot) || GetLastError() != 0xE000020B) ExitProcess(8);
    SetupDiDestroyDeviceInfoList(set);

    // 9. Nettoyage : services supprimés, .inf retiré du magasin.
    if (!DeleteService(other) || !DeleteService(svc)) ExitProcess(9);
    CloseServiceHandle(other);
    CloseServiceHandle(svc);
    CloseServiceHandle(scm);
    if (!SetupUninstallOEMInfW(file, 0, 0)) ExitProcess(9);
    if (SetupUninstallOEMInfW(file, 0, 0) || GetLastError() != 2) ExitProcess(9);
    ExitProcess(0);
}

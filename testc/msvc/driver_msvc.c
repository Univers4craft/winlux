// Installation virtualisée d'un pilote constructeur (#1532), au format MSVC
// (clang en mode x86_64-pc-windows-msvc, sans CRT) : le parcours d'un
// installeur (SetupCopyOEMInf, DiInstallDriver, UpdateDriverForPlugAndPlay-
// Devices, service SERVICE_KERNEL_DRIVER) face à Winlux, qui ne charge jamais
// le .sys. Lancé par scripts/test-drivers.sh avec WINLUX_VDEV_ESSAI=1 : le
// pilote d'essai winluxessai.sys a une traduction (appareil virtuel
// « WinluxEssaiPilote »), inconnu.sys n'en a pas.
// Depuis #1605, l'étape 8 crée en plus l'appareil racine comme devcon install ;
// depuis #1625, l'étape 9 rejoue le parcours d'un outil d'installation.
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
// Parcours d'un outil d'installation de pilote (#1625).
__declspec(dllimport) u32 __stdcall CM_Connect_MachineW(const u16 *, void **);
__declspec(dllimport) u32 __stdcall CM_Disconnect_Machine(void *);
__declspec(dllimport) u32 __stdcall CM_Locate_DevNode_ExW(u32 *, const u16 *, u32, void *);
__declspec(dllimport) u32 __stdcall CM_Get_DevNode_Status_Ex(u32 *, u32 *, u32, u32, void *);
__declspec(dllimport) u32 __stdcall CM_Get_First_Log_Conf_Ex(void **, u32, u32, void *);
__declspec(dllimport) void *__stdcall SetupOpenInfFileW(const u16 *, const u16 *, u32, u32 *);
__declspec(dllimport) void __stdcall SetupCloseInfFile(void *);
__declspec(dllimport) int __stdcall SetupFindFirstLineW(void *, const u16 *, const u16 *, void *);
__declspec(dllimport) int __stdcall SetupGetStringFieldW(void *, u32, u16 *, u32, u32 *);
__declspec(dllimport) void *__stdcall SetupOpenFileQueue(void);
__declspec(dllimport) int __stdcall SetupScanFileQueueW(void *, u32, void *, void *, void *, u32 *);
__declspec(dllimport) int __stdcall SetupCloseFileQueue(void *);
__declspec(dllimport) int __stdcall SetupDiGetINFClassW(const u16 *, u8 *, u16 *, u32, u32 *);
__declspec(dllimport) int __stdcall SetupDiClassNameFromGuidExW(const u8 *, u16 *, u32, u32 *, const u16 *, void *);
__declspec(dllimport) void *__stdcall SetupDiGetClassDevsExW(const u8 *, const u16 *, void *, u32, void *, const u16 *, void *);
__declspec(dllimport) int __stdcall SetupDiGetDeviceInstallParamsW(void *, void *, void *);
__declspec(dllimport) int __stdcall SetupDiSetDeviceInstallParamsW(void *, void *, void *);
__declspec(dllimport) int __stdcall SetupDiBuildDriverInfoList(void *, void *, u32);
__declspec(dllimport) int __stdcall SetupDiEnumDriverInfoW(void *, void *, u32, u32, void *);
__declspec(dllimport) int __stdcall SetupDiGetDriverInfoDetailW(void *, void *, void *, void *, u32, u32 *);
__declspec(dllimport) int __stdcall SetupDiSetSelectedDriverW(void *, void *, void *);
__declspec(dllimport) int __stdcall SetupDiDestroyDriverInfoList(void *, void *, u32);
__declspec(dllimport) int __stdcall DiUninstallDevice(void *, void *, void *, u32, int *);

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
static u16 version[] = L"Version";
static u16 class_key[] = L"Class";
static u16 usb[] = L"USB";
static u16 remote_pc[] = L"\\\\autre-pc";
static u16 out[260], again[260], text[300];
// SP_DEVINSTALL_PARAMS_W (584 o), SP_DRVINFO_DATA_V2_W (1568 o),
// SP_DRVINFO_DETAIL_DATA_W (1584 o + identifiants) : en .bss, déjà à zéro.
static u8 dip[584], drv[1568], detail[2048];

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

static int same_text(const u16 *a, const char *b) {
    for (; *a == (u16)*b; a++, b++)
        if (!*b) return 1;
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

    // 9. Parcours d'un outil d'installation de pilote (#1625, comme
    //    driverinstalltool_64 d'iCUE) : machine locale, lecture du .inf,
    //    liste des pilotes compatibles, file de copie vide, désinstallation.
    void *machine = 0, *lc = (void *)1;
    u32 status = 0, problem = 9, line = 7;
    if (CM_Connect_MachineW(0, &machine) != 0 || !machine) ExitProcess(9);
    if (CM_Connect_MachineW(remote_pc, &lc) != 0x30) ExitProcess(9);
    lc = (void *)1;
    if (CM_Locate_DevNode_ExW(&dn, 0, 0, machine) != 0 || !dn) ExitProcess(9);
    if (CM_Get_DevNode_Status_Ex(&status, &problem, dn, 0, machine) != 0 || problem != 0) ExitProcess(9);
    if (CM_Get_First_Log_Conf_Ex(&lc, dn, 0, machine) != 0x0E || lc) ExitProcess(9);
    if (CM_Disconnect_Machine(machine) != 0) ExitProcess(9);
    void *hinf = SetupOpenInfFileW(inf, 0, 2, &line);
    u8 ctx[24];
    if (hinf == INVALID || line != 0) ExitProcess(9);
    if (!SetupFindFirstLineW(hinf, version, class_key, ctx)) ExitProcess(9);
    if (!SetupGetStringFieldW(ctx, 1, text, 300, &need) || need != 9 || !same_text(text, "HIDClass")) ExitProcess(9);
    SetupCloseInfFile(hinf);
    if (SetupOpenInfFileW(inf, usb, 2, 0) != INVALID || GetLastError() != 0xE0000201) ExitProcess(9);
    u8 g[16];
    if (!SetupDiGetINFClassW(inf, g, text, 300, 0) || !same_text(text, "HIDClass") || g[0] != 0xa0 || g[15] != 0xda)
        ExitProcess(9);
    if (!SetupDiClassNameFromGuidExW(hid_class, text, 300, 0, 0, 0) || !same_text(text, "HIDClass")) ExitProcess(9);
    void *all = SetupDiGetClassDevsExW(hid_class, 0, 0, 2, 0, 0, 0);
    if (all == INVALID) ExitProcess(9);
    SetupDiDestroyDeviceInfoList(all);
    if (SetupDiGetClassDevsExW(hid_class, 0, 0, 2, 0, remote_pc, 0) != INVALID || GetLastError() != 0xE0000220)
        ExitProcess(9);

    set = SetupDiCreateDeviceInfoList(hid_class, 0);
    DevInfo dv = {sizeof(DevInfo)};
    if (!SetupDiCreateDeviceInfoW(set, root_name, hid_class, root_desc, 0, 1, &dv)) ExitProcess(9);
    if (!SetupDiSetDeviceRegistryPropertyW(set, &dv, 1, root_hw, sizeof root_hw)) ExitProcess(9);
    if (!SetupDiCallClassInstaller(0x19, set, &dv)) ExitProcess(9);
    *(u32 *)dip = 584;
    if (!SetupDiGetDeviceInstallParamsW(set, &dv, dip)) ExitProcess(9);
    *(u32 *)(dip + 4) |= 0x10000; // DI_ENUMSINGLEINF
    for (int i = 0; inf[i]; i++) ((u16 *)(dip + 60))[i] = inf[i];
    if (!SetupDiSetDeviceInstallParamsW(set, &dv, dip)) ExitProcess(9);
    if (!SetupDiBuildDriverInfoList(set, &dv, 2)) ExitProcess(9);
    *(u32 *)drv = 1568;
    if (!SetupDiEnumDriverInfoW(set, &dv, 2, 0, drv) || !same_text((u16 *)(drv + 16), "Clavier d'essai") ||
        !same_text((u16 *)(drv + 16 + 1024), "Winlux Essais") || *(u64 *)(drv + 1560) != (3ull << 48 | 3ull << 32))
        ExitProcess(9);
    if (SetupDiEnumDriverInfoW(set, &dv, 2, 1, drv) || GetLastError() != 259) ExitProcess(9);
    if (SetupDiGetDriverInfoDetailW(set, &dv, drv, 0, 0, &need) || GetLastError() != 122 || need <= 1576)
        ExitProcess(9);
    *(u32 *)detail = 1584;
    if (!SetupDiGetDriverInfoDetailW(set, &dv, drv, detail, sizeof detail, 0) ||
        !same_text((u16 *)(detail + 32), "Essai") || !same_text((u16 *)(detail + 1576), "ROOT\\WinluxEssai"))
        ExitProcess(9);
    if (!SetupDiSetSelectedDriverW(set, &dv, drv) || !SetupDiDestroyDriverInfoList(set, &dv, 2)) ExitProcess(9);
    void *queue = SetupOpenFileQueue();
    u32 scan = 5;
    if (!queue || queue == INVALID || !SetupScanFileQueueW(queue, 4, 0, 0, 0, &scan) || scan != 0 ||
        !SetupCloseFileQueue(queue))
        ExitProcess(9);
    reboot = -1;
    if (!DiUninstallDevice(0, set, &dv, 0, &reboot) || reboot != 0) ExitProcess(9);
    if (CM_Locate_DevNodeW(&dn, root_id, 0) != 0x0D) ExitProcess(9);
    SetupDiDestroyDeviceInfoList(set);

    // 10. Nettoyage : services supprimés, .inf retiré du magasin.
    if (!DeleteService(other) || !DeleteService(svc)) ExitProcess(10);
    CloseServiceHandle(other);
    CloseServiceHandle(svc);
    CloseServiceHandle(scm);
    if (!SetupUninstallOEMInfW(file, 0, 0)) ExitProcess(10);
    if (SetupUninstallOEMInfW(file, 0, 0) || GetLastError() != 2) ExitProcess(10);
    ExitProcess(0);
}

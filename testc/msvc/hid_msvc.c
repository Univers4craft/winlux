// Manettes et périphériques HID (#271), au format MSVC (clang en mode
// x86_64-pc-windows-msvc, sans CRT) : le parcours que suivent SDL, les
// moteurs de jeu et les outils de configuration de manettes.
//   setupapi : GUID_DEVINTERFACE_HID -> chemin d'interface ;
//   CreateFileW sur ce chemin, puis hid.dll : attributs, nom, capacités ;
//   WriteFile / ReadFile en OVERLAPPED avec l'octet d'identifiant de
//   rapport (0) que Windows ajoute et que hidraw omet.
// Le faux appareil est une FIFO (scripts/test-hid.sh) : ce qui est écrit
// revient en lecture, ce qui vérifie les deux sens du relais.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall CreateFileW(const u16 *, u32, u32, void *, u32, u32, void *);
__declspec(dllimport) int __stdcall ReadFile(void *, void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall WriteFile(void *, const void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall GetOverlappedResult(void *, void *, u32 *, int);
__declspec(dllimport) void *__stdcall CreateEventW(void *, int, int, const u16 *);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) u32 __stdcall GetLastError(void);

__declspec(dllimport) void *__stdcall SetupDiGetClassDevsW(const void *, const u16 *, void *, u32);
__declspec(dllimport) int __stdcall SetupDiEnumDeviceInterfaces(void *, void *, const void *, u32, void *);
__declspec(dllimport) int __stdcall SetupDiGetDeviceInterfaceDetailW(void *, void *, void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall SetupDiGetDeviceRegistryPropertyW(void *, void *, u32, u32 *, void *, u32, u32 *);
__declspec(dllimport) int __stdcall SetupDiDestroyDeviceInfoList(void *);

__declspec(dllimport) void __stdcall HidD_GetHidGuid(void *);
__declspec(dllimport) u8 __stdcall HidD_GetAttributes(void *, void *);
__declspec(dllimport) u8 __stdcall HidD_GetProductString(void *, void *, u32);
__declspec(dllimport) u8 __stdcall HidD_GetPreparsedData(void *, void **);
__declspec(dllimport) u8 __stdcall HidD_FreePreparsedData(void *);
__declspec(dllimport) u32 __stdcall HidP_GetCaps(void *, void *);

typedef struct { u32 cb; u8 guid[16]; u32 flags; u64 reserved; } IfaceData;
typedef struct { u32 cb; u8 guid[16]; u32 inst; u64 reserved; } DevInfo;
typedef struct { u32 size; u16 vendor, product, version; } Attributes;
typedef struct { u16 usage, page, in_len, out_len, feat_len, reserved[17], nodes, in_buttons, in_values, rest[7]; } Caps;
typedef struct { u64 internal, internal_high; u32 offset, offset_high; void *event; } Overlapped;

void *memset(void *d, int c, u64 n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}

static int wcontains(const u16 *s, const char *w) {
    for (; *s; s++) {
        const u16 *a = s;
        const char *b = w;
        while (*b && (*a | 0x20) == (u16)(*b | 0x20)) a++, b++;
        if (!*b) return 1;
    }
    return 0;
}

static void *INVALID = (void *)~0ull;
static u8 detail[1024];
static u16 text[256];

int entry() {
    u8 hid_guid[16], other[16] = {1, 2, 3};
    HidD_GetHidGuid(hid_guid);
    // 1. Le GUID HID est bien {4D1E55B2-F16F-11CF-88CB-001111000030}.
    if (hid_guid[0] != 0xB2 || hid_guid[3] != 0x4D || hid_guid[15] != 0x30) ExitProcess(1);
    // 2. Une autre classe : pas de liste (comme avant, pour Electron).
    if (SetupDiGetClassDevsW(other, 0, 0, 0x12) != INVALID) ExitProcess(2);
    // 3. Une seule interface HID présente.
    void *set = SetupDiGetClassDevsW(hid_guid, 0, 0, 0x12); // PRESENT | DEVICEINTERFACE
    if (set == INVALID) ExitProcess(3);
    IfaceData iface = {sizeof iface};
    if (!SetupDiEnumDeviceInterfaces(set, 0, hid_guid, 0, &iface)) ExitProcess(3);
    IfaceData none = {sizeof none};
    if (SetupDiEnumDeviceInterfaces(set, 0, hid_guid, 1, &none) || GetLastError() != 259) ExitProcess(3);
    // 4. Détail en deux appels : taille demandée, puis chemin.
    u32 need = 0;
    if (SetupDiGetDeviceInterfaceDetailW(set, &iface, 0, 0, &need, 0) || GetLastError() != 122 || need < 40)
        ExitProcess(4);
    *(u32 *)detail = 8;
    DevInfo info = {sizeof info};
    if (!SetupDiGetDeviceInterfaceDetailW(set, &iface, detail, sizeof detail, 0, &info)) ExitProcess(4);
    const u16 *path = (const u16 *)(detail + 4);
    if (!wcontains(path, "vid_045e&pid_028e") || !wcontains(path, "{4d1e55b2-")) ExitProcess(4);
    // 5. Identifiants matériels (MULTI_SZ).
    u32 type = 0;
    if (!SetupDiGetDeviceRegistryPropertyW(set, &info, 1, &type, text, sizeof text, 0) || type != 7 ||
        !wcontains(text, "HID\\VID_045E&PID_028E"))
        ExitProcess(5);
    SetupDiDestroyDeviceInfoList(set);
    // 6. Ouverture asynchrone du chemin d'interface.
    void *h = CreateFileW(path, 0xC0000000, 3, 0, 3, 0x40000000, 0);
    if (h == INVALID) ExitProcess(6);
    // 7. Attributs et nom.
    Attributes at = {sizeof at};
    if (!HidD_GetAttributes(h, &at) || at.vendor != 0x045E || at.product != 0x028E)
        ExitProcess(7);
    if (!HidD_GetProductString(h, text, sizeof text) || !wcontains(text, "Manette test")) ExitProcess(7);
    // 8. Capacités tirées du descripteur : manette, rapports de 7 et 3 octets.
    void *pre = 0;
    Caps caps;
    if (!HidD_GetPreparsedData(h, &pre) || HidP_GetCaps(pre, &caps) != 0x00110000) ExitProcess(8);
    if (caps.page != 1 || caps.usage != 5 || caps.in_len != 7 || caps.out_len != 3 || caps.in_buttons != 1)
        ExitProcess(8);
    HidD_FreePreparsedData(pre);
    // 9. Rapport de sortie (vibrations) : identifiant 0 puis deux octets.
    Overlapped ov;
    memset(&ov, 0, sizeof ov);
    ov.event = CreateEventW(0, 1, 0, 0);
    u8 out[3] = {0, 0x11, 0x22};
    u32 done = 0;
    if (!WriteFile(h, out, 3, &done, &ov) && GetLastError() != 997) ExitProcess(9);
    if (!GetOverlappedResult(h, &ov, &done, 1) || done != 3) ExitProcess(9);
    // 10. Rapport d'entrée : l'appareil reçoit 11 22 sans l'identifiant, et
    //     la lecture rend 00 11 22, comme sous Windows.
    u8 in[7] = {9, 9, 9, 9, 9, 9, 9};
    memset(&ov, 0, sizeof ov - sizeof ov.event);
    done = 0;
    if (!ReadFile(h, in, 7, &done, &ov) && GetLastError() != 997) ExitProcess(10);
    if (!GetOverlappedResult(h, &ov, &done, 1)) ExitProcess(10);
    if (done != 3 || in[0] != 0 || in[1] != 0x11 || in[2] != 0x22) ExitProcess(10);
    CloseHandle(ov.event);
    // 11. Fermeture : le handle n'est plus un périphérique HID.
    if (!CloseHandle(h)) ExitProcess(11);
    ExitProcess(0);
    return 0;
}

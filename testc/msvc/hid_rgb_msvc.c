// Logiciel d'éclairage type (#1523), au format MSVC (clang en mode
// x86_64-pc-windows-msvc, sans CRT) : le parcours de hidapi et des
// logiciels constructeurs face à un clavier RVB à deux collections de tête
// (clavier + interface fabricant 0xFF00), faux appareil de
// scripts/test-hid-rgb.sh (sysfs factice d'un appareil USB, FIFO).
//   cfgmgr32 : liste des interfaces HID, identifiant d'instance (&MI_, &COL) ;
//   ouverture sans droit d'accès pour lire les attributs (hidraw1 absent) ;
//   chaînes USB, capacités par collection, trame de couleurs de 32 octets
//   écrite 120 fois (cadence d'une animation), touches lues et décodées.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall CreateFileW(const u16 *, u32, u32, void *, u32, u32, void *);
__declspec(dllimport) int __stdcall ReadFile(void *, void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall WriteFile(void *, const void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) u32 __stdcall GetLastError(void);
__declspec(dllimport) u64 __stdcall GetTickCount64(void);

__declspec(dllimport) u32 __stdcall CM_Get_Device_Interface_List_SizeW(u32 *, const void *, const u16 *, u32);
__declspec(dllimport) u32 __stdcall CM_Get_Device_Interface_ListW(const void *, const u16 *, u16 *, u32, u32);
__declspec(dllimport) u32 __stdcall CM_Get_Device_Interface_PropertyW(const u16 *, const void *, u32 *, void *, u32 *, u32);
__declspec(dllimport) u32 __stdcall CM_Locate_DevNodeW(u32 *, const u16 *, u32);
__declspec(dllimport) u32 __stdcall CM_Get_DevNode_PropertyW(u32, const void *, u32 *, void *, u32 *, u32);

__declspec(dllimport) void __stdcall HidD_GetHidGuid(void *);
__declspec(dllimport) u8 __stdcall HidD_GetAttributes(void *, void *);
__declspec(dllimport) u8 __stdcall HidD_GetManufacturerString(void *, void *, u32);
__declspec(dllimport) u8 __stdcall HidD_GetProductString(void *, void *, u32);
__declspec(dllimport) u8 __stdcall HidD_GetSerialNumberString(void *, void *, u32);
__declspec(dllimport) u8 __stdcall HidD_GetIndexedString(void *, u32, void *, u32);
__declspec(dllimport) u8 __stdcall HidD_GetPreparsedData(void *, void **);
__declspec(dllimport) u8 __stdcall HidD_FreePreparsedData(void *);
__declspec(dllimport) u8 __stdcall HidD_GetFeature(void *, void *, u32);
__declspec(dllimport) u32 __stdcall HidP_GetCaps(void *, void *);
__declspec(dllimport) u32 __stdcall HidP_GetValueCaps(u32, void *, u16 *, void *);
__declspec(dllimport) u32 __stdcall HidP_GetUsages(u32, u16, u16, u16 *, u32 *, void *, void *, u32);
__declspec(dllimport) u32 __stdcall HidP_InitializeReportForID(u32, u8, void *, void *, u32);
__declspec(dllimport) u32 __stdcall HidP_SetUsageValueArray(u32, u16, u16, u16, void *, u16, void *, void *, u32);

typedef struct { u32 size; u16 vendor, product, version; } Attributes;
typedef struct { u16 usage, page, in_len, out_len, feat_len, reserved[17], nodes, in_buttons, in_values, rest[7]; } Caps;
typedef struct { u8 guid[16]; u32 pid; } PropKey;

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

static const u16 *find(const u16 *list, const char *w) {
    for (const u16 *p = list; *p;) {
        if (wcontains(p, w)) return p;
        while (*p) p++;
        p++;
    }
    return 0;
}

static void *INVALID = (void *)~0ull;
static u16 list[4096];
static u16 text[256];
static u8 value[512];
static u8 caps_buf[72 * 8];

// DEVPKEY_Device_InstanceId {78C34FC8-104A-4ACA-9EA4-524D52996E57}, 256.
static const PropKey INSTANCE = {{0xC8, 0x4F, 0xC3, 0x78, 0x4A, 0x10, 0xCA, 0x4A, 0x9E, 0xA4, 0x52, 0x4D, 0x52, 0x99, 0x6E, 0x57}, 256};
// DEVPKEY_Device_HardwareIds {A45C254E-DF1C-4EFD-8020-67D146A850E0}, 3.
static const PropKey HARDWARE = {{0x4E, 0x25, 0x5C, 0xA4, 0x1C, 0xDF, 0xFD, 0x4E, 0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0}, 3};

int entry() {
    u8 hid[16];
    HidD_GetHidGuid(hid);
    // 1. Trois interfaces : deux collections pour hidraw0, une pour hidraw1.
    u32 n = 0;
    if (CM_Get_Device_Interface_List_SizeW(&n, hid, 0, 0) || n > 4096) ExitProcess(1);
    if (CM_Get_Device_Interface_ListW(hid, 0, list, n, 0)) ExitProcess(1);
    const u16 *kbd = find(list, "&mi_02&col01#hidraw0");
    const u16 *rgb = find(list, "&mi_02&col02#hidraw0");
    const u16 *closed = find(list, "#hidraw1#");
    if (!kbd || !rgb || !closed) ExitProcess(1);
    // 2. Identifiant d'instance de l'interface, puis son nœud et ses
    //    identifiants matériels (ce que lit hidapi).
    u32 type = 0, len = sizeof text;
    if (CM_Get_Device_Interface_PropertyW(rgb, &INSTANCE, &type, text, &len, 0) || type != 0x12 ||
        !wcontains(text, "HID\\VID_1209&PID_0001&MI_02&COL02"))
        ExitProcess(20);
    u32 dn = 0;
    len = sizeof value;
    if (CM_Locate_DevNodeW(&dn, text, 0)) ExitProcess(21);
    if (CM_Get_DevNode_PropertyW(dn, &HARDWARE, &type, value, &len, 0) || type != 0x2012) ExitProcess(22);
    if (!find((u16 *)value, "HID_DEVICE_UP:FF00_U:0001")) ExitProcess(23);
    // 3. Ouverture sans droit d'accès d'un appareil fermé : attributs
    //    lisibles, rapports refusés ; avec accès en lecture : refusée.
    void *h = CreateFileW(closed, 0, 3, 0, 3, 0, 0);
    Attributes at = {sizeof at};
    if (h == INVALID || !HidD_GetAttributes(h, &at) || at.vendor != 0x1209 || at.product != 0x0002) ExitProcess(3);
    if (HidD_GetFeature(h, value, 65) || GetLastError() != 5) ExitProcess(3);
    CloseHandle(h);
    if (CreateFileW(closed, 0x80000000, 3, 0, 3, 0, 0) != INVALID) ExitProcess(3);
    // 4. Chaînes USB : fabricant, produit, série, et par indice.
    h = CreateFileW(rgb, 0xC0000000, 3, 0, 3, 0, 0);
    if (h == INVALID) ExitProcess(4);
    if (!HidD_GetManufacturerString(h, text, sizeof text) || !wcontains(text, "Winlux Essais")) ExitProcess(4);
    if (!HidD_GetProductString(h, text, sizeof text) || !wcontains(text, "Clavier RVB")) ExitProcess(4);
    if (!HidD_GetSerialNumberString(h, text, sizeof text) || !wcontains(text, "RVB0001")) ExitProcess(4);
    if (!HidD_GetIndexedString(h, 2, text, sizeof text) || !wcontains(text, "Clavier RVB")) ExitProcess(4);
    if (!HidD_GetAttributes(h, &at) || at.version != 0x0105) ExitProcess(4);
    // 5. Capacités de la collection fabricant : Output 33, Feature 65.
    void *pre = 0;
    Caps caps;
    if (!HidD_GetPreparsedData(h, &pre) || HidP_GetCaps(pre, &caps) != 0x00110000) ExitProcess(5);
    if (caps.page != 0xFF00 || caps.usage != 1 || caps.in_len != 0 || caps.out_len != 33 || caps.feat_len != 65)
        ExitProcess(5);
    u16 count = 8;
    if (HidP_GetValueCaps(1, caps_buf, &count, pre) != 0x00110000 || count != 1 || caps_buf[2] != 8 ||
        *(u16 *)(caps_buf + 20) != 32)
        ExitProcess(5);
    // 6. Animation : 120 trames de couleurs (une par touche), sans aplatir
    //    les octets, en moins de 2 s (au moins 60 trames par seconde).
    u8 frame[33];
    u64 start = GetTickCount64();
    for (u32 f = 0; f < 120; f++) {
        for (u32 k = 0; k < 32; k++) value[k] = (u8)(f + k);
        if (HidP_InitializeReportForID(1, 8, pre, frame, 33) != 0x00110000 ||
            HidP_SetUsageValueArray(1, 0xFF00, 0, 3, value, 32, pre, frame, 33) != 0x00110000 ||
            frame[0] != 8 || frame[1] != (u8)f || frame[32] != (u8)(f + 31))
            ExitProcess(6);
        u32 done = 0;
        if (!WriteFile(h, frame, 33, &done, 0) || done != 33) ExitProcess(6);
    }
    if (GetTickCount64() - start > 2000) ExitProcess(6);
    HidD_FreePreparsedData(pre);
    CloseHandle(h);
    // 7. Collection clavier : un rapport 1 (Maj gauche) revient en lecture
    //    et se décode ; les trames de couleurs (rapport 8) n'y passent pas.
    h = CreateFileW(kbd, 0xC0000000, 3, 0, 3, 0, 0);
    if (h == INVALID) ExitProcess(7);
    u8 key[2] = {1, 0x02};
    u32 done = 0;
    if (!WriteFile(h, key, 2, &done, 0) || done != 2) ExitProcess(7);
    u8 in[64];
    done = 0;
    if (!ReadFile(h, in, sizeof in, &done, 0) || done != 2 || in[0] != 1 || in[1] != 0x02) ExitProcess(7);
    if (!HidD_GetPreparsedData(h, &pre)) ExitProcess(7);
    u16 usages[8];
    u32 nu = 8;
    if (HidP_GetUsages(0, 7, 0, usages, &nu, pre, in, 2) != 0x00110000 || nu != 1 || usages[0] != 0xE1)
        ExitProcess(7);
    HidD_FreePreparsedData(pre);
    if (!CloseHandle(h)) ExitProcess(8);
    ExitProcess(0);
    return 0;
}

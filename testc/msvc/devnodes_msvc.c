// Arbre des périphériques (#271, partie 4), au format MSVC (clang en mode
// x86_64-pc-windows-msvc, sans CRT) : trouver un port COM comme pyserial, les
// logiciels Arduino ou de flashage, puis l'ouvrir par son chemin d'interface.
//   SetupDiClassGuidsFromNameW("Ports"), SetupDiGetClassDevsW,
//   SetupDiEnumDeviceInfo, SetupDiOpenDevRegKey → PortName,
//   SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceRegistryPropertyW,
//   CM_Get_Parent, CM_Get_Device_IDW, CM_Get_DevNode_Registry_PropertyW,
//   CM_Locate_DevNodeW, CM_Get_Child, CM_Get_Sibling,
//   CM_Get_Device_Interface_List(_Size)W, puis GetDIBits (format de l'écran).
// scripts/test-devnodes.sh fournit un faux sysfs : COM5 = ttyACM0 d'une
// « Arduino Uno » (VID 2341, PID 0043, série SN123), et COM7 = un
// pseudo-terminal dont l'autre bout renvoie en majuscules ce qu'il reçoit.
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
__declspec(dllimport) int __stdcall SetCommTimeouts(void *, void *);
__declspec(dllimport) int __stdcall GetCommState(void *, void *);
__declspec(dllimport) int __stdcall SetCommState(void *, void *);
__declspec(dllimport) long __stdcall RegQueryValueExW(void *, const u16 *, u32 *, u32 *, void *, u32 *);
__declspec(dllimport) long __stdcall RegCloseKey(void *);
__declspec(dllimport) int __stdcall SetupDiClassGuidsFromNameW(const u16 *, u8 *, u32, u32 *);
__declspec(dllimport) void *__stdcall SetupDiGetClassDevsW(const u8 *, const u16 *, void *, u32);
__declspec(dllimport) int __stdcall SetupDiEnumDeviceInfo(void *, u32, void *);
__declspec(dllimport) void *__stdcall SetupDiOpenDevRegKey(void *, void *, u32, u32, u32, u32);
__declspec(dllimport) int __stdcall SetupDiGetDeviceInstanceIdW(void *, void *, u16 *, u32, u32 *);
__declspec(dllimport) int __stdcall SetupDiGetDeviceRegistryPropertyW(void *, void *, u32, u32 *, void *, u32, u32 *);
__declspec(dllimport) int __stdcall SetupDiDestroyDeviceInfoList(void *);
__declspec(dllimport) u32 __stdcall CM_Get_Parent(u32 *, u32, u32);
__declspec(dllimport) u32 __stdcall CM_Get_Child(u32 *, u32, u32);
__declspec(dllimport) u32 __stdcall CM_Get_Sibling(u32 *, u32, u32);
__declspec(dllimport) u32 __stdcall CM_Locate_DevNodeW(u32 *, const u16 *, u32);
__declspec(dllimport) u32 __stdcall CM_Get_Device_IDW(u32, u16 *, u32, u32);
__declspec(dllimport) u32 __stdcall CM_Get_DevNode_Registry_PropertyW(u32, u32, u32 *, void *, u32 *, u32);
__declspec(dllimport) u32 __stdcall CM_Get_Device_Interface_List_SizeW(u32 *, const u8 *, const u16 *, u32);
__declspec(dllimport) u32 __stdcall CM_Get_Device_Interface_ListW(const u8 *, const u16 *, u16 *, u32, u32);
__declspec(dllimport) void *__stdcall GetDC(void *);
__declspec(dllimport) void *__stdcall CreateCompatibleBitmap(void *, int, int);
__declspec(dllimport) int __stdcall GetDIBits(void *, void *, u32, u32, void *, void *, u32);

void *memset(void *d, int c, u64 n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}
void *memcpy(void *d, const void *s, u64 n) {
    u8 *o = d;
    const u8 *i = s;
    while (n--) *o++ = *i++;
    return d;
}
u64 wcslen(const u16 *s) {
    u64 n = 0;
    while (s[n]) n++;
    return n;
}

static int weq(const u16 *a, const char *b) {
    while (*b)
        if (*a++ != (u16)*b++) return 0;
    return *a == 0;
}
static int wcontains(const u16 *a, const char *b) {
    for (; *a; a++) {
        const u16 *x = a;
        const char *y = b;
        while (*y && *x == (u16)*y) x++, y++;
        if (!*y) return 1;
    }
    return 0;
}
static const u16 *wnext(const u16 *s) {
    while (*s) s++;
    return s + 1;
}

// {86E0D1E0-8089-11D0-9CE4-08003E301F73}
static const u8 COMPORT[16] = {0xE0, 0xD1, 0xE0, 0x86, 0x89, 0x80, 0xD0, 0x11,
                               0x9C, 0xE4, 0x08, 0x00, 0x3E, 0x30, 0x1F, 0x73};

typedef struct { u32 size; u8 guid[16]; u32 devinst; u64 reserved; } DevInfo;

void entry(void) {
    static const u16 ports[] = {'P', 'o', 'r', 't', 's', 0};
    static const u16 portname[] = {'P', 'o', 'r', 't', 'N', 'a', 'm', 'e', 0};
    u8 guid[16];
    u32 count = 0;
    if (!SetupDiClassGuidsFromNameW(ports, guid, 1, &count) || count != 1) ExitProcess(1);
    if (guid[0] != 0x78 || guid[3] != 0x4D) ExitProcess(2); // {4D36E978-…}
    void *set = SetupDiGetClassDevsW(guid, 0, 0, 2 /* DIGCF_PRESENT */);
    if (set == (void *)-1) ExitProcess(3);

    int seen5 = 0, seen7 = 0;
    DevInfo dev;
    for (u32 i = 0;; i++) {
        memset(&dev, 0, sizeof dev);
        dev.size = sizeof dev;
        if (!SetupDiEnumDeviceInfo(set, i, &dev)) break;
        void *key = SetupDiOpenDevRegKey(set, &dev, 1, 0, 1 /* DIREG_DEV */, 0x20019);
        if (key == (void *)-1) ExitProcess(4);
        u16 name[32];
        u32 type = 0, size = sizeof name;
        if (RegQueryValueExW(key, portname, 0, &type, name, &size) != 0 || type != 1) ExitProcess(5);
        RegCloseKey(key);
        u16 id[256];
        u32 need = 0;
        if (!SetupDiGetDeviceInstanceIdW(set, &dev, id, 256, &need)) ExitProcess(6);
        if (weq(name, "COM5")) {
            seen5 = 1;
            u16 text[256];
            if (!SetupDiGetDeviceRegistryPropertyW(set, &dev, 0xC /* FRIENDLYNAME */, &type, text, sizeof text, &need))
                ExitProcess(7);
            if (!weq(text, "Arduino Uno (COM5)")) ExitProcess(8);
            if (!SetupDiGetDeviceRegistryPropertyW(set, &dev, 1 /* HARDWAREID */, &type, text, sizeof text, &need) ||
                type != 7 || !weq(text, "USB\\VID_2341&PID_0043&MI_00"))
                ExitProcess(9);
            // pyserial : VID, PID et numéro de série lus sur le parent USB.
            u32 parent = 0;
            if (CM_Get_Parent(&parent, dev.devinst, 0) != 0) ExitProcess(10);
            if (CM_Get_Device_IDW(parent, text, 256, 0) != 0 || !weq(text, "USB\\VID_2341&PID_0043\\SN123"))
                ExitProcess(11);
            u32 len = sizeof text;
            if (CM_Get_DevNode_Registry_PropertyW(parent, 0xC /* CM_DRP_MFG */, &type, text, &len, 0) != 0 ||
                !weq(text, "Arduino"))
                ExitProcess(12);
        } else if (weq(name, "COM7")) {
            seen7 = 1;
            if (!weq(id, "ROOT\\PORTS\\0007")) ExitProcess(13);
        }
    }
    if (!seen5 || !seen7) ExitProcess(14);
    SetupDiDestroyDeviceInfoList(set);

    // Parcours de l'arbre depuis la racine : le parent USB et COM7.
    u32 root = 0, node = 0, n = 0;
    if (CM_Locate_DevNodeW(&root, 0, 0) != 0) ExitProcess(15);
    if (CM_Get_Child(&node, root, 0) != 0) ExitProcess(16);
    do n++;
    while (CM_Get_Sibling(&node, node, 0) == 0 && n < 50);
    if (n < 2) ExitProcess(17);

    // Interfaces COMPORT, puis ouverture de COM7 par son chemin.
    u32 len = 0;
    if (CM_Get_Device_Interface_List_SizeW(&len, COMPORT, 0, 0) != 0 || len < 10 || len > 4096) ExitProcess(18);
    static u16 list[4096];
    if (CM_Get_Device_Interface_ListW(COMPORT, 0, list, len, 0) != 0) ExitProcess(19);
    const u16 *com7 = 0;
    for (const u16 *p = list; *p; p = wnext(p))
        if (wcontains(p, "root#ports#0007#{86e0d1e0-8089-11d0-9ce4-08003e301f73}")) com7 = p;
    if (!com7) ExitProcess(20);
    void *h = CreateFileW(com7, 0xC0000000, 0, 0, 3 /* OPEN_EXISTING */, 0, 0);
    if (h == (void *)-1) ExitProcess(21);
    // Comme pyserial : DCB relu puis réappliqué (mode brut, 8N1).
    u8 dcb[28];
    memset(dcb, 0, sizeof dcb);
    *(u32 *)dcb = sizeof dcb;
    if (!GetCommState(h, dcb) || !SetCommState(h, dcb)) ExitProcess(29);
    u32 timeouts[5] = {0, 0, 2000, 0, 0};
    SetCommTimeouts(h, timeouts);
    u32 done = 0;
    if (!WriteFile(h, "abc", 3, &done, 0) || done != 3) ExitProcess(22);
    char back[8];
    u32 got = 0;
    while (got < 3) {
        if (!ReadFile(h, back + got, 3 - got, &done, 0) || done == 0) ExitProcess(23);
        got += done;
    }
    if (back[0] != 'A' || back[1] != 'B' || back[2] != 'C') ExitProcess(24);
    CloseHandle(h);

    // Format de l'écran comme le lit SDL3 : 32 bits, BI_BITFIELDS, 8 bits
    // par composante.
    void *dc = GetDC(0);
    void *bmp = CreateCompatibleBitmap(dc, 1, 1);
    u32 bmi[10 + 256];
    memset(bmi, 0, sizeof bmi);
    bmi[0] = 40;
    if (!GetDIBits(dc, bmp, 0, 1, 0, bmi, 0)) ExitProcess(25);
    if ((bmi[3] >> 16) != 32 || bmi[4] != 3) ExitProcess(26);
    if (!GetDIBits(dc, bmp, 0, 1, 0, bmi, 0)) ExitProcess(27);
    if (bmi[10] != 0xFF0000 || bmi[11] != 0xFF00 || bmi[12] != 0xFF) ExitProcess(28);
    ExitProcess(0);
}

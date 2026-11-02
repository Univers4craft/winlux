// winusb.dll (#271, partie 5), au format MSVC (clang en mode
// x86_64-pc-windows-msvc, sans CRT) : trouver un appareil USB par setupapi,
// l'ouvrir, puis le décrire par WinUSB comme un outil de flashage.
// scripts/test-winusb.sh fournit un faux sysfs : appareil 16D0:1234, série
// WUSB01, « Carte de test », 480 Mbit/s, une interface à deux points bulk de
// 512 octets, sans usbfs (les transferts doivent alors échouer proprement).
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) u32 __stdcall GetLastError(void);
__declspec(dllimport) void *__stdcall CreateFileW(const u16 *, u32, u32, void *, u32, u32, void *);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) void *__stdcall SetupDiGetClassDevsW(const u8 *, const u16 *, void *, u32);
__declspec(dllimport) int __stdcall SetupDiEnumDeviceInterfaces(void *, void *, const u8 *, u32, void *);
__declspec(dllimport) int __stdcall SetupDiGetDeviceInterfaceDetailW(void *, void *, void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall SetupDiDestroyDeviceInfoList(void *);
__declspec(dllimport) int __stdcall WinUsb_Initialize(void *, void **);
__declspec(dllimport) int __stdcall WinUsb_Free(void *);
__declspec(dllimport) int __stdcall WinUsb_QueryDeviceInformation(void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall WinUsb_GetDescriptor(void *, u8, u8, u16, u8 *, u32, u32 *);
__declspec(dllimport) int __stdcall WinUsb_QueryInterfaceSettings(void *, u8, u8 *);
__declspec(dllimport) int __stdcall WinUsb_QueryPipe(void *, u8, u8, void *);
__declspec(dllimport) int __stdcall WinUsb_SetPipePolicy(void *, u8, u32, u32, void *);
__declspec(dllimport) int __stdcall WinUsb_GetPipePolicy(void *, u8, u32, u32 *, void *);
typedef struct { u8 type, request; u16 value, index, length; } Setup;
__declspec(dllimport) int __stdcall WinUsb_ControlTransfer(void *, Setup, u8 *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall WinUsb_ReadPipe(void *, u8, u8 *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall WinUsb_GetAssociatedInterface(void *, u8, void **);

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

static int wcontains(const u16 *a, const char *b) {
    for (; *a; a++) {
        const u16 *x = a;
        const char *y = b;
        while (*y && *x == (u16)*y) x++, y++;
        if (!*y) return 1;
    }
    return 0;
}

// {A5DCBF10-6530-11D2-901F-00C04FB951ED} : GUID_DEVINTERFACE_USB_DEVICE
static const u8 USBDEV[16] = {0x10, 0xBF, 0xDC, 0xA5, 0x30, 0x65, 0xD2, 0x11,
                              0x90, 0x1F, 0x00, 0xC0, 0x4F, 0xB9, 0x51, 0xED};

void entry(void) {
    void *set = SetupDiGetClassDevsW(USBDEV, 0, 0, 0x12 /* PRESENT | DEVICEINTERFACE */);
    if (set == (void *)-1) ExitProcess(1);
    struct { u32 size; u8 guid[16]; u32 flags; u64 reserved; } ifd;
    memset(&ifd, 0, sizeof ifd);
    ifd.size = sizeof ifd;
    if (!SetupDiEnumDeviceInterfaces(set, 0, USBDEV, 0, &ifd)) ExitProcess(2);
    static u8 detail[1024];
    u32 need = 0;
    *(u32 *)detail = 8;
    if (!SetupDiGetDeviceInterfaceDetailW(set, &ifd, detail, sizeof detail, &need, 0)) ExitProcess(3);
    const u16 *path = (const u16 *)(detail + 4);
    if (!wcontains(path, "vid_16d0&pid_1234#wusb01#")) ExitProcess(4);
    void *h = CreateFileW(path, 0xC0000000, 3, 0, 3 /* OPEN_EXISTING */, 0x40000000 /* OVERLAPPED */, 0);
    if (h == (void *)-1) ExitProcess(5);
    SetupDiDestroyDeviceInfoList(set);

    void *w = 0;
    if (!WinUsb_Initialize(h, &w) || !w) ExitProcess(6);
    u8 speed = 0;
    u32 len = 1;
    if (!WinUsb_QueryDeviceInformation(w, 1 /* DEVICE_SPEED */, &len, &speed) || speed != 3) ExitProcess(7);
    u8 buf[256];
    u32 n = 0;
    if (!WinUsb_GetDescriptor(w, 1, 0, 0, buf, 18, &n) || n != 18) ExitProcess(8);
    if (buf[8] != 0xD0 || buf[9] != 0x16 || buf[10] != 0x34 || buf[11] != 0x12) ExitProcess(9);
    u8 ifdesc[9];
    if (!WinUsb_QueryInterfaceSettings(w, 0, ifdesc) || ifdesc[4] != 2) ExitProcess(10);
    struct { u32 type; u8 id; u16 maxp; u8 interval; } pipe;
    if (!WinUsb_QueryPipe(w, 0, 0, &pipe) || pipe.type != 2 || pipe.id != 0x81 || pipe.maxp != 512) ExitProcess(11);
    if (!WinUsb_GetDescriptor(w, 3, 2, 0x0409, buf, sizeof buf, &n) || buf[1] != 3) ExitProcess(12);
    // « Carte de test » en UTF-16LE.
    if (n != 2 + 13 * 2 || buf[2] != 'C' || buf[4] != 'a' || buf[26] != 't') ExitProcess(13);
    u32 ms = 1000, back = 0;
    if (!WinUsb_SetPipePolicy(w, 0x81, 3 /* PIPE_TRANSFER_TIMEOUT */, 4, &ms)) ExitProcess(14);
    len = 4;
    if (!WinUsb_GetPipePolicy(w, 0x81, 3, &len, &back) || back != 1000) ExitProcess(15);
    Setup get = {0x80, 6, 0x0100, 0, 18};
    if (!WinUsb_ControlTransfer(w, get, buf, sizeof buf, &n, 0) || n != 18 || buf[1] != 1) ExitProcess(16);
    // Sans usbfs : refus propre.
    if (WinUsb_ReadPipe(w, 0x81, buf, 64, &n, 0) || GetLastError() != 5) ExitProcess(17);
    void *w2 = 0;
    if (WinUsb_GetAssociatedInterface(w, 0, &w2) || GetLastError() != 259) ExitProcess(18);
    if (!WinUsb_Free(w)) ExitProcess(19);
    CloseHandle(h);
    ExitProcess(0);
}

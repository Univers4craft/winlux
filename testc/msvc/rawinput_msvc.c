// Entrée brute (#271), au format MSVC (clang en mode x86_64-pc-windows-msvc,
// sans CRT) : la souris relative et le clavier des jeux de tir.
//   RegisterRawInputDevices, WM_INPUT, GetRawInputData (taille puis paquet),
//   GetRawInputDeviceList, GetRawInputDeviceInfoW.
// Les appareils sont deux FIFO (scripts/test-rawinput.sh) dans lesquelles le
// programme écrit lui-même des struct input_event Linux.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall CreateFileW(const u16 *, u32, u32, void *, u32, u32, void *);
__declspec(dllimport) int __stdcall WriteFile(void *, const void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) void __stdcall Sleep(u32);
__declspec(dllimport) u64 __stdcall GetTickCount64(void);

__declspec(dllimport) void *__stdcall CreateWindowExW(u32, const u16 *, const u16 *, u32, int, int, int, int, void *,
                                                      void *, void *, void *);
__declspec(dllimport) int __stdcall PeekMessageW(void *, void *, u32, u32, u32);
__declspec(dllimport) int __stdcall RegisterRawInputDevices(const void *, u32, u32);
__declspec(dllimport) u32 __stdcall GetRawInputData(void *, u32, void *, u32 *, u32);
__declspec(dllimport) u32 __stdcall GetRawInputDeviceList(void *, u32 *, u32);
__declspec(dllimport) u32 __stdcall GetRawInputDeviceInfoW(void *, u32, void *, u32 *);

typedef struct { void *hwnd; u32 message, pad; u64 wparam, lparam; u32 time; int x, y, priv; } Msg;
typedef struct { u16 page, usage; u32 flags; void *hwnd; } Device;
typedef struct { u64 sec, usec; u16 type, code; int value; } Event;

void *memset(void *d, int c, u64 n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}
u64 wcslen(const u16 *s) {
    u64 n = 0;
    while (s[n]) n++;
    return n;
}
void *memcpy(void *d, const void *s, u64 n) {
    u8 *o = d;
    const u8 *i = s;
    while (n--) *o++ = *i++;
    return d;
}

static const u16 STATIC_[] = {'S', 'T', 'A', 'T', 'I', 'C', 0};
static const u16 souris[] = {'s', 'o', 'u', 'r', 'i', 's', '.', 'f', 'i', 'f', 'o', 0};
static const u16 clavier[] = {'c', 'l', 'a', 'v', 'i', 'e', 'r', '.', 'f', 'i', 'f', 'o', 0};
static void *INVALID = (void *)~0ull;
static u8 packet[64];

static void inject(const u16 *path, const Event *e, u32 n) {
    void *f = CreateFileW(path, 0x40000000, 3, 0, 3, 0, 0);
    if (f == INVALID) ExitProcess(20);
    u32 done = 0;
    WriteFile(f, e, n * sizeof *e, &done, 0);
    CloseHandle(f);
}

// Attend un WM_INPUT (5 s au plus) et rend son paquet.
static u32 wait_input(void *hwnd, u32 *type) {
    Msg m;
    u64 end = GetTickCount64() + 5000;
    while (GetTickCount64() < end) {
        while (PeekMessageW(&m, 0, 0, 0, 1)) {
            if (m.message != 0x00FF) continue;
            if (m.hwnd != hwnd) return 0;
            u32 size = 0;
            if (GetRawInputData((void *)m.lparam, 0x10000003, 0, &size, 24) != 0 || size > sizeof packet) return 0;
            if (GetRawInputData((void *)m.lparam, 0x10000003, packet, &size, 24) != size) return 0;
            *type = *(u32 *)packet;
            return size;
        }
        Sleep(10);
    }
    return 0;
}

int entry() {
    // 1. Fenêtre cible et inscription souris + clavier.
    void *hwnd = CreateWindowExW(0, STATIC_, STATIC_, 0, 0, 0, 10, 10, 0, 0, 0, 0);
    if (!hwnd) ExitProcess(1);
    Device devs[2] = {{1, 2, 0, hwnd}, {1, 6, 0, hwnd}};
    if (!RegisterRawInputDevices(devs, 2, sizeof *devs)) ExitProcess(1);
    // 2. Liste des appareils : une souris et un clavier, avec leur nom.
    u32 n = 0;
    u64 list[8];
    if (GetRawInputDeviceList(0, &n, 16) != 0 || n != 2) ExitProcess(2);
    if (GetRawInputDeviceList(list, &n, 16) != 2 || list[1] != 0 || list[3] != 1) ExitProcess(2);
    u16 name[160];
    u32 chars = 160;
    if (GetRawInputDeviceInfoW((void *)list[0], 0x20000007, name, &chars) == (u32)-1 || name[0] != '\\')
        ExitProcess(2);
    Sleep(300); // le fil de lecture ouvre les appareils
    // 3. Souris : +5, -3, bouton gauche, en un seul paquet RAWMOUSE.
    Event m[4] = {{0, 0, 2, 0, 5}, {0, 0, 2, 1, -3}, {0, 0, 1, 0x110, 1}, {0, 0, 0, 0, 0}};
    inject(souris, m, 4);
    u32 type = 9, size = wait_input(hwnd, &type);
    if (size != 48 || type != 0) ExitProcess(3);
    if (*(u16 *)(packet + 28) != 0x1 || *(int *)(packet + 36) != 5 || *(int *)(packet + 40) != -3) ExitProcess(3);
    // 4. Molette : un cran = 120.
    Event w[2] = {{0, 0, 2, 8, -1}, {0, 0, 0, 0, 0}};
    inject(souris, w, 2);
    size = wait_input(hwnd, &type);
    if (size != 48 || *(u16 *)(packet + 28) != 0x400 || *(short *)(packet + 30) != -120) ExitProcess(4);
    // 5. Clavier : W enfoncée -> MakeCode 0x11, VKey 'W', WM_KEYDOWN.
    Event k[2] = {{0, 0, 1, 17, 1}, {0, 0, 0, 0, 0}};
    inject(clavier, k, 2);
    size = wait_input(hwnd, &type);
    if (size != 40 || type != 1) ExitProcess(5);
    if (*(u16 *)(packet + 24) != 0x11 || *(u16 *)(packet + 30) != 'W' || *(u32 *)(packet + 32) != 0x100)
        ExitProcess(5);
    // 6. Désinscription de la souris : plus de WM_INPUT pour elle.
    Device off = {1, 2, 1, 0};
    if (!RegisterRawInputDevices(&off, 1, sizeof off)) ExitProcess(6);
    inject(souris, m, 4);
    Sleep(300);
    Msg msg;
    while (PeekMessageW(&msg, 0, 0, 0, 1))
        if (msg.message == 0x00FF) ExitProcess(6);
    ExitProcess(0);
    return 0;
}

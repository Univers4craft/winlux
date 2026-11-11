// Saisie simulée (#1683), au format MSVC sans CRT, construit en x64 et en
// x86 par scripts/test-sendinput.sh et lancé sous Xvfb :
//   1. SendInput refuse un cbSize faux (ERROR_INVALID_PARAMETER) ;
//   2. déplacement absolu au centre de la fenêtre puis clic gauche :
//      WM_LBUTTONDOWN et WM_LBUTTONUP reviennent ;
//   3. VK_A (KEYBDINPUT) : WM_KEYDOWN 0x41 puis WM_CHAR « a » ;
//   4. KEYEVENTF_UNICODE : « é » puis « € » (keycode réaffecté) en WM_CHAR ;
//   5. molette : WM_MOUSEWHEEL vers le bas ;
//   6. keybd_event(VK_B) : WM_CHAR « b ».
// Avec « refus » sur la ligne de commande : l'autorisation n'est pas
// accordée, SendInput rend 0 et ERROR_ACCESS_DENIED (test 7).
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;
typedef unsigned long long u64;
typedef __SIZE_TYPE__ uptr;
typedef long long i64;
typedef __PTRDIFF_TYPE__ iptr;

#define API __declspec(dllimport) __stdcall
API void ExitProcess(u32);
API u32 GetLastError(void);
API void SetLastError(u32);
API void Sleep(u32);
API u32 GetTickCount(void);
API u16 *GetCommandLineW(void);
API void *GetModuleHandleW(const u16 *);

typedef struct { int left, top, right, bottom; } Rect;
typedef struct { void *hwnd; u32 message; uptr wparam; iptr lparam; u32 time; int x, y; u32 priv; } Msg;
typedef iptr(__stdcall *WndProc)(void *, u32, uptr, iptr);
typedef struct {
    u32 style;
    WndProc proc;
    int cls_extra, wnd_extra;
    void *instance, *icon, *cursor, *background;
    const u16 *menu, *name;
} WndClass;

// INPUT : 40 octets en x64, 28 en x86 (disposition naturelle, comme le SDK).
typedef struct { int dx, dy; u32 data, flags, time; uptr extra; } MouseInput;
typedef struct { u16 vk, scan; u32 flags, time; uptr extra; } KeybdInput;
typedef struct {
    u32 type;
    union { MouseInput mi; KeybdInput ki; };
} Input;

API u16 RegisterClassW(const WndClass *);
API void *CreateWindowExW(u32, const u16 *, const u16 *, u32, int, int, int, int, void *, void *, void *, void *);
API iptr DefWindowProcW(void *, u32, uptr, iptr);
API int PeekMessageW(Msg *, void *, u32, u32, u32);
API int TranslateMessage(const Msg *);
API iptr DispatchMessageW(const Msg *);
API u32 SendInput(u32, Input *, int);
API void keybd_event(u8, u8, u32, uptr);
API int GetWindowRect(void *, Rect *);
API int GetSystemMetrics(int);
API int SetForegroundWindow(void *);
API void *SetFocus(void *);
API int ShowWindow(void *, int);
API int UpdateWindow(void *);

void *memset(void *d, int c, uptr n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}

static volatile int keydown_a, ldown, lup, wheel_down;
static u16 chars[16];
static volatile int nchars;

static iptr __stdcall proc(void *h, u32 m, uptr w, iptr l) {
    switch (m) {
    case 0x0100: if (w == 0x41) keydown_a = 1; break; // WM_KEYDOWN
    case 0x0102: if (nchars < 16) chars[nchars++] = (u16)w; break; // WM_CHAR
    case 0x0201: ldown = 1; break;
    case 0x0202: lup = 1; break;
    case 0x020A: if ((short)(w >> 16) < 0) wheel_down = 1; break; // WM_MOUSEWHEEL
    }
    return DefWindowProcW(h, m, w, l);
}

static void pump(u32 ms) {
    u32 end = GetTickCount() + ms;
    Msg msg;
    do {
        while (PeekMessageW(&msg, 0, 0, 0, 1)) {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Sleep(10);
    } while ((int)(GetTickCount() - end) < 0);
}

static int wait_for(volatile int *flag) {
    for (int i = 0; i < 300 && !*flag; i++) pump(10);
    return *flag;
}

static int wait_chars(int n) {
    for (int i = 0; i < 300 && nchars < n; i++) pump(10);
    return nchars >= n;
}

static Input key(u16 vk, u16 scan, u32 flags) {
    Input in;
    memset(&in, 0, sizeof in);
    in.type = 1;
    in.ki.vk = vk;
    in.ki.scan = scan;
    in.ki.flags = flags;
    return in;
}

static int has(const u16 *s, const u16 *word) {
    for (; *s; s++) {
        const u16 *a = s, *b = word;
        while (*b && *a == *b) a++, b++;
        if (!*b) return 1;
    }
    return 0;
}

void entry(void) {
    static const u16 cls[] = {'S', 'a', 'i', 's', 'i', 'e', 0};
    static const u16 refus[] = {'r', 'e', 'f', 'u', 's', 0};
    if (sizeof(Input) != (sizeof(void *) == 8 ? 40 : 28)) ExitProcess(99);
    WndClass wc;
    memset(&wc, 0, sizeof wc);
    wc.proc = proc;
    wc.instance = GetModuleHandleW(0);
    wc.name = cls;
    RegisterClassW(&wc);
    void *hwnd = CreateWindowExW(0, cls, cls, 0x10CF0000, 100, 100, 400, 300, 0, 0, wc.instance, 0);
    if (!hwnd) ExitProcess(98);
    ShowWindow(hwnd, 5);
    UpdateWindow(hwnd);
    SetForegroundWindow(hwnd);
    SetFocus(hwnd);
    pump(800);

    Input in[4];
    if (has(GetCommandLineW(), refus)) {
        in[0] = key(0x41, 0, 0);
        SetLastError(0);
        if (SendInput(1, in, sizeof(Input)) != 0 || GetLastError() != 5) ExitProcess(7);
        ExitProcess(0);
    }

    // 1. cbSize faux.
    SetLastError(0);
    in[0] = key(0x41, 0, 0);
    if (SendInput(1, in, sizeof(Input) + 4) != 0 || GetLastError() != 87) ExitProcess(1);

    // 2. Pointeur au centre de la fenêtre, puis clic gauche.
    Rect r;
    GetWindowRect(hwnd, &r);
    int sw = GetSystemMetrics(0), sh = GetSystemMetrics(1);
    int cx = (r.left + r.right) / 2, cy = (r.top + r.bottom) / 2;
    memset(in, 0, sizeof in);
    in[0].type = 0;
    in[0].mi.dx = (int)((u32)cx * 65535u / (u32)(sw - 1));
    in[0].mi.dy = (int)((u32)cy * 65535u / (u32)(sh - 1));
    in[0].mi.flags = 0x8001; // MOVE | ABSOLUTE
    in[1].type = 0;
    in[1].mi.flags = 0x2; // LEFTDOWN
    in[2].type = 0;
    in[2].mi.flags = 0x4; // LEFTUP
    if (SendInput(3, in, sizeof(Input)) != 3) ExitProcess(2);
    if (!wait_for(&ldown) || !wait_for(&lup)) ExitProcess(2);
    SetFocus(hwnd);
    pump(100);

    // 3. VK_A.
    nchars = 0;
    in[0] = key(0x41, 0, 0);
    in[1] = key(0x41, 0, 2);
    if (SendInput(2, in, sizeof(Input)) != 2) ExitProcess(3);
    if (!wait_for(&keydown_a) || !wait_chars(1) || chars[0] != 'a') ExitProcess(3);

    // 4. Caractères Unicode : é (Latin-1) puis € (keycode réaffecté).
    nchars = 0;
    in[0] = key(0, 0xE9, 4);
    in[1] = key(0, 0xE9, 4 | 2);
    in[2] = key(0, 0x20AC, 4);
    in[3] = key(0, 0x20AC, 4 | 2);
    if (SendInput(4, in, sizeof(Input)) != 4) ExitProcess(4);
    if (!wait_chars(2) || chars[0] != 0xE9 || chars[1] != 0x20AC) ExitProcess(4);

    // 5. Molette d'un cran vers le bas.
    memset(in, 0, sizeof in);
    in[0].type = 0;
    in[0].mi.data = (u32)-120;
    in[0].mi.flags = 0x800;
    if (SendInput(1, in, sizeof(Input)) != 1 || !wait_for(&wheel_down)) ExitProcess(5);

    // 6. keybd_event.
    nchars = 0;
    keybd_event(0x42, 0, 0, 0);
    keybd_event(0x42, 0, 2, 0);
    if (!wait_chars(1) || chars[0] != 'b') ExitProcess(6);
    ExitProcess(0);
}

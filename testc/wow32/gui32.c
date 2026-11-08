/* Fenêtre d'un programme Windows 32 bits (x86) sous Winlux (#232, étape 5).
 * Construit par clang --target=i686-pc-windows-msvc et lld-link, sans CRT :
 * kernel32, user32 et gdi32 en stdcall. Classe et procédure de fenêtre x86,
 * CREATESTRUCT de WM_NCCREATE et WM_CREATE, boucle GetMessage/Dispatch,
 * WM_PAINT avec BeginPaint et dessin GDI, clavier et souris, minuteurs,
 * sous-classement par SetWindowLong et CallWindowProc, structure modifiée
 * par la procédure (WINDOWPOS), DIB section et GetObject, boîte de dialogue
 * en mémoire, WM_QUIT. Rend 0 si tout va bien, sinon le numéro de la
 * vérification qui a échoué.
 * Argument « afficher » : la fenêtre reste 3 secondes à l'écran (essai à
 * l'œil sous X11). */
typedef unsigned int DWORD, UINT;
typedef int BOOL, LONG;
typedef unsigned short WORD, ATOM;
typedef unsigned char BYTE;
typedef void *HANDLE, *HWND, *HDC, *HINSTANCE, *HBRUSH, *HGDIOBJ, *HBITMAP, *HMENU, *HICON,
    *HCURSOR;
typedef unsigned int WPARAM;
typedef long LPARAM, LRESULT;
typedef unsigned long COLORREF;

#define WINAPI __stdcall
#define CALLBACK __stdcall
typedef LRESULT(CALLBACK *WNDPROC)(HWND, UINT, WPARAM, LPARAM);
typedef void(CALLBACK *TIMERPROC)(HWND, UINT, UINT, DWORD);
typedef int(CALLBACK *DLGPROC)(HWND, UINT, WPARAM, LPARAM);

typedef struct { LONG left, top, right, bottom; } RECT;
typedef struct { LONG x, y; } POINT;
typedef struct { HWND hwnd; UINT message; WPARAM wParam; LPARAM lParam; DWORD time; POINT pt; } MSG;
typedef struct {
    HDC hdc; BOOL fErase; RECT rcPaint; BOOL fRestore; BOOL fIncUpdate; BYTE rgbReserved[32];
} PAINTSTRUCT;
typedef struct {
    UINT cbSize, style; WNDPROC lpfnWndProc; int cbClsExtra, cbWndExtra; HINSTANCE hInstance;
    HICON hIcon; HCURSOR hCursor; HBRUSH hbrBackground; const char *lpszMenuName;
    const char *lpszClassName; HICON hIconSm;
} WNDCLASSEXA;
typedef struct {
    void *lpCreateParams; HINSTANCE hInstance; HMENU hMenu; HWND hwndParent;
    int cy, cx, y, x; LONG style; const char *lpszName, *lpszClass; DWORD dwExStyle;
} CREATESTRUCTA;
typedef struct { HWND hwnd, hwndInsertAfter; int x, y, cx, cy; UINT flags; } WINDOWPOS;
typedef struct { LONG bmType, bmWidth, bmHeight, bmWidthBytes; WORD bmPlanes, bmBitsPixel; void *bmBits; } BITMAP;
typedef struct {
    DWORD biSize; LONG biWidth, biHeight; WORD biPlanes, biBitCount; DWORD biCompression,
    biSizeImage; LONG biXPelsPerMeter, biYPelsPerMeter; DWORD biClrUsed, biClrImportant;
} BITMAPINFOHEADER;
typedef struct { UINT lbStyle; COLORREF lbColor; unsigned long lbHatch; } LOGBRUSH;

__declspec(dllimport) HANDLE WINAPI GetStdHandle(DWORD);
__declspec(dllimport) BOOL WINAPI WriteFile(HANDLE, const void *, DWORD, DWORD *, void *);
__declspec(dllimport) void WINAPI ExitProcess(DWORD);
__declspec(dllimport) char *WINAPI GetCommandLineA(void);
__declspec(dllimport) int WINAPI lstrlenA(const char *);
__declspec(dllimport) HINSTANCE WINAPI GetModuleHandleA(const char *);
__declspec(dllimport) void WINAPI Sleep(DWORD);

__declspec(dllimport) ATOM WINAPI RegisterClassExA(const WNDCLASSEXA *);
__declspec(dllimport) HWND WINAPI CreateWindowExA(DWORD, const char *, const char *, DWORD, int, int,
                                                  int, int, HWND, HMENU, HINSTANCE, void *);
__declspec(dllimport) LRESULT WINAPI DefWindowProcA(HWND, UINT, WPARAM, LPARAM);
__declspec(dllimport) BOOL WINAPI ShowWindow(HWND, int);
__declspec(dllimport) BOOL WINAPI UpdateWindow(HWND);
__declspec(dllimport) BOOL WINAPI GetMessageA(MSG *, HWND, UINT, UINT);
__declspec(dllimport) BOOL WINAPI PeekMessageA(MSG *, HWND, UINT, UINT, UINT);
__declspec(dllimport) BOOL WINAPI TranslateMessage(const MSG *);
__declspec(dllimport) LRESULT WINAPI DispatchMessageA(const MSG *);
__declspec(dllimport) BOOL WINAPI PostMessageA(HWND, UINT, WPARAM, LPARAM);
__declspec(dllimport) LRESULT WINAPI SendMessageA(HWND, UINT, WPARAM, LPARAM);
__declspec(dllimport) void WINAPI PostQuitMessage(int);
__declspec(dllimport) BOOL WINAPI DestroyWindow(HWND);
__declspec(dllimport) HDC WINAPI BeginPaint(HWND, PAINTSTRUCT *);
__declspec(dllimport) BOOL WINAPI EndPaint(HWND, const PAINTSTRUCT *);
__declspec(dllimport) int WINAPI FillRect(HDC, const RECT *, HBRUSH);
__declspec(dllimport) BOOL WINAPI GetClientRect(HWND, RECT *);
__declspec(dllimport) LONG WINAPI GetWindowLongA(HWND, int);
__declspec(dllimport) LONG WINAPI SetWindowLongA(HWND, int, LONG);
__declspec(dllimport) LRESULT WINAPI CallWindowProcA(WNDPROC, HWND, UINT, WPARAM, LPARAM);
__declspec(dllimport) int WINAPI GetWindowTextA(HWND, char *, int);
__declspec(dllimport) UINT WINAPI SetTimer(HWND, UINT, UINT, TIMERPROC);
__declspec(dllimport) BOOL WINAPI KillTimer(HWND, UINT);
__declspec(dllimport) BOOL WINAPI InvalidateRect(HWND, const RECT *, BOOL);
__declspec(dllimport) HCURSOR WINAPI LoadCursorA(HINSTANCE, const char *);
__declspec(dllimport) int WINAPI DialogBoxIndirectParamA(HINSTANCE, const void *, HWND, DLGPROC, LPARAM);
__declspec(dllimport) BOOL WINAPI EndDialog(HWND, int);
__declspec(dllimport) BOOL WINAPI IsWindow(HWND);
__declspec(dllimport) DWORD WINAPI CharUpperBuffW(unsigned short *, DWORD);
__declspec(dllimport) DWORD WINAPI CharUpperBuffA(char *, DWORD);
__declspec(dllimport) LRESULT WINAPI SendMessageTimeoutA(HWND, UINT, WPARAM, LPARAM, UINT, UINT, DWORD *);
__declspec(dllimport) BOOL WINAPI GetScrollBarInfo(HWND, LONG, DWORD *);
__declspec(dllimport) HGDIOBJ WINAPI CreatePenIndirect(const DWORD *);
__declspec(dllimport) COLORREF WINAPI SetDCBrushColor(HDC, COLORREF);

__declspec(dllimport) HBRUSH WINAPI CreateSolidBrush(COLORREF);
__declspec(dllimport) HBRUSH WINAPI CreateBrushIndirect(const LOGBRUSH *);
__declspec(dllimport) BOOL WINAPI DeleteObject(HGDIOBJ);
__declspec(dllimport) HGDIOBJ WINAPI SelectObject(HDC, HGDIOBJ);
__declspec(dllimport) BOOL WINAPI TextOutA(HDC, int, int, const char *, int);
__declspec(dllimport) BOOL WINAPI Rectangle(HDC, int, int, int, int);
__declspec(dllimport) COLORREF WINAPI SetTextColor(HDC, COLORREF);
__declspec(dllimport) int WINAPI SetBkMode(HDC, int);
__declspec(dllimport) HDC WINAPI CreateCompatibleDC(HDC);
__declspec(dllimport) BOOL WINAPI DeleteDC(HDC);
__declspec(dllimport) HBITMAP WINAPI CreateDIBSection(HDC, const void *, UINT, void **, HANDLE, DWORD);
__declspec(dllimport) int WINAPI GetObjectA(HGDIOBJ, int, void *);
__declspec(dllimport) COLORREF WINAPI GetPixel(HDC, int, int);
__declspec(dllimport) BOOL WINAPI GdiFlush(void);

#define WM_CREATE 0x0001
#define WM_DESTROY 0x0002
#define WM_PAINT 0x000F
#define WM_CLOSE 0x0010
#define WM_SETTEXT 0x000C
#define WM_WINDOWPOSCHANGING 0x0046
#define WM_NCCREATE 0x0081
#define WM_KEYDOWN 0x0100
#define WM_CHAR 0x0102
#define WM_INITDIALOG 0x0110
#define WM_TIMER 0x0113
#define WM_LBUTTONDOWN 0x0201
#define WM_USER 0x0400
#define GWL_WNDPROC (-4)
#define DWL_USER 8
#define DWL_DLGPROC 4
#define WS_OVERLAPPEDWINDOW 0x00CF0000
#define CW_USEDEFAULT ((int)0x80000000)
#define RGB(r, g, b) ((COLORREF)((r) | ((g) << 8) | ((b) << 16)))

/* clang peut émettre des appels à memset/memcpy même sans CRT. */
void *memset(void *d, int c, unsigned n) {
    unsigned char *p = d;
    while (n--) *p++ = (unsigned char)c;
    return d;
}
void *memcpy(void *d, const void *s, unsigned n) {
    unsigned char *p = d;
    const unsigned char *q = s;
    while (n--) *p++ = *q++;
    return d;
}

static void say(const char *s) {
    DWORD n = 0;
    WriteFile(GetStdHandle((DWORD)-11), s, lstrlenA(s), &n, 0);
}

static void fail(DWORD check, DWORD value) {
    static char m[] = "GUI 32 bits : echec n. 00, valeur 0x00000000\r\n";
    m[23] = '0' + check / 10;
    m[24] = '0' + check % 10;
    for (int i = 0; i < 8; i++)
        m[36 + i] = "0123456789abcdef"[(value >> (28 - 4 * i)) & 15];
    say(m);
    ExitProcess(check);
}

#define CHECK(n, cond, v) do { if (!(cond)) fail(n, (DWORD)(v)); } while (0)

static int same(const char *a, const char *b) {
    while (*a && *a == *b) a++, b++;
    return *a == *b;
}

static int contains(const char *h, const char *n) {
    for (; *h; h++) {
        const char *a = h, *b = n;
        while (*a && *b && *a == *b) a++, b++;
        if (!*b) return 1;
    }
    return 0;
}

static int marker;
static HWND main_hwnd;
static int nccreate_ok, create_ok, painted, key_ok, char_ok, click_ok, timer_ok, timerproc_ok;
static int user_seen, sub_seen, destroyed, show;

static void check_cs(int n, LPARAM lp) {
    CREATESTRUCTA *cs = (CREATESTRUCTA *)lp;
    CHECK(n, cs != 0, 0);
    CHECK(n, cs->lpCreateParams == &marker, cs->lpCreateParams);
    CHECK(n + 1, cs->cx == 320 && cs->cy == 240, cs->cx);
    CHECK(n + 2, cs->x == 40 && cs->y == 30, cs->x);
    CHECK(n + 3, (cs->style & WS_OVERLAPPEDWINDOW) == WS_OVERLAPPEDWINDOW, cs->style);
    CHECK(n + 4, cs->lpszClass && same(cs->lpszClass, "Winlux32"), cs->lpszClass);
    CHECK(n + 5, cs->lpszName && same(cs->lpszName, "Fenetre 32 bits"), cs->lpszName);
}

static void CALLBACK timer_proc(HWND hwnd, UINT msg, UINT id, DWORD time) {
    (void)time;
    if (hwnd == main_hwnd && msg == WM_TIMER && id == 2) timerproc_ok = 1;
    KillTimer(hwnd, 2);
}

/* Comme InitWndProc de la VCL Delphi (WinSCP) : la procédure de classe
 * sous-classe la fenêtre au premier message ; WM_CREATE doit aller à la
 * nouvelle procédure, pas revenir à celle de la classe (#822). */
static int init_calls, init_saw_create, real_saw_create;
static LRESULT CALLBACK real_proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    if (msg == WM_CREATE) real_saw_create++;
    return DefWindowProcA(hwnd, msg, wp, lp);
}
static LRESULT CALLBACK init_proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    init_calls++;
    if (msg == WM_CREATE) init_saw_create++;
    SetWindowLongA(hwnd, GWL_WNDPROC, (LONG)real_proc);
    return CallWindowProcA(real_proc, hwnd, msg, wp, lp);
}

static LRESULT CALLBACK wnd_proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    switch (msg) {
    case WM_NCCREATE:
        check_cs(10, lp);
        nccreate_ok = 1;
        break;
    case WM_CREATE:
        check_cs(10, lp);
        create_ok = 1;
        return 0;
    case WM_PAINT: {
        PAINTSTRUCT ps;
        HDC dc = BeginPaint(hwnd, &ps);
        CHECK(20, dc != 0 && ps.hdc == dc, dc);
        CHECK(21, ps.rcPaint.right > 0 && ps.rcPaint.bottom > 0, ps.rcPaint.right);
        HBRUSH bg = CreateSolidBrush(RGB(106, 61, 240));
        RECT r;
        GetClientRect(hwnd, &r);
        FillRect(dc, &r, bg);
        DeleteObject(bg);
        SetBkMode(dc, 1);
        SetTextColor(dc, RGB(255, 255, 255));
        TextOutA(dc, 20, 20, "Programme Windows 32 bits", 25);
        Rectangle(dc, 20, 60, 140, 120);
        CHECK(22, EndPaint(hwnd, &ps), 0);
        painted++;
        return 0;
    }
    case WM_KEYDOWN:
        if (wp == 'A' && (lp & 0xffff) == 1) key_ok = 1;
        return 0;
    case WM_CHAR:
        if (wp == 'a') char_ok = 1;
        return 0;
    case WM_LBUTTONDOWN:
        if ((lp & 0xffff) == 10 && ((lp >> 16) & 0xffff) == 20) click_ok = 1;
        return 0;
    case WM_TIMER:
        if (wp == 1) {
            timer_ok = 1;
            KillTimer(hwnd, 1);
        }
        return 0;
    case WM_WINDOWPOSCHANGING: {
        WINDOWPOS *p = (WINDOWPOS *)lp;
        if (p->hwnd == hwnd && p->cx == 400) p->cx = 123;
        return 0;
    }
    case WM_USER:
        CHECK(40, lp == (LPARAM)&marker, lp);
        user_seen++;
        return 0x1234;
    case WM_USER + 1:
        /* Valeur négative : LRESULT signé. */
        return -5;
    case WM_DESTROY:
        destroyed = 1;
        PostQuitMessage(7);
        return 0;
    }
    return DefWindowProcA(hwnd, msg, wp, lp);
}

static WNDPROC old_proc;

static LRESULT CALLBACK sub_proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    if (msg == WM_USER) sub_seen++;
    return CallWindowProcA(old_proc, hwnd, msg, wp, lp);
}

static int CALLBACK dlg_proc(HWND dlg, UINT msg, WPARAM wp, LPARAM lp) {
    (void)wp;
    if (msg == WM_INITDIALOG) {
        CHECK(60, lp == 0x5150, lp);
        SetWindowLongA(dlg, DWL_USER, (LONG)&marker);
        CHECK(61, GetWindowLongA(dlg, DWL_USER) == (LONG)&marker, GetWindowLongA(dlg, DWL_USER));
        CHECK(62, GetWindowLongA(dlg, DWL_DLGPROC) == (LONG)dlg_proc, GetWindowLongA(dlg, DWL_DLGPROC));
        EndDialog(dlg, 42);
        return 1;
    }
    return 0;
}

static void pump(void) {
    MSG m;
    while (PeekMessageA(&m, 0, 0, 0, 1)) {
        TranslateMessage(&m);
        DispatchMessageA(&m);
    }
}

void __stdcall entry(void) {
    show = contains(GetCommandLineA(), "afficher");
    HINSTANCE inst = GetModuleHandleA(0);

    WNDCLASSEXA wc = {0};
    wc.cbSize = sizeof wc;
    wc.lpfnWndProc = wnd_proc;
    wc.hInstance = inst;
    wc.hCursor = LoadCursorA(0, (const char *)32512);
    wc.hbrBackground = (HBRUSH)(5 + 1);
    wc.lpszClassName = "Winlux32";
    CHECK(1, sizeof(MSG) == 28 && sizeof(PAINTSTRUCT) == 64 && sizeof(WNDCLASSEXA) == 48, sizeof(MSG));
    ATOM atom = RegisterClassExA(&wc);
    CHECK(2, atom != 0, atom);

    HWND hwnd = CreateWindowExA(0, "Winlux32", "Fenetre 32 bits", WS_OVERLAPPEDWINDOW, 40, 30, 320,
                                240, 0, 0, inst, &marker);
    main_hwnd = hwnd;
    CHECK(3, hwnd != 0, 0);
    CHECK(4, nccreate_ok && create_ok, nccreate_ok * 2 + create_ok);
    CHECK(5, GetWindowLongA(hwnd, GWL_WNDPROC) == (LONG)wnd_proc, GetWindowLongA(hwnd, GWL_WNDPROC));

    ShowWindow(hwnd, 1);
    UpdateWindow(hwnd);
    pump();
    CHECK(23, painted > 0, painted);

    /* Titre par WM_SETTEXT (traité par DefWindowProcA). */
    CHECK(24, SendMessageA(hwnd, WM_SETTEXT, 0, (LPARAM) "Titre change"), 0);
    char title[64];
    GetWindowTextA(hwnd, title, sizeof title);
    CHECK(25, same(title, "Titre change"), title[0]);

    /* Clavier et souris par la file de messages. */
    MSG m;
    PostMessageA(hwnd, WM_KEYDOWN, 'A', 1);
    CHECK(26, PeekMessageA(&m, hwnd, 0, 0, 0) && m.hwnd == hwnd && m.message == WM_KEYDOWN
                  && m.wParam == 'A' && m.lParam == 1,
          m.message);
    PostMessageA(hwnd, WM_CHAR, 'a', 1);
    PostMessageA(hwnd, WM_LBUTTONDOWN, 1, 10 | (20 << 16));
    pump();
    CHECK(27, key_ok, 0);
    CHECK(28, char_ok, 0);
    CHECK(29, click_ok, 0);

    /* Minuteurs : WM_TIMER et TIMERPROC. */
    SetTimer(hwnd, 1, 10, 0);
    SetTimer(hwnd, 2, 10, timer_proc);
    /* PeekMessage : en mode sans écran, GetMessage rend WM_QUIT dès que
     * la file est vide, minuteurs en attente ou non. */
    for (int i = 0; i < 400 && !(timer_ok && timerproc_ok); i++) {
        if (PeekMessageA(&m, 0, 0, 0, 1))
            DispatchMessageA(&m);
        else
            Sleep(5);
    }
    CHECK(30, timer_ok, 0);
    CHECK(31, timerproc_ok, 0);

    /* Sous-classement : SetWindowLong rend l'ancienne procédure x86. */
    CHECK(41, SendMessageA(hwnd, WM_USER, 0, (LPARAM)&marker) == 0x1234, 0);
    old_proc = (WNDPROC)SetWindowLongA(hwnd, GWL_WNDPROC, (LONG)sub_proc);
    CHECK(42, old_proc == wnd_proc, old_proc);
    CHECK(43, SendMessageA(hwnd, WM_USER, 0, (LPARAM)&marker) == 0x1234, 0);
    CHECK(44, sub_seen == 1 && user_seen == 2, sub_seen * 16 + user_seen);
    CHECK(45, SendMessageA(hwnd, WM_USER + 1, 0, 0) == -5, 0);
    SetWindowLongA(hwnd, GWL_WNDPROC, (LONG)old_proc);
    CHECK(46, GetWindowLongA(hwnd, GWL_WNDPROC) == (LONG)wnd_proc, 0);

    /* Structure modifiée par la procédure et recopiée (WINDOWPOS). */
    WINDOWPOS pos = {hwnd, (HWND)-1, 1, 2, 400, 300, 0x14};
    SendMessageA(hwnd, WM_WINDOWPOSCHANGING, 0, (LPARAM)&pos);
    CHECK(47, pos.cx == 123 && pos.hwnd == hwnd && pos.hwndInsertAfter == (HWND)-1 && pos.flags == 0x14,
          pos.cx);

    /* DIB section 32 bits : pointeur de pixels de 4 octets, dessin GDI. */
    BITMAPINFOHEADER bi = {sizeof bi, 16, -16, 1, 32, 0, 0, 0, 0, 0, 0};
    void *bits = 0;
    HDC mem = CreateCompatibleDC(0);
    HBITMAP bmp = CreateDIBSection(mem, &bi, 0, &bits, 0, 0);
    CHECK(50, bmp != 0 && bits != 0, bits);
    CHECK(51, (DWORD)bits < 0x80000000u, bits);
    SelectObject(mem, bmp);
    LOGBRUSH lb = {0, RGB(255, 0, 0), 0};
    HBRUSH red = CreateBrushIndirect(&lb);
    CHECK(52, red != 0, 0);
    RECT all = {0, 0, 16, 16};
    FillRect(mem, &all, red);
    GdiFlush();
    CHECK(53, (((DWORD *)bits)[5 * 16 + 5] & 0xffffff) == 0xff0000, ((DWORD *)bits)[5 * 16 + 5]);
    CHECK(54, GetPixel(mem, 5, 5) == RGB(255, 0, 0), GetPixel(mem, 5, 5));
    BITMAP bm;
    CHECK(55, GetObjectA(bmp, sizeof bm, &bm) == sizeof bm, GetObjectA(bmp, sizeof bm, &bm));
    CHECK(56, bm.bmWidth == 16 && bm.bmBitsPixel == 32 && bm.bmBits == bits, bm.bmBits);
    CHECK(57, GetObjectA(red, 0, 0) == sizeof(LOGBRUSH), GetObjectA(red, 0, 0));
    /* DIBSECTION x86 (84 octets) : la VCL recopie dsBmih pour recréer la
     * DIB (TBitmap, listes d'images PNG de WinSCP, #905). */
    struct {
        BITMAP dsBm;
        BITMAPINFOHEADER dsBmih;
        DWORD dsBitfields[3];
        HANDLE dshSection;
        DWORD dsOffset;
    } ds;
    int ds_size = GetObjectA(bmp, sizeof ds, &ds);
    CHECK(58, ds_size == 84 && sizeof ds == 84 && ds.dsBm.bmBits == bits && ds.dsBmih.biSize == 40 &&
                  ds.dsBmih.biWidth == 16 && ds.dsBmih.biHeight == -16 && ds.dsBmih.biBitCount == 32,
          ds_size);
    void *bits2 = 0;
    HBITMAP copy = CreateDIBSection(mem, &ds.dsBmih, 0, &bits2, 0, 0);
    CHECK(59, copy != 0 && bits2 != 0, copy);
    DeleteObject(copy);
    DeleteObject(red);
    DeleteDC(mem);
    DeleteObject(bmp);

    /* Boîte de dialogue en mémoire : DLGPROC x86, DWL_USER, DWL_DLGPROC. */
    static const WORD tmpl[] __attribute__((aligned(4))) = {
        /* style WS_POPUP|WS_CAPTION|DS_MODALFRAME, exstyle, 0 contrôle, x, y, cx, cy */
        0x0080, 0x80C0, 0, 0, 0, 10, 10, 100, 50,
        0, 0, /* menu, classe */
        'E', 's', 's', 'a', 'i', 0,
    };
    int r = DialogBoxIndirectParamA(inst, tmpl, hwnd, dlg_proc, 0x5150);
    CHECK(63, r == 42, r);

    if (show) {
        InvalidateRect(hwnd, 0, 1);
        SetTimer(hwnd, 3, 3000, 0);
        while (GetMessageA(&m, 0, 0, 0) > 0) {
            if (m.message == WM_TIMER && m.wParam == 3) break;
            TranslateMessage(&m);
            DispatchMessageA(&m);
        }
    }

    /* Fonctions relevées sur WinSCP 6.7 x86. */
    DWORD res = 0;
    CHECK(75, SendMessageTimeoutA(hwnd, WM_USER, 0, (LPARAM)&marker, 2, 1000, &res) && res == 0x1234, res);
    DWORD sbi[15] = {60};
    CHECK(76, GetScrollBarInfo(hwnd, -5, sbi) && (sbi[9] & 0x8000), sbi[9]);
    DWORD lp[4] = {0, 2, 0, RGB(1, 2, 3)};
    HGDIOBJ pen = CreatePenIndirect(lp);
    CHECK(77, pen != 0, 0);
    DeleteObject(pen);
    HDC sdc = CreateCompatibleDC(0);
    CHECK(78, SetDCBrushColor(sdc, RGB(9, 9, 9)) == RGB(255, 255, 255) && SetDCBrushColor(sdc, 0) == RGB(9, 9, 9), 0);
    DeleteDC(sdc);

    /* 79-80 : sous-classement dès le premier message (VCL, #822). */
    {
        WNDCLASSEXA ic = {0};
        ic.cbSize = sizeof ic;
        ic.lpfnWndProc = init_proc;
        ic.hInstance = inst;
        ic.lpszClassName = "Winlux32Init";
        CHECK(79, RegisterClassExA(&ic) != 0, 0);
        HWND w = CreateWindowExA(0, "Winlux32Init", "", WS_OVERLAPPEDWINDOW, 0, 0, 50, 50, 0, 0, inst, 0);
        CHECK(79, w != 0, 0);
        CHECK(80, init_calls >= 1 && init_saw_create == 0 && real_saw_create == 1,
              init_calls * 256 + init_saw_create * 16 + real_saw_create);
        DestroyWindow(w);
    }

    /* Fin : WM_DESTROY, PostQuitMessage(7), GetMessage rend 0. */
    DestroyWindow(hwnd);
    CHECK(70, destroyed, 0);
    BOOL got;
    while ((got = GetMessageA(&m, 0, 0, 0)) > 0) DispatchMessageA(&m);
    CHECK(71, got == 0 && m.message == 0x0012 && m.wParam == 7, m.wParam);
    CHECK(72, !IsWindow(hwnd), 0);

    /* Casse des tampons (WinSCP.com s'arrêtait sur CharUpperBuffW). */
    unsigned short wtext[] = {'s', 'c', 'p', 0xE9, 0};
    char atext[] = "ftp";
    CHECK(73, CharUpperBuffW(wtext, 4) == 4 && wtext[0] == 'S' && wtext[3] == 0xC9, wtext[3]);
    CHECK(74, CharUpperBuffA(atext, 2) == 2 && atext[0] == 'F' && atext[1] == 'T' && atext[2] == 'p',
          atext[2]);

    say("GUI 32 bits : tout est correct\r\n");
    ExitProcess(0);
}

/* Fenêtre d'un programme 32 bits (x86) agrandie par le gestionnaire de
 * fenêtres sous Winlux. Construit par clang --target=i686-pc-windows-msvc et
 * lld-link, sans CRT.
 * Sous Windows, agrandir une fenêtre passe par SetWindowPos, qui envoie
 * WM_WINDOWPOSCHANGED avant WM_SIZE (Learn, « WM_WINDOWPOSCHANGED » : « The
 * WM_SIZE and WM_MOVE messages are not sent if an application handles the
 * WM_WINDOWPOSCHANGED message without calling DefWindowProc »). Chromium et
 * CEF ne recalculent leur zone cliente que sur ce message (#1476). La
 * procédure x86 doit le recevoir, avec un WINDOWPOS en disposition x86, comme
 * un programme 64 bits.
 * Codes de sortie : 0 tout va bien ; 5 WM_SIZE agrandi sans
 * WM_WINDOWPOSCHANGED avant ; 6 WINDOWPOS x86 invalide ; 7 jamais agrandie
 * (délai de 20 s). Le script de test agrandit la fenêtre par wmctrl. */
typedef unsigned int DWORD, UINT;
typedef int BOOL, LONG;
typedef unsigned short WCHAR;
typedef void *HANDLE, *HWND, *HINSTANCE, *HMENU, *HICON, *HCURSOR, *HBRUSH;
typedef unsigned int WPARAM, UINT_PTR;
typedef long LPARAM, LRESULT;

#define WINAPI __stdcall
#define CALLBACK __stdcall
typedef LRESULT(CALLBACK *WNDPROC)(HWND, UINT, WPARAM, LPARAM);

typedef struct {
    UINT style;
    WNDPROC lpfnWndProc;
    int cbClsExtra, cbWndExtra;
    HINSTANCE hInstance;
    HICON hIcon;
    HCURSOR hCursor;
    HBRUSH hbrBackground;
    const WCHAR *lpszMenuName, *lpszClassName;
} WNDCLASSW;

typedef struct {
    LONG x, y;
} POINT;

typedef struct {
    HWND hwnd;
    UINT message;
    WPARAM wParam;
    LPARAM lParam;
    DWORD time;
    POINT pt;
} MSG;

typedef struct {
    HWND hwnd, hwndInsertAfter;
    int x, y, cx, cy;
    UINT flags;
} WINDOWPOS;

__declspec(dllimport) void WINAPI ExitProcess(DWORD);
__declspec(dllimport) HINSTANCE WINAPI GetModuleHandleW(const WCHAR *);
__declspec(dllimport) unsigned short WINAPI RegisterClassW(const WNDCLASSW *);
__declspec(dllimport) HWND WINAPI CreateWindowExW(DWORD, const WCHAR *, const WCHAR *, DWORD, int, int,
                                                 int, int, HWND, HMENU, HINSTANCE, void *);
__declspec(dllimport) LRESULT WINAPI DefWindowProcW(HWND, UINT, WPARAM, LPARAM);
__declspec(dllimport) BOOL WINAPI ShowWindow(HWND, int);
__declspec(dllimport) BOOL WINAPI GetMessageW(MSG *, HWND, UINT, UINT);
__declspec(dllimport) LRESULT WINAPI DispatchMessageW(const MSG *);
__declspec(dllimport) UINT_PTR WINAPI SetTimer(HWND, UINT_PTR, UINT, void *);

#define WM_SIZE 0x0005
#define WM_TIMER 0x0113
#define WM_WINDOWPOSCHANGED 0x0047
#define WS_OVERLAPPEDWINDOW 0x00CF0000
#define SW_SHOW 5
#define SWP_NOSIZE 0x0001
#define GRANDE 1000

static int pos_grande;

static LRESULT CALLBACK proc(HWND h, UINT m, WPARAM w, LPARAM l) {
    if (m == WM_WINDOWPOSCHANGED) {
        WINDOWPOS *p = (WINDOWPOS *)l;
        if (!p || p->hwnd != h)
            ExitProcess(6);
        if (!(p->flags & SWP_NOSIZE) && p->cx >= GRANDE)
            pos_grande = 1;
    } else if (m == WM_SIZE && (int)(l & 0xFFFF) >= GRANDE) {
        ExitProcess(pos_grande ? 0 : 5);
    } else if (m == WM_TIMER) {
        ExitProcess(7);
    }
    return DefWindowProcW(h, m, w, l);
}

void entry(void) {
    static const WCHAR cls[] = {'A', 'g', 'r', 'a', 'n', 'd', 'i', 'e', '3', '2', 0};
    WNDCLASSW wc = {0};
    MSG msg;
    HWND hwnd;
    wc.lpfnWndProc = proc;
    wc.hInstance = GetModuleHandleW(0);
    wc.lpszClassName = cls;
    if (!RegisterClassW(&wc))
        ExitProcess(1);
    hwnd = CreateWindowExW(0, cls, cls, WS_OVERLAPPEDWINDOW, 100, 100, 400, 300, 0, 0, wc.hInstance, 0);
    if (!hwnd)
        ExitProcess(2);
    ShowWindow(hwnd, SW_SHOW);
    SetTimer(hwnd, 1, 20000, 0);
    while (GetMessageW(&msg, 0, 0, 0) > 0)
        DispatchMessageW(&msg);
    ExitProcess(8);
}

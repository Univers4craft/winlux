/* Cadre maison d'un programme 32 bits (x86) sous Winlux.
 * Construit par clang --target=i686-pc-windows-msvc et lld-link, sans CRT.
 * Une fenêtre qui dessine sa propre barre de titre traite WM_NCCALCSIZE
 * (wParam TRUE) sans DefWindowProc et garde tout le rectangle pour sa zone
 * cliente (Learn, « WM_NCCALCSIZE » et « Custom Window Frame Using DWM »).
 * SetWindowPos(SWP_FRAMECHANGED) doit lui envoyer ce message (Learn,
 * « SetWindowPos » : SWP_FRAMECHANGED « sends a WM_NCCALCSIZE message to
 * the window »), comme à un programme 64 bits, avec un NCCALCSIZE_PARAMS
 * en disposition x86 : trois RECT puis un pointeur 32 bits de WINDOWPOS.
 * Rend 0 si tout va bien, sinon le numéro de la vérification en échec. */
typedef unsigned int DWORD, UINT;
typedef int BOOL, LONG;
typedef unsigned short WCHAR;
typedef void *HANDLE, *HWND, *HINSTANCE, *HMENU, *HICON, *HCURSOR, *HBRUSH;
typedef unsigned int WPARAM;
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
    LONG left, top, right, bottom;
} RECT;

typedef struct {
    HWND hwnd, hwndInsertAfter;
    int x, y, cx, cy;
    UINT flags;
} WINDOWPOS;

typedef struct {
    RECT rgrc[3];
    WINDOWPOS *lppos;
} NCCALCSIZE_PARAMS;

__declspec(dllimport) void WINAPI ExitProcess(DWORD);
__declspec(dllimport) HINSTANCE WINAPI GetModuleHandleW(const WCHAR *);
__declspec(dllimport) unsigned short WINAPI RegisterClassW(const WNDCLASSW *);
__declspec(dllimport) HWND WINAPI CreateWindowExW(DWORD, const WCHAR *, const WCHAR *, DWORD, int, int,
                                                 int, int, HWND, HMENU, HINSTANCE, void *);
__declspec(dllimport) LRESULT WINAPI DefWindowProcW(HWND, UINT, WPARAM, LPARAM);
__declspec(dllimport) BOOL WINAPI ShowWindow(HWND, int);
__declspec(dllimport) BOOL WINAPI SetWindowPos(HWND, HWND, int, int, int, int, UINT);
__declspec(dllimport) BOOL WINAPI DestroyWindow(HWND);

#define WM_NCCALCSIZE 0x0083
#define WM_NCHITTEST 0x0084
#define HTCAPTION 2
#define WS_OVERLAPPEDWINDOW 0x00CF0000
#define SW_SHOW 5
#define SWP_NOSIZE 0x0001
#define SWP_NOMOVE 0x0002
#define SWP_NOZORDER 0x0004
#define SWP_FRAMECHANGED 0x0020

static int recus;
static int params_valides;

static LRESULT CALLBACK proc(HWND h, UINT m, WPARAM w, LPARAM l) {
    if (m == WM_NCCALCSIZE && w) {
        NCCALCSIZE_PARAMS *p = (NCCALCSIZE_PARAMS *)l;
        recus++;
        /* Rectangle proposé non vide, et WINDOWPOS de cette fenêtre. */
        if (p && p->rgrc[0].right > p->rgrc[0].left && p->rgrc[0].bottom > p->rgrc[0].top &&
            (!p->lppos || p->lppos->hwnd == h))
            params_valides++;
        /* Toute la fenêtre reste zone cliente : cadre maison. */
        return 0;
    }
    if (m == WM_NCHITTEST)
        return HTCAPTION;
    return DefWindowProcW(h, m, w, l);
}

void entry(void) {
    static const WCHAR cls[] = {'C', 'a', 'd', 'r', 'e', '3', '2', 0};
    static const WCHAR titre[] = {'C', 'a', 'd', 'r', 'e', ' ', 'm', 'a', 'i', 's', 'o', 'n', ' ', '3', '2', 0};
    WNDCLASSW wc = {0};
    HWND hwnd;
    wc.lpfnWndProc = proc;
    wc.hInstance = GetModuleHandleW(0);
    wc.lpszClassName = cls;
    if (!RegisterClassW(&wc))
        ExitProcess(1);
    hwnd = CreateWindowExW(0, cls, titre, WS_OVERLAPPEDWINDOW, 100, 100, 400, 300, 0, 0, wc.hInstance, 0);
    if (!hwnd)
        ExitProcess(2);
    ShowWindow(hwnd, SW_SHOW);
    recus = params_valides = 0;
    SetWindowPos(hwnd, 0, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED);
    if (recus == 0)
        ExitProcess(3);
    if (params_valides != recus)
        ExitProcess(4);
    DestroyWindow(hwnd);
    ExitProcess(0);
}

/* Sous-classement comctl32 d'un programme 32 bits (x86) sous Winlux (#2876).
 * Construit par clang --target=i686-pc-windows-msvc et lld-link, sans CRT.
 * SetWindowSubclass, GetWindowSubclass, RemoveWindowSubclass et
 * DefSubclassProc sont importés par numéro d'ordre (410 à 413), comme le
 * fait Notepad++ : la SUBCLASSPROC x86 doit recevoir hwnd, message,
 * wParam, lParam, uIdSubclass et dwRefData, et non être appelée comme du
 * code 64 bits. Rend 0 si tout va bien, sinon le numéro de la vérification
 * qui a échoué. */
typedef unsigned int DWORD, UINT, UINT_PTR, DWORD_PTR;
typedef int BOOL;
typedef unsigned short WCHAR;
typedef void *HANDLE, *HWND, *HINSTANCE, *HMENU;
typedef unsigned int WPARAM;
typedef long LPARAM, LRESULT;

#define WINAPI __stdcall
#define CALLBACK __stdcall
typedef LRESULT(CALLBACK *SUBCLASSPROC)(HWND, UINT, WPARAM, LPARAM, UINT_PTR, DWORD_PTR);

__declspec(dllimport) void WINAPI ExitProcess(DWORD);
__declspec(dllimport) HINSTANCE WINAPI GetModuleHandleW(const WCHAR *);
__declspec(dllimport) HWND WINAPI CreateWindowExW(DWORD, const WCHAR *, const WCHAR *, DWORD, int, int,
                                                 int, int, HWND, HMENU, HINSTANCE, void *);
__declspec(dllimport) BOOL WINAPI DestroyWindow(HWND);
__declspec(dllimport) LRESULT WINAPI SendMessageW(HWND, UINT, WPARAM, LPARAM);
__declspec(dllimport) int WINAPI GetWindowTextW(HWND, WCHAR *, int);
__declspec(dllimport) BOOL WINAPI SetWindowSubclass(HWND, SUBCLASSPROC, UINT_PTR, DWORD_PTR);
__declspec(dllimport) BOOL WINAPI GetWindowSubclass(HWND, SUBCLASSPROC, UINT_PTR, DWORD_PTR *);
__declspec(dllimport) BOOL WINAPI RemoveWindowSubclass(HWND, SUBCLASSPROC, UINT_PTR);
__declspec(dllimport) LRESULT WINAPI DefSubclassProc(HWND, UINT, WPARAM, LPARAM);

#define WM_SETTEXT 0x000C
#define WM_APP 0x8000
#define TCM_SETIMAGELIST 0x1303

static int calls;
static UINT last_msg;
static WPARAM last_wp;
static LPARAM last_lp;
static UINT_PTR last_id;
static DWORD_PTR last_ref;
static HWND last_hwnd;

static LRESULT CALLBACK sous_classe(HWND h, UINT m, WPARAM w, LPARAM l, UINT_PTR id, DWORD_PTR ref) {
    if (m == WM_APP || m == TCM_SETIMAGELIST) {
        calls++;
        last_hwnd = h;
        last_msg = m;
        last_wp = w;
        last_lp = l;
        last_id = id;
        last_ref = ref;
        if (m == WM_APP)
            return 0x1234;
        return 0;
    }
    return DefSubclassProc(h, m, w, l);
}

void entry(void) {
    static const WCHAR cls[] = {'S', 'T', 'A', 'T', 'I', 'C', 0};
    static const WCHAR titre[] = {'x', 0};
    static const WCHAR abc[] = {'a', 'b', 'c', 0};
    WCHAR lu[8];
    DWORD_PTR ref = 0;
    int avant;
    HWND hwnd = CreateWindowExW(0, cls, titre, 0, 0, 0, 10, 10, 0, 0, GetModuleHandleW(0), 0);
    if (!hwnd)
        ExitProcess(1);
    if (!SetWindowSubclass(hwnd, sous_classe, 0x7c0, 0x5150))
        ExitProcess(2);
    if (SendMessageW(hwnd, WM_APP, 7, 9) != 0x1234)
        ExitProcess(3);
    if (calls != 1 || last_hwnd != hwnd || last_wp != 7 || last_lp != 9 || last_id != 0x7c0 ||
        last_ref != 0x5150)
        ExitProcess(4);
    /* Le message qui faisait planter Notepad++ (#2876). */
    SendMessageW(hwnd, TCM_SETIMAGELIST, 0, 0x210985);
    if (calls != 2 || last_msg != TCM_SETIMAGELIST || last_lp != 0x210985)
        ExitProcess(5);
    if (!GetWindowSubclass(hwnd, sous_classe, 0x7c0, &ref) || ref != 0x5150)
        ExitProcess(6);
    /* Même procédure, même identifiant : seule la donnée change. */
    if (!SetWindowSubclass(hwnd, sous_classe, 0x7c0, 0x6000))
        ExitProcess(7);
    SendMessageW(hwnd, WM_APP, 0, 0);
    if (calls != 3 || last_ref != 0x6000)
        ExitProcess(8);
    /* DefSubclassProc rend le message au contrôle d'origine. */
    SendMessageW(hwnd, WM_SETTEXT, 0, (LPARAM)abc);
    if (GetWindowTextW(hwnd, lu, 8) != 3 || lu[0] != 'a' || lu[2] != 'c')
        ExitProcess(9);
    if (!RemoveWindowSubclass(hwnd, sous_classe, 0x7c0))
        ExitProcess(10);
    avant = calls;
    SendMessageW(hwnd, WM_APP, 0, 0);
    if (calls != avant)
        ExitProcess(11);
    if (GetWindowSubclass(hwnd, sous_classe, 0x7c0, &ref) || ref != 0)
        ExitProcess(12);
    DestroyWindow(hwnd);
    ExitProcess(0);
}

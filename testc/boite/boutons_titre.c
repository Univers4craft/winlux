/* #3064 : trois fenêtres de premier niveau dont la barre de titre doit
 * montrer les boutons Réduire et Agrandir selon leur style (Learn, « Window
 * Styles » : WS_MINIMIZEBOX et WS_MAXIMIZEBOX, avec WS_SYSMENU). Ferme tout
 * au bout de 20 s. */
#include <windows.h>

static LRESULT CALLBACK proc(HWND h, UINT m, WPARAM w, LPARAM l)
{
    if (m == WM_TIMER) {
        PostQuitMessage(0);
        return 0;
    }
    return DefWindowProcW(h, m, w, l);
}

static HWND fenetre(const wchar_t *titre, DWORD style, int x)
{
    HWND h = CreateWindowExW(0, L"BoutonsTitre", titre, style, x, 40, 300, 150,
                             NULL, NULL, GetModuleHandleW(NULL), NULL);
    ShowWindow(h, SW_SHOW);
    return h;
}

int main(void)
{
    WNDCLASSW wc = {0};
    MSG msg;
    HWND tous;
    wc.lpfnWndProc = proc;
    wc.hInstance = GetModuleHandleW(NULL);
    wc.lpszClassName = L"BoutonsTitre";
    wc.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
    RegisterClassW(&wc);
    /* Réduire, Agrandir et Fermer. */
    tous = fenetre(L"BoutonsTous", WS_OVERLAPPEDWINDOW, 20);
    /* Boîte de message ou dialogue fixe : Fermer seul. */
    fenetre(L"BoutonsFixe", WS_POPUP | WS_CAPTION | WS_SYSMENU, 340);
    /* Cadre épais et Réduire, sans Agrandir. */
    fenetre(L"BoutonsReduire",
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX,
            660);
    SetTimer(tous, 1, 20000, NULL);
    while (GetMessageW(&msg, NULL, 0, 0) > 0) {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    return 0;
}

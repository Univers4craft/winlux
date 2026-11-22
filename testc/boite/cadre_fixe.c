/* #3011 : deux fenêtres de premier niveau, l'une à cadre fixe (barre de
 * titre sans WS_THICKFRAME ni WS_MAXIMIZEBOX, comme une boîte de message),
 * l'autre ordinaire (WS_OVERLAPPEDWINDOW). Ferme tout au bout de 20 s. */
#include <windows.h>

static LRESULT CALLBACK proc(HWND h, UINT m, WPARAM w, LPARAM l)
{
    if (m == WM_TIMER) {
        PostQuitMessage(0);
        return 0;
    }
    return DefWindowProcW(h, m, w, l);
}

int main(void)
{
    WNDCLASSW wc = {0};
    MSG msg;
    HWND fixe, normale;
    wc.lpfnWndProc = proc;
    wc.hInstance = GetModuleHandleW(NULL);
    wc.lpszClassName = L"CadreFixe3011";
    wc.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
    RegisterClassW(&wc);
    fixe = CreateWindowExW(0, wc.lpszClassName, L"CadreFixe",
                           WS_POPUP | WS_CAPTION | WS_SYSMENU, 40, 40, 320, 160,
                           NULL, NULL, wc.hInstance, NULL);
    normale = CreateWindowExW(0, wc.lpszClassName, L"CadreNormal",
                              WS_OVERLAPPEDWINDOW, 420, 40, 320, 160, NULL, NULL,
                              wc.hInstance, NULL);
    ShowWindow(fixe, SW_SHOW);
    ShowWindow(normale, SW_SHOW);
    SetTimer(fixe, 1, 20000, NULL);
    while (GetMessageW(&msg, NULL, 0, 0) > 0) {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    return 0;
}

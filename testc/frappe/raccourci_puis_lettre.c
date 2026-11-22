/* #2870 : un raccourci Ctrl+N pris par TranslateAccelerator, puis « n »
   tapé, doit donner la lettre. Une fenêtre principale avec une table
   d'accélérateurs (Ctrl+N) et une zone enfant qui a le focus ; la boucle de
   messages est celle de Notepad++ : TranslateAccelerator, sinon
   TranslateMessage et DispatchMessage. La zone compte les « n » reçus et
   termine sur « q ». Code de sortie : 10 × nombre de « n » + nombre de
   commandes Ctrl+N. Attendu après « Ctrl+N », puis « nnnq » : 31. */
#include <windows.h>
static HWND hMain, hZone;
static int lettres, commandes;
static LRESULT CALLBACK ZoneProc(HWND h, UINT m, WPARAM w, LPARAM l)
{
    if (m == WM_CHAR) {
        if (w == 'n') lettres++;
        if (w == 'q') PostQuitMessage(10 * lettres + commandes);
        return 0;
    }
    return DefWindowProcW(h, m, w, l);
}
static LRESULT CALLBACK MainProc(HWND h, UINT m, WPARAM w, LPARAM l)
{
    if (m == WM_COMMAND && LOWORD(w) == 100) { commandes++; return 0; }
    if (m == WM_SETFOCUS) { SetFocus(hZone); return 0; }
    if (m == WM_DESTROY) { PostQuitMessage(99); return 0; }
    return DefWindowProcW(h, m, w, l);
}
int WINAPI WinMain(HINSTANCE hi, HINSTANCE p, LPSTR c, int s)
{
    WNDCLASSW wc = {0};
    wc.lpfnWndProc = MainProc; wc.hInstance = hi; wc.lpszClassName = L"RaccourciMain";
    wc.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
    RegisterClassW(&wc);
    wc.lpfnWndProc = ZoneProc; wc.lpszClassName = L"RaccourciZone";
    RegisterClassW(&wc);
    hMain = CreateWindowExW(0, L"RaccourciMain", L"Raccourci2870", WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                            50, 50, 400, 200, 0, 0, hi, 0);
    hZone = CreateWindowExW(0, L"RaccourciZone", L"", WS_CHILD | WS_VISIBLE, 10, 10, 360, 140, hMain, 0, hi, 0);
    SetFocus(hZone);
    ACCEL a = {FVIRTKEY | FCONTROL, 'N', 100};
    HACCEL acc = CreateAcceleratorTableW(&a, 1);
    MSG msg;
    while (GetMessageW(&msg, 0, 0, 0) > 0) {
        if (!TranslateAcceleratorW(hMain, acc, &msg)) {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    return (int)msg.wParam;
}

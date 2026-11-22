/* #3072 : le « . » tapé juste avant Entrée doit arriver avant elle quand
   Entrée est déjà en file au moment de TranslateMessage. La zone lit Entrée
   au WM_KEYDOWN et les caractères au WM_CHAR, comme Scintilla. Elle dort
   au WM_KEYDOWN du « a » pour que « . », Entrée et « q » arrivent ensemble.
   Ordre écrit dans ordre.txt ; code de sortie 7 si « a.| », 3 sinon. */
#include <windows.h>
#include <stdio.h>
static char ordre[64];
static int n;
static void note(char c) { if (n < 63) ordre[n++] = c; }
static LRESULT CALLBACK ZoneProc(HWND h, UINT m, WPARAM w, LPARAM l)
{
    if (m == WM_KEYDOWN) {
        if (w == 'A') Sleep(800);
        if (w == VK_RETURN) note('|');
        return 0;
    }
    if (m == WM_CHAR) {
        if (w == 'q') {
            FILE *f = fopen("ordre.txt", "w");
            if (f) { fputs(ordre, f); fclose(f); }
            PostQuitMessage(lstrcmpA(ordre, "a.|") == 0 ? 7 : 3);
        } else if (w >= 32) note((char)w);
        return 0;
    }
    return DefWindowProcW(h, m, w, l);
}
static HWND hZone;
static LRESULT CALLBACK MainProc(HWND h, UINT m, WPARAM w, LPARAM l)
{
    if (m == WM_SETFOCUS) { SetFocus(hZone); return 0; }
    if (m == WM_DESTROY) { PostQuitMessage(99); return 0; }
    return DefWindowProcW(h, m, w, l);
}
int WINAPI WinMain(HINSTANCE hi, HINSTANCE p, LPSTR c, int s)
{
    WNDCLASSW wc = {0};
    wc.lpfnWndProc = MainProc; wc.hInstance = hi; wc.lpszClassName = L"PointMain";
    wc.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
    RegisterClassW(&wc);
    wc.lpfnWndProc = ZoneProc; wc.lpszClassName = L"PointZone";
    RegisterClassW(&wc);
    HWND hMain = CreateWindowExW(0, L"PointMain", L"PointEntree3072", WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                                 50, 50, 400, 200, 0, 0, hi, 0);
    hZone = CreateWindowExW(0, L"PointZone", L"", WS_CHILD | WS_VISIBLE, 10, 10, 360, 140, hMain, 0, hi, 0);
    SetFocus(hZone);
    MSG msg;
    while (GetMessageW(&msg, 0, 0, 0) > 0) {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    return (int)msg.wParam;
}

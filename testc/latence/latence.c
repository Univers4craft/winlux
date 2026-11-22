/* Sonde de latence (Réduire le temps de réponse des applications).
 *
 * Une fenêtre Win32 ordinaire, sans rien d'autre : elle se peint d'une
 * couleur unie et change de couleur à chaque touche et à chaque clic. La
 * mesure se fait DEHORS, côté X11 (scripts/mesure-latence.py) : instant du
 * lancement -> première fenêtre peinte de la bonne couleur, puis instant de
 * la touche ou du clic envoyé par XTEST -> pixel de la nouvelle couleur.
 * Le programme n'écrit aucun chiffre lui-même : une valeur écrite par la
 * sonde ne serait pas une mesure.
 *
 * Couleurs : 0 = rouge (premier affichage), puis alternance vert / bleu.
 * Échap ou WM_CLOSE : sortie avec le code 0.
 * Compilé par MinGW-w64 (x86_64 et i686), code écrit pour Winlux. */
#include <windows.h>

static int etat;
static const COLORREF couleurs[3] = {RGB(255, 0, 0), RGB(0, 255, 0), RGB(0, 0, 255)};

static LRESULT CALLBACK proc(HWND w, UINT m, WPARAM wp, LPARAM lp)
{
    switch (m) {
    case WM_PAINT: {
        PAINTSTRUCT ps;
        HDC dc = BeginPaint(w, &ps);
        HBRUSH b = CreateSolidBrush(couleurs[etat]);
        RECT r;
        GetClientRect(w, &r);
        FillRect(dc, &r, b);
        DeleteObject(b);
        EndPaint(w, &ps);
        return 0;
    }
    case WM_KEYDOWN:
        if (wp == VK_ESCAPE) {
            DestroyWindow(w);
            return 0;
        }
        /* fall through */
    case WM_LBUTTONDOWN:
        etat = etat == 1 ? 2 : 1;
        InvalidateRect(w, NULL, FALSE);
        return 0;
    case WM_DESTROY:
        PostQuitMessage(0);
        return 0;
    }
    return DefWindowProcW(w, m, wp, lp);
}

int WINAPI WinMain(HINSTANCE inst, HINSTANCE prev, LPSTR cmd, int show)
{
    (void)prev;
    (void)cmd;
    WNDCLASSW wc = {0};
    wc.lpfnWndProc = proc;
    wc.hInstance = inst;
    wc.hCursor = LoadCursorW(NULL, (LPCWSTR)IDC_ARROW);
    wc.lpszClassName = L"WinluxSondeLatence";
    if (!RegisterClassW(&wc))
        return 1;
    HWND w = CreateWindowExW(0, wc.lpszClassName, L"Sonde de latence", WS_OVERLAPPEDWINDOW,
                             40, 40, 320, 240, NULL, NULL, inst, NULL);
    if (!w)
        return 2;
    ShowWindow(w, show);
    UpdateWindow(w);
    MSG msg;
    while (GetMessageW(&msg, NULL, 0, 0) > 0) {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    return (int)msg.wParam;
}

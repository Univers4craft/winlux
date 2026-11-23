/* #3170 : MsgWaitForMultipleObjects dort jusqu'au signal, sans se réveiller
   toutes les 10 ms. 1) 3 s d'attente sans rien (le script compte les réveils
   pendant ce temps) ; 2) un événement signalé par un autre fil après 300 ms
   rend WAIT_OBJECT_0 ; 3) un message posté par un autre fil après 300 ms
   rend WAIT_OBJECT_0 + 1. Chaque réveil doit arriver en moins de 150 ms
   après le signal. Code de sortie : 0 si tout va bien, sinon le numéro de
   l'étape en échec (10 + n pour un retard). */
#include <windows.h>
static HANDLE ev;
static DWORD main_tid;
static DWORD WINAPI signale(LPVOID p) { Sleep(300); SetEvent(ev); return 0; }
static DWORD WINAPI poste(LPVOID p) { Sleep(300); PostThreadMessageW(main_tid, WM_USER, 0, 0); return 0; }
int WINAPI WinMain(HINSTANCE hi, HINSTANCE p, LPSTR c, int s)
{
    WNDCLASSW wc = {0}; wc.lpfnWndProc = DefWindowProcW; wc.hInstance = hi; wc.lpszClassName = L"Attente3170";
    RegisterClassW(&wc);
    CreateWindowExW(0, L"Attente3170", L"Attente3170", WS_OVERLAPPEDWINDOW | WS_VISIBLE, 0, 0, 200, 100, 0, 0, hi, 0);
    MSG msg;
    while (PeekMessageW(&msg, 0, 0, 0, PM_REMOVE)) DispatchMessageW(&msg);
    main_tid = GetCurrentThreadId();
    ev = CreateEventW(0, TRUE, FALSE, 0);
    if (MsgWaitForMultipleObjects(1, &ev, FALSE, 3000, QS_ALLINPUT) != WAIT_TIMEOUT) return 1;
    DWORD t0 = GetTickCount();
    CloseHandle(CreateThread(0, 0, signale, 0, 0, 0));
    if (MsgWaitForMultipleObjects(1, &ev, FALSE, 5000, QS_ALLINPUT) != WAIT_OBJECT_0) return 2;
    if (GetTickCount() - t0 > 450) return 12;
    ResetEvent(ev);
    while (PeekMessageW(&msg, 0, 0, 0, PM_REMOVE)) DispatchMessageW(&msg);
    t0 = GetTickCount();
    CloseHandle(CreateThread(0, 0, poste, 0, 0, 0));
    DWORD r;
    do {
        r = MsgWaitForMultipleObjects(1, &ev, FALSE, 5000, QS_ALLINPUT);
        if (r != WAIT_OBJECT_0 + 1) return 3;
    } while (!PeekMessageW(&msg, 0, WM_USER, WM_USER, PM_REMOVE) && GetTickCount() - t0 < 5000);
    if (msg.message != WM_USER) return 3;
    if (GetTickCount() - t0 > 450) return 13;
    return 0;
}

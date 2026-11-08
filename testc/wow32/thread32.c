/* Threads 32 bits (#232, étape 4) : CreateThread et _beginthreadex vers du
 * code x86, TLS statique (_Thread_local, compilé par clang) ou émulé (gcc),
 * 300 threads courts (entrées de LDT recyclées), rappel x86 appelé par une
 * API (EnumSystemLocalesW), GetProcAddress vers une fonction intégrée. */
#include <windows.h>
#include <process.h>
#include <stdio.h>

static _Thread_local int tls_value = 7;
static volatile LONG counter;
static int fails;
#define CHECK(n, c) do { if (!(c)) { printf("echec %d : %s\n", n, #c); fails = n; } } while (0)

static DWORD WINAPI worker(LPVOID p) {
    int id = (int)(INT_PTR)p;
    CHECK(1, tls_value == 7);
    tls_value = 100 + id;
    for (int i = 0; i < 1000; i++) InterlockedIncrement(&counter);
    Sleep(10);
    CHECK(2, tls_value == 100 + id);
    return 40 + id;
}
static unsigned __stdcall worker2(void *p) { return (unsigned)(INT_PTR)p * 2; }
/* File de minuteurs (#1226 : installeur Creative Cloud). */
static volatile LONG ticks;
static HANDLE tick_event;
static void CALLBACK tick_cb(PVOID p, BOOLEAN fired) {
    CHECK(10, p == (PVOID)0x1226 && fired);
    if (InterlockedIncrement(&ticks) == 3) SetEvent(tick_event);
}
static BOOL CALLBACK locale_cb(LPWSTR s) { (void)s; InterlockedIncrement(&counter); return FALSE; }

int main(void) {
    HANDLE h[4];
    for (int i = 0; i < 4; i++) h[i] = CreateThread(NULL, 0, worker, (LPVOID)(INT_PTR)i, 0, NULL);
    WaitForMultipleObjects(4, h, TRUE, INFINITE);
    for (int i = 0; i < 4; i++) { DWORD code = 0; GetExitCodeThread(h[i], &code); CHECK(3, code == (DWORD)(40 + i)); CloseHandle(h[i]); }
    CHECK(4, counter == 4000);
    CHECK(5, tls_value == 7);
    HANDLE t = (HANDLE)_beginthreadex(NULL, 0, worker2, (void *)21, 0, NULL);
    WaitForSingleObject(t, INFINITE);
    DWORD code = 0; GetExitCodeThread(t, &code); CHECK(6, code == 42); CloseHandle(t);
    /* Beaucoup de threads courts : entrées de LDT recyclées. */
    for (int i = 0; i < 300; i++) { HANDLE x = CreateThread(NULL, 0, worker, (LPVOID)(INT_PTR)i, 0, NULL); WaitForSingleObject(x, INFINITE); CloseHandle(x); }
    LONG before = counter;
    EnumSystemLocalesW(locale_cb, 2);
    CHECK(7, counter == before + 1);
    HMODULE k = GetModuleHandleA("kernel32.dll");
    DWORD (WINAPI *gtc)(void) = (DWORD (WINAPI *)(void))GetProcAddress(k, "GetTickCount");
    CHECK(8, gtc != NULL && gtc() != 0);
    HANDLE q = CreateTimerQueue();
    CHECK(9, q != NULL);
    tick_event = CreateEventW(NULL, TRUE, FALSE, NULL);
    HANDLE tm = NULL;
    CHECK(11, CreateTimerQueueTimer(&tm, q, tick_cb, (PVOID)0x1226, 5, 5, 0) && tm != NULL);
    CHECK(12, WaitForSingleObject(tick_event, 5000) == WAIT_OBJECT_0);
    CHECK(13, ChangeTimerQueueTimer(q, tm, 60000, 0));
    CHECK(14, DeleteTimerQueueEx(q, INVALID_HANDLE_VALUE));
    LONG frozen = ticks;
    Sleep(30);
    CHECK(15, ticks == frozen);
    CloseHandle(tick_event);
    if (!fails) printf("Threads 32 bits : tout est correct\n");
    return fails;
}

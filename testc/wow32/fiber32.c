#define _WIN32_WINNT 0x0601
/* Fibres x86 (#232, étape 7 ; WinSCP s'en sert) : ConvertThreadToFiber,
 * CreateFiber(Ex), SwitchToFiber aller-retour, GetFiberData,
 * GetCurrentFiber, IsThreadAFiber, pile propre à chaque fibre, fin de la
 * routine qui termine le thread, ConvertFiberToThread, DeleteFiber. */
#include <windows.h>
#include <stdio.h>

static LPVOID main_fiber, worker, second;
static volatile int steps;
static volatile unsigned worker_stack, main_stack;

static int fail(int n, const char *what)
{
    printf("echec %d : %s\n", n, what);
    fflush(stdout);
    ExitProcess(n);
    return n;
}

static void WINAPI worker_proc(LPVOID param)
{
    volatile int local = 0;
    worker_stack = (unsigned)(ULONG_PTR)&local;
    if (param != (LPVOID)0x1234)
        fail(10, "paramètre de la routine");
    if (GetFiberData() != (LPVOID)0x1234)
        fail(11, "GetFiberData dans la fibre");
    if (GetCurrentFiber() != worker)
        fail(12, "GetCurrentFiber dans la fibre");
    for (int i = 0; i < 3; i++) {
        steps += 10;
        local += 1;
        SwitchToFiber(main_fiber);
    }
    if (local != 3)
        fail(13, "variable locale de la fibre perdue");
    steps += 1000;
    SwitchToFiber(main_fiber);
    fail(14, "fibre reprise après la fin");
}

static void WINAPI second_proc(LPVOID param)
{
    double x = 1.5;
    char buf[64];
    snprintf(buf, sizeof buf, "%.1f", x * (int)(ULONG_PTR)param);
    if (lstrcmpA(buf, "4.5") != 0)
        fail(20, "printf dans une fibre");
    steps = -1;
    SwitchToFiber(main_fiber);
}

/* Fibre dont la routine se termine : le thread se termine avec elle. */
static void WINAPI ending_proc(LPVOID param)
{
    (void)param;
}

static DWORD WINAPI thread_proc(LPVOID param)
{
    (void)param;
    LPVOID me = ConvertThreadToFiber(NULL);
    if (!me)
        return 30;
    LPVOID f = CreateFiber(0, ending_proc, NULL);
    if (!f)
        return 31;
    SwitchToFiber(f);
    return 32; /* jamais atteint */
}

int main(void)
{
    if (IsThreadAFiber())
        fail(1, "IsThreadAFiber avant conversion");
    main_fiber = ConvertThreadToFiber((LPVOID)0x77);
    if (!main_fiber)
        fail(2, "ConvertThreadToFiber");
    if (!IsThreadAFiber() || GetCurrentFiber() != main_fiber || GetFiberData() != (LPVOID)0x77)
        fail(3, "fibre principale");
    if (ConvertThreadToFiber(NULL) != NULL || GetLastError() != ERROR_ALREADY_FIBER)
        fail(4, "double conversion refusée");
    worker = CreateFiberEx(0, 256 * 1024, 0, worker_proc, (LPVOID)0x1234);
    if (!worker)
        fail(5, "CreateFiberEx");
    volatile int here = 0;
    main_stack = (unsigned)(ULONG_PTR)&here;
    for (int i = 1; i <= 3; i++) {
        SwitchToFiber(worker);
        if (steps != 10 * i)
            fail(6, "aller-retour dans la fibre");
        if (GetCurrentFiber() != main_fiber)
            fail(7, "fibre courante après retour");
    }
    unsigned d = worker_stack > main_stack ? worker_stack - main_stack : main_stack - worker_stack;
    if (d < 4096)
        fail(8, "la fibre n'a pas sa propre pile");
    SwitchToFiber(worker);
    if (steps != 1030)
        fail(9, "fin de la boucle de la fibre");
    DeleteFiber(worker);

    second = CreateFiber(64 * 1024, second_proc, (LPVOID)3);
    SwitchToFiber(second);
    if (steps != -1)
        fail(21, "seconde fibre");
    DeleteFiber(second);

    HANDLE t = CreateThread(NULL, 0, thread_proc, NULL, 0, NULL);
    if (!t || WaitForSingleObject(t, 10000) != WAIT_OBJECT_0)
        fail(33, "thread à fibres");
    DWORD code = 99;
    GetExitCodeThread(t, &code);
    /* Code de sortie non défini (routine VOID) ; seul compte que le thread
     * se termine sans revenir dans thread_proc ni planter. */
    if (code == 32 || code >= 0xC0000000u)
        fail(34, "fin de routine de fibre = fin du thread");
    CloseHandle(t);

    if (!ConvertFiberToThread() || IsThreadAFiber())
        fail(40, "ConvertFiberToThread");
    printf("Fibres 32 bits : tout est correct\n");
    return 0;
}

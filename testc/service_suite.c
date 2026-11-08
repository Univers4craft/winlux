/* Vrai service Windows en Unicode (#17), compilé par MinGW-w64.
 *
 *   service_suite.exe            installe le service (CreateServiceW), le
 *                                démarre (StartServiceW), attend RUNNING,
 *                                l'arrête (ControlService), le supprime ;
 *   service_suite.exe run        mode service : StartServiceCtrlDispatcherW,
 *                                RegisterServiceCtrlHandlerExW, SetServiceStatus.
 *
 * Code de sortie 0 et « Service Windows : tout est correct » si tout va bien.
 */
#include <windows.h>
#include <stdio.h>
#include <wchar.h>

#define SVC_NAME L"WinluxServiceSuite"

static SERVICE_STATUS_HANDLE status_handle;
static SERVICE_STATUS status;
static HANDLE stop_event;
static int failures;

static void check(int ok, const char *what)
{
    if (!ok) {
        printf("ÉCHEC : %s (erreur %lu)\n", what, GetLastError());
        failures++;
    }
}

static DWORD WINAPI handler(DWORD control, DWORD type, LPVOID data, LPVOID context)
{
    (void)type;
    (void)data;
    if (context != (LPVOID)0x5eed)
        return ERROR_INVALID_PARAMETER;
    if (control == SERVICE_CONTROL_STOP) {
        status.dwCurrentState = SERVICE_STOP_PENDING;
        SetServiceStatus(status_handle, &status);
        SetEvent(stop_event);
    }
    return NO_ERROR;
}

static void WINAPI service_main(DWORD argc, LPWSTR *argv)
{
    /* argv[0] est le nom du service, en UTF-16. */
    FILE *log = _wfopen(L"service-main.txt", L"w");
    if (log) {
        fprintf(log, "argc=%lu nom=%s\n", argc,
                argc >= 1 && wcscmp(argv[0], SVC_NAME) == 0 ? "ok" : "faux");
        fclose(log);
    }
    stop_event = CreateEventW(NULL, TRUE, FALSE, NULL);
    status_handle = RegisterServiceCtrlHandlerExW(SVC_NAME, handler, (LPVOID)0x5eed);
    if (!status_handle)
        return;
    status.dwServiceType = SERVICE_WIN32_OWN_PROCESS;
    status.dwControlsAccepted = SERVICE_ACCEPT_STOP;
    status.dwCurrentState = SERVICE_RUNNING;
    SetServiceStatus(status_handle, &status);
    WaitForSingleObject(stop_event, INFINITE);
    status.dwCurrentState = SERVICE_STOPPED;
    SetServiceStatus(status_handle, &status);
}

static DWORD state_of(SC_HANDLE svc)
{
    SERVICE_STATUS_PROCESS p;
    DWORD needed = 0;
    if (!QueryServiceStatusEx(svc, SC_STATUS_PROCESS_INFO, (LPBYTE)&p, sizeof p, &needed))
        return 0;
    return p.dwCurrentState;
}

static int wait_state(SC_HANDLE svc, DWORD wanted)
{
    for (int i = 0; i < 200; i++) {
        if (state_of(svc) == wanted)
            return 1;
        Sleep(50);
    }
    return 0;
}

int wmain(int argc, wchar_t **argv)
{
    if (argc > 1 && wcscmp(argv[1], L"run") == 0) {
        SERVICE_TABLE_ENTRYW table[] = {{SVC_NAME, service_main}, {NULL, NULL}};
        return StartServiceCtrlDispatcherW(table) ? 0 : 1;
    }

    wchar_t self[MAX_PATH], image[MAX_PATH + 16];
    GetModuleFileNameW(NULL, self, MAX_PATH);
    swprintf(image, MAX_PATH + 16, L"\"%ls\" run", self);

    SC_HANDLE scm = OpenSCManagerW(NULL, SERVICES_ACTIVE_DATABASEW, SC_MANAGER_ALL_ACCESS);
    check(scm != NULL, "OpenSCManagerW");
    if (!scm)
        return 1;
    /* Reste d'un essai interrompu. */
    SC_HANDLE old = OpenServiceW(scm, SVC_NAME, SERVICE_ALL_ACCESS);
    if (old) {
        SERVICE_STATUS s;
        ControlService(old, SERVICE_CONTROL_STOP, &s);
        DeleteService(old);
        CloseServiceHandle(old);
    }

    SC_HANDLE svc = CreateServiceW(scm, SVC_NAME, L"Service d'essai Winlux (é)", SERVICE_ALL_ACCESS,
                                   SERVICE_WIN32_OWN_PROCESS, SERVICE_DEMAND_START,
                                   SERVICE_ERROR_NORMAL, image, NULL, NULL, NULL, NULL, NULL);
    check(svc != NULL, "CreateServiceW");
    if (!svc)
        return 1;
    SC_HANDLE dup = CreateServiceW(scm, SVC_NAME, NULL, SERVICE_ALL_ACCESS, SERVICE_WIN32_OWN_PROCESS,
                                   SERVICE_DEMAND_START, SERVICE_ERROR_NORMAL, image, NULL, NULL,
                                   NULL, NULL, NULL);
    check(dup == NULL && GetLastError() == ERROR_SERVICE_EXISTS, "CreateServiceW en double refusé");

    SC_HANDLE opened = OpenServiceW(scm, SVC_NAME, SERVICE_ALL_ACCESS);
    check(opened != NULL, "OpenServiceW");
    check(StartServiceW(svc, 0, NULL), "StartServiceW");
    check(wait_state(svc, SERVICE_RUNNING), "service en cours (RUNNING)");
    SERVICE_STATUS st;
    check(QueryServiceStatus(opened ? opened : svc, &st) && st.dwCurrentState == SERVICE_RUNNING,
          "QueryServiceStatus");
    check(ControlService(svc, SERVICE_CONTROL_STOP, &st), "ControlService(STOP)");
    check(wait_state(svc, SERVICE_STOPPED), "service arrêté (STOPPED)");

    char line[64] = "";
    FILE *log = _wfopen(L"service-main.txt", L"r");
    if (log) {
        fgets(line, sizeof line, log);
        fclose(log);
    }
    check(strncmp(line, "argc=1 nom=ok", 13) == 0, "ServiceMain reçoit son nom en argv[0]");

    check(DeleteService(svc), "DeleteService");
    if (opened)
        CloseServiceHandle(opened);
    CloseServiceHandle(svc);
    SC_HANDLE gone = OpenServiceW(scm, SVC_NAME, SERVICE_ALL_ACCESS);
    check(gone == NULL && GetLastError() == ERROR_SERVICE_DOES_NOT_EXIST, "service supprimé");
    CloseServiceHandle(scm);

    if (failures)
        return 1;
    printf("Service Windows : tout est correct\n");
    return 0;
}

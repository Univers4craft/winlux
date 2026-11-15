/* tube32.exe (#2530) : un programme x86 lance son enfant avec
 * STARTF_USESTDHANDLES, sortie et erreur sur un même tube, et lit ce
 * tube comme une donnée. L'enfant écrit une ligne, puis faute dans son
 * code x86 avec WINLUX_WOW32_HISTORY=1 : rapport de plantage, historique
 * des passages et note de vidage. Aucune de ces lignes de Winlux ne doit
 * arriver dans le tube, seule la ligne de l'enfant. */
#include <windows.h>
#include <stdio.h>
#include <string.h>

static const char line[] = "ligne de l'enfant\n";

int main(int argc, char **argv)
{
    if (argc > 1 && strcmp(argv[1], "enfant") == 0) {
        DWORD n;
        WriteFile(GetStdHandle(STD_ERROR_HANDLE), line, sizeof line - 1, &n, NULL);
        *(volatile int *)0 = 1;
        return 0;
    }
    SECURITY_ATTRIBUTES sa = { sizeof sa, NULL, TRUE };
    HANDLE r, w;
    if (!CreatePipe(&r, &w, &sa, 0)) {
        printf("tube32 : CreatePipe en échec\n");
        return 1;
    }
    SetHandleInformation(r, HANDLE_FLAG_INHERIT, 0);
    SetEnvironmentVariableA("WINLUX_WOW32_HISTORY", "1");
    STARTUPINFOA si;
    memset(&si, 0, sizeof si);
    si.cb = sizeof si;
    si.dwFlags = STARTF_USESTDHANDLES;
    si.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
    si.hStdOutput = w;
    si.hStdError = w;
    PROCESS_INFORMATION pi;
    char cmd[MAX_PATH + 16];
    GetModuleFileNameA(NULL, cmd, MAX_PATH);
    strcat(cmd, " enfant");
    if (!CreateProcessA(NULL, cmd, NULL, NULL, TRUE, 0, NULL, NULL, &si, &pi)) {
        printf("tube32 : CreateProcess en échec (%lu)\n", GetLastError());
        return 2;
    }
    CloseHandle(w);
    static char buf[65536];
    DWORD n, total = 0;
    while (total < sizeof buf - 1 && ReadFile(r, buf + total, sizeof buf - 1 - total, &n, NULL) && n)
        total += n;
    buf[total] = 0;
    WaitForSingleObject(pi.hProcess, 60000);
    DWORD code = 0;
    GetExitCodeProcess(pi.hProcess, &code);
    if (code == 0) {
        printf("tube32 : l'enfant n'a pas fauté\n");
        return 3;
    }
    if (strcmp(buf, line) != 0) {
        printf("tube32 : le tube contient %lu octets au lieu de la seule ligne de l'enfant :\n%.400s\n", total, buf);
        return 4;
    }
    printf("tube32 : diagnostic hors du tube, tout est correct\n");
    return 0;
}

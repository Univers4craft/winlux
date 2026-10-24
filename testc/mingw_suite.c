/* Suite de compatibilité pour un vrai exécutable compilé par MinGW-w64.
 * Elle passe par le code de démarrage de la CRT msvcrt (__getmainargs,
 * _initterm, atexit) puis exerce stdio, chaînes, mémoire, temps,
 * environnement, E/S bas niveau et threads Win32. */
#include <errno.h>
#include <fcntl.h>
#include <io.h>
#include <locale.h>
#include <math.h>
#include <process.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <wchar.h>
#include <windows.h>

static int failures;
#define CHECK(name, cond) do { if (cond) printf("  OK     %s\n", name); \
    else { printf("  ÉCHEC  %s (ligne %d)\n", name, __LINE__); failures++; } } while (0)

static int constructed;
__attribute__((constructor)) static void init_global(void) { constructed = 42; }

static void at_exit_message(void) { printf("atexit exécuté\n"); fflush(stdout); }

static int compare_int(const void *a, const void *b) { return *(const int *)a - *(const int *)b; }

static volatile LONG counter;
static unsigned __stdcall worker(void *arg) {
    for (int i = 0; i < 1000; i++) InterlockedIncrement(&counter);
    return (unsigned)(size_t)arg;
}

static volatile sig_atomic_t got_signal;
static void on_signal(int sig) { got_signal = sig; }

int main(int argc, char **argv) {
    printf("== mingw_suite ==\n");
    CHECK("constructeur global (_initterm/.ctors)", constructed == 42);
    CHECK("argc/argv", argc == 3 && strcmp(argv[1], "un") == 0 && strcmp(argv[2], "deux mots") == 0);
    CHECK("argv[0] se termine par mingw_suite.exe", strstr(argv[0], "mingw_suite.exe") != NULL);
    CHECK("atexit", atexit(at_exit_message) == 0);

    /* Mémoire */
    int *v = calloc(16, sizeof *v);
    CHECK("calloc remet à zéro", v && v[15] == 0);
    v = realloc(v, 1024 * sizeof *v);
    v[1023] = 7;
    CHECK("realloc agrandit", v[1023] == 7 && v[0] == 0);
    free(v);
    void *al = _aligned_malloc(100, 64);
    CHECK("_aligned_malloc", al && ((size_t)al & 63) == 0);
    _aligned_free(al);

    /* Chaînes et formatage */
    char buf[128];
    snprintf(buf, sizeof buf, "%d|%5.2f|%s|%x|%c", -12, 3.14159, "abc", 255, 'Z');
    CHECK("snprintf", strcmp(buf, "-12| 3.14|abc|ff|Z") == 0);
    int a = 0; char word[16]; double d = 0;
    CHECK("sscanf", sscanf("17 mot 2.5", "%d %15s %lf", &a, word, &d) == 3 && a == 17 && strcmp(word, "mot") == 0 && d == 2.5);
    CHECK("strtol base 16", strtol("ff", NULL, 16) == 255);
    CHECK("strspn/strcspn/strpbrk", strspn("aaab", "a") == 3 && strcspn("abc,d", ",") == 3 && *strpbrk("x;y", ";") == ';');
    char tok[] = "a,b,,c"; int ntok = 0;
    for (char *t = strtok(tok, ","); t; t = strtok(NULL, ",")) ntok++;
    CHECK("strtok", ntok == 3);
    CHECK("atof", fabs(atof("1.5e2") - 150.0) < 1e-9);
    int nums[] = {5, 3, 9, 1};
    qsort(nums, 4, sizeof nums[0], compare_int);
    CHECK("qsort", nums[0] == 1 && nums[3] == 9);
    wchar_t wbuf[32];
    CHECK("mbstowcs/wcslen", mbstowcs(wbuf, "large", 32) == 5 && wcslen(wbuf) == 5);

    /* Mathématiques */
    CHECK("sqrt/pow/floor", sqrt(16.0) == 4.0 && pow(2, 10) == 1024 && floor(-1.5) == -2);
    int ex; double m = frexp(8.0, &ex);
    CHECK("frexp/ldexp", m == 0.5 && ex == 4 && ldexp(0.5, 4) == 8.0);

    /* Fichiers via stdio */
    const char *path = "winlux-mingw-test.txt";
    FILE *f = fopen(path, "w");
    CHECK("fopen écriture", f != NULL);
    fprintf(f, "ligne %d\n", 1);
    fputs("ligne 2\n", f);
    fclose(f);
    f = fopen(path, "r");
    char line[64];
    CHECK("fgets relit", f && fgets(line, sizeof line, f) && strcmp(line, "ligne 1\n") == 0);
    fseek(f, 0, SEEK_END);
    CHECK("fseek/ftell", ftell(f) == 16);
    fclose(f);
    CHECK("remove", remove(path) == 0);
    errno = 0;
    CHECK("fopen absent → NULL", fopen("n-existe-pas.txt", "r") == NULL);

    /* E/S bas niveau */
    int fd = _open(path, _O_CREAT | _O_RDWR | _O_BINARY, 0600);
    CHECK("_open", fd > 2);
    CHECK("_write", _write(fd, "abc", 3) == 3);
    CHECK("_lseek", _lseek(fd, 0, SEEK_SET) == 0);
    CHECK("_read", _read(fd, line, 3) == 3 && memcmp(line, "abc", 3) == 0);
    _close(fd);
    CHECK("_unlink", _unlink(path) == 0);
    CHECK("errno après _unlink absent", _unlink(path) == -1 && errno == ENOENT);

    /* Environnement */
    CHECK("_putenv/getenv", _putenv("WINLUX_MINGW=ok") == 0 && getenv("WINLUX_MINGW") && strcmp(getenv("WINLUX_MINGW"), "ok") == 0);
    CHECK("GetEnvironmentVariableA", GetEnvironmentVariableA("WINLUX_MINGW", buf, sizeof buf) == 2);

    /* Temps */
    time_t now = time(NULL);
    struct tm *tm = localtime(&now);
    CHECK("time/localtime", now > 1600000000 && tm && tm->tm_year >= 120);
    time_t epoch = 86400;
    CHECK("gmtime/strftime", strftime(buf, sizeof buf, "%Y-%m-%d", gmtime(&epoch)) == 10 && strcmp(buf, "1970-01-02") == 0);
    CHECK("clock", clock() >= 0);
    srand(1);
    CHECK("rand (générateur Microsoft)", rand() == 41);

    /* Locale et signaux */
    CHECK("setlocale C", setlocale(LC_ALL, "C") != NULL);
    CHECK("localeconv", strcmp(localeconv()->decimal_point, ".") == 0);
    signal(SIGINT, on_signal);
    raise(SIGINT);
    CHECK("signal/raise", got_signal == SIGINT);

    /* Threads */
    unsigned tid;
    HANDLE th[4];
    for (int i = 0; i < 4; i++) th[i] = (HANDLE)_beginthreadex(NULL, 0, worker, (void *)(size_t)(i + 10), 0, &tid);
    WaitForMultipleObjects(4, th, TRUE, INFINITE);
    DWORD code = 0;
    GetExitCodeThread(th[3], &code);
    for (int i = 0; i < 4; i++) CloseHandle(th[i]);
    CHECK("_beginthreadex + Interlocked", counter == 4000 && code == 13);

    /* Console */
    DWORD written = 0;
    CHECK("WriteConsoleA/WriteFile", WriteFile(GetStdHandle(STD_OUTPUT_HANDLE), "  OK     WriteFile\n", 19, &written, NULL) && written == 19);

    printf("%d échec(s)\n", failures);
    return failures;
}

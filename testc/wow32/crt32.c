/* CRT 32 bits (#232, étape 3) : programme MinGW i686 ordinaire, lié à
 * msvcrt.dll. Construit deux fois par build.sh : avec le printf de MinGW
 * (données _iob, __initenv, GetProcAddress…) et avec celui de msvcrt
 * (fonctions variadiques et va_list relues sur la pile x86). */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <setjmp.h>
#include <stdarg.h>
#include <signal.h>
#include <time.h>
#include <locale.h>
#include <windows.h>
#include <wininet.h>
#include <io.h>
#include <wchar.h>

static int ctor_ran;
__attribute__((constructor)) static void ctor(void) { ctor_ran = 42; }
static int fails;
#define CHECK(n, c) do { if (!(c)) { printf("echec %d : %s\n", n, #c); fails = n; } } while (0)

static int cmp(const void *a, const void *b) { return *(const int*)a - *(const int*)b; }
static jmp_buf jb;
static void jump(int v) { longjmp(jb, v); }
static volatile int got_signal;
static void on_signal(int s) { got_signal = s; }
static void bye(void) { printf("atexit appele\n"); fflush(stdout); }
static int vfmt(char *buf, size_t n, const char *f, ...) { va_list ap; va_start(ap, f); int r = vsnprintf(buf, n, f, ap); va_end(ap); return r; }

int main(int argc, char **argv) {
    char buf[256];
    printf("Bonjour depuis la CRT 32 bits (%d arguments)\n", argc);
    CHECK(1, ctor_ran == 42);
    CHECK(2, argc == 2 && strcmp(argv[1], "argument-de-test") == 0);
    sprintf(buf, "%d|%5.2f|%s|%lld|%c|%x", -7, 3.14159, "abc", 1234567890123LL, 'Z', 0xBEEF);
    CHECK(3, strcmp(buf, "-7| 3.14|abc|1234567890123|Z|beef") == 0);
    if (fails == 3) printf("obtenu : %s\n", buf);
    vfmt(buf, sizeof buf, "%s-%d-%g", "va", 12, 0.5);
    CHECK(4, strcmp(buf, "va-12-0.5") == 0);
    int a = 0; double d = 0; char w[16];
    CHECK(5, sscanf("17 2.5 mot", "%d %lf %15s", &a, &d, w) == 3 && a == 17 && d == 2.5 && strcmp(w, "mot") == 0);
    int t[5] = {5, 3, 9, 1, 4};
    qsort(t, 5, sizeof(int), cmp);
    CHECK(6, t[0] == 1 && t[4] == 9);
    errno = 0;
    strtol("999999999999999999999", NULL, 10);
    CHECK(7, errno == ERANGE);
    char *p = malloc(100000);
    CHECK(8, p != NULL && (unsigned long)p < 0x80000000UL);
    free(p);
    CHECK(9, getenv("WINLUX_WOW32_TEST") != NULL);
    volatile int n = 0;
    int r = setjmp(jb);
    if (r == 0) { n = 1; jump(5); }
    CHECK(10, r == 5 && n == 1);
    fprintf(stderr, "sur stderr %d\n", 3);
    FILE *f = fopen("crt32.tmp", "wb");
    CHECK(11, f != NULL);
    if (f) { fwrite("xyz", 1, 3, f); fclose(f); }
    f = fopen("crt32.tmp", "rb");
    memset(buf, 0, 8);
    CHECK(12, f && fread(buf, 1, 8, f) == 3 && strcmp(buf, "xyz") == 0);
    if (f) fclose(f);
    remove("crt32.tmp");
    CHECK(13, strtod("1e3", NULL) == 1000.0);
    double x = 7.9; CHECK(14, (int)x == 7 && (long long)(x * 1e10) == 79000000000LL);
    int *hit = bsearch(&t[3], t, 5, sizeof(int), cmp);
    CHECK(15, hit == &t[3]);
    signal(SIGINT, on_signal);
    raise(SIGINT);
    CHECK(16, got_signal == SIGINT);
    time_t now = time(NULL);
    struct tm *lt = localtime(&now);
    CHECK(17, now > 1600000000 && lt != NULL && lt->tm_year >= 120 && (unsigned long)lt < 0x80000000UL);
    CHECK(18, setlocale(LC_ALL, NULL) != NULL && localeconv()->decimal_point[0] == '.');
    CHECK(19, strlen(strerror(ENOENT)) > 3);
    sprintf(buf, "%5s|%-4d|%04x|%.3s|%e", "ab", 12, 0x2a, "abcdef", 1500.0);
    CHECK(20, strcmp(buf, "   ab|12  |002a|abc|1.500000e+003") == 0 || strcmp(buf, "   ab|12  |002a|abc|1.500000e+03") == 0);
    if (fails == 20) printf("obtenu : %s\n", buf);
    atexit(bye);
    if (!fails) printf("CRT 32 bits : tout est correct\n");
    {
        /* Aides mathématiques du CRT x86 (#1333) : convention x87 (_CI*) et
         * SSE2 (_libm_sse2_*), appelées comme le code de MSVC. */
        HMODULE u = LoadLibraryA("ucrtbase.dll");
        void *ci_fmod = (void *)GetProcAddress(u, "_CIfmod");
        void *ci_atan2 = (void *)GetProcAddress(u, "_CIatan2");
        void *pow2 = (void *)GetProcAddress(u, "_libm_sse2_pow_precise");
        void *sin2 = (void *)GetProcAddress(u, "_libm_sse2_sin_precise");
        CHECK(21, ci_fmod && ci_atan2 && pow2 && sin2);
        if (ci_fmod && ci_atan2 && pow2 && sin2) {
            double a = 7.5, b = 2.0, r;
            __asm__ volatile("fldl %1\n\tfldl %2\n\tcall *%3\n\tfstpl %0"
                             : "=m"(r) : "m"(a), "m"(b), "r"(ci_fmod) : "eax", "ecx", "edx", "memory");
            CHECK(22, r == 1.5);
            double y = 1.0, x = -1.0;
            __asm__ volatile("fldl %1\n\tfldl %2\n\tcall *%3\n\tfstpl %0"
                             : "=m"(r) : "m"(y), "m"(x), "r"(ci_atan2) : "eax", "ecx", "edx", "memory");
            CHECK(23, r > 2.356 && r < 2.357);
            double p = 2.0, e = 10.0;
            __asm__ volatile("movsd %1, %%xmm0\n\tmovsd %2, %%xmm1\n\tcall *%3\n\tmovsd %%xmm0, %0"
                             : "=m"(r) : "m"(p), "m"(e), "r"(pow2)
                             : "eax", "ecx", "edx", "memory");
            CHECK(24, r == 1024.0);
            double z = 0.0;
            __asm__ volatile("movsd %1, %%xmm0\n\tcall *%2\n\tmovsd %%xmm0, %0"
                             : "=m"(r) : "m"(z), "r"(sin2)
                             : "eax", "ecx", "edx", "memory");
            CHECK(25, r == 0.0);
        }
        /* Python 3.12 x86 (#1383) : _CI* à un opérande, log en SSE2 et
         * __fpe_flt_rounds (arrondi au plus proche : 1). */
        void *ci_sinh = (void *)GetProcAddress(u, "_CIsinh");
        void *log2 = (void *)GetProcAddress(u, "_libm_sse2_log_precise");
        int (*flt_rounds)(void) = (int (*)(void))GetProcAddress(u, "__fpe_flt_rounds");
        CHECK(26, ci_sinh && log2 && flt_rounds);
        if (ci_sinh && log2 && flt_rounds) {
            double z = 0.0, one = 1.0, r;
            __asm__ volatile("fldl %1\n\tcall *%2\n\tfstpl %0"
                             : "=m"(r) : "m"(z), "r"(ci_sinh) : "eax", "ecx", "edx", "memory");
            CHECK(27, r == 0.0);
            __asm__ volatile("movsd %1, %%xmm0\n\tcall *%2\n\tmovsd %%xmm0, %0"
                             : "=m"(r) : "m"(one), "r"(log2)
                             : "eax", "ecx", "edx", "memory");
            CHECK(28, r == 0.0);
            CHECK(29, flt_rounds() == 1);
        }
    }
    {
        /* Steam.exe (#1380) : masques d'affinité sur 4 octets, variable
         * cachée « =C: » écrite par _wchdir, imports par ordinal. */
        struct { DWORD_PTR p, s; DWORD garde; } m = { 0, 0, 0xA5A5A5A5u };
        CHECK(30, GetProcessAffinityMask(GetCurrentProcess(), &m.p, &m.s) && m.p && m.garde == 0xA5A5A5A5u);
        CHECK(31, SetProcessAffinityMask(GetCurrentProcess(), m.p));
        wchar_t v[64] = L"";
        CHECK(32, SetEnvironmentVariableW(L"=C:", L"C:\\Steam")
                  && GetEnvironmentVariableW(L"=C:", v, 64) == 8 && !wcscmp(v, L"C:\\Steam"));
        HMODULE sh = LoadLibraryA("shell32.dll"), ws = LoadLibraryA("wsock32.dll");
        CHECK(33, sh && ws && GetProcAddress(sh, MAKEINTRESOURCEA(680))
                  && GetProcAddress(ws, MAKEINTRESOURCEA(1142)));
    }
    {
        /* python312.dll x86 (#1407) : __timezone et __tzname rendent des
         * pointeurs lisibles, fonctions mathématiques C99 et _wstat64i32. */
        HMODULE u = LoadLibraryA("ucrtbase.dll");
        long *(*tz)(void) = (long *(*)(void))GetProcAddress(u, "__timezone");
        char **(*tzn)(void) = (char **(*)(void))GetProcAddress(u, "__tzname");
        CHECK(34, tz && tzn && tz() && *tz() >= -50400 && *tz() <= 50400
                  && tzn() && tzn()[0] && strlen(tzn()[0]) < 64);
        double (*acosh_)(double) = (double (*)(double))GetProcAddress(u, "acosh");
        double (*cbrt_)(double) = (double (*)(double))GetProcAddress(u, "cbrt");
        double (*round_)(double) = (double (*)(double))GetProcAddress(u, "round");
        double (*log2_)(double) = (double (*)(double))GetProcAddress(u, "log2");
        double (*copysign_)(double, double) = (double (*)(double, double))GetProcAddress(u, "copysign");
        CHECK(35, acosh_ && cbrt_ && round_ && log2_ && copysign_
                  && acosh_(1.0) == 0.0 && cbrt_(27.0) == 3.0 && round_(2.5) == 3.0
                  && log2_(8.0) == 3.0 && copysign_(2.0, -1.0) == -2.0);
        int (*wstat_)(const wchar_t *, void *) = (int (*)(const wchar_t *, void *))GetProcAddress(u, "_wstat64i32");
        unsigned char st[48];
        memset(st, 0, sizeof st);
        CHECK(36, wstat_ && wstat_(L"C:\\", st) == 0 && (*(unsigned short *)(st + 6) & 0x4000));
    }
    {
        /* 7-Zip (#1413) : FORMAT_MESSAGE_ALLOCATE_BUFFER écrit un pointeur
         * de 4 octets, sans toucher au mot suivant. */
        struct { wchar_t *p; DWORD garde; } b = { NULL, 0xA5A5A5A5u };
        DWORD n = FormatMessageW(FORMAT_MESSAGE_ALLOCATE_BUFFER | FORMAT_MESSAGE_FROM_SYSTEM
                                 | FORMAT_MESSAGE_IGNORE_INSERTS, NULL, 5, 0, (LPWSTR)&b.p, 0, NULL);
        CHECK(37, n > 0 && b.p && b.garde == 0xA5A5A5A5u && wcslen(b.p) == n);
        LocalFree(b.p);
    }
    {
        /* Python 3.12 x86 (#1436) : GetFileInformationByHandle sur la sortie
         * standard, redirigée vers un fichier par test-wow32.sh. */
        BY_HANDLE_FILE_INFORMATION fi;
        memset(&fi, 0xEE, sizeof fi);
        HANDLE out = (HANDLE)_get_osfhandle(1);
        CHECK(38, GetFileType(out) != FILE_TYPE_DISK
                  || (GetFileInformationByHandle(out, &fi) && fi.nNumberOfLinks >= 1
                      && !(fi.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY)));
    }
    {
        /* Steam x86 (#1439) : manifeste écrit par CreateFileW (CREATE_ALWAYS),
         * GetFileType, sonde à -1 depuis la fin (ERROR_NEGATIVE_SEEK
         * attendu), SetFilePointerEx puis WriteFile. Le LARGE_INTEGER
         * passe par valeur sur 8 octets de pile. */
        wchar_t dir[MAX_PATH], path[MAX_PATH];
        GetTempPathW(MAX_PATH, dir);
        GetTempFileNameW(dir, L"spx", 0, path);
        HANDLE h = CreateFileW(path, GENERIC_READ | GENERIC_WRITE, 0, NULL, CREATE_ALWAYS,
                               FILE_ATTRIBUTE_NORMAL, NULL);
        LARGE_INTEGER z, pos, sz;
        z.QuadPart = 0;
        pos.QuadPart = -1;
        DWORD w = 0;
        z.QuadPart = -1;
        int neg = h != INVALID_HANDLE_VALUE && !SetFilePointerEx(h, z, &pos, FILE_END)
                  && GetLastError() == ERROR_NEGATIVE_SEEK;
        z.QuadPart = 0;
        int ok1 = neg && GetFileType(h) == FILE_TYPE_DISK
                  && SetFilePointerEx(h, z, &pos, FILE_END) && pos.QuadPart == 0
                  && WriteFile(h, "abcde", 5, &w, NULL) && w == 5;
        z.QuadPart = -2;
        int ok2 = ok1 && SetFilePointerEx(h, z, &pos, FILE_CURRENT) && pos.QuadPart == 3;
        z.QuadPart = 1;
        int ok3 = ok2 && SetFilePointerEx(h, z, NULL, FILE_BEGIN)
                  && GetFileSizeEx(h, &sz) && sz.QuadPart == 5;
        if (h != INVALID_HANDLE_VALUE) CloseHandle(h);
        DeleteFileW(path);
        CHECK(39, ok3);
    }
    {
        /* tkinter x86 (#1474) : TCL_LIBRARY posée par _wputenv après le
         * démarrage doit apparaître dans _wenviron et _environ, que Tcl
         * parcourt. */
        int seen_w = 0, seen_a = 0;
        _wputenv(L"WINLUX_T1474=ok");
        for (wchar_t **e = _wenviron; e && *e; e++)
            if (!wcscmp(*e, L"WINLUX_T1474=ok")) seen_w = 1;
        for (char **e = _environ; e && *e; e++)
            if (!strcmp(*e, "WINLUX_T1474=ok")) seen_a = 1;
        _putenv("WINLUX_T1474=");
        int gone = 1;
        for (char **e = _environ; e && *e; e++)
            if (!strncmp(*e, "WINLUX_T1474=", 13)) gone = 0;
        CHECK(40, seen_w && seen_a && gone && !getenv("WINLUX_T1474"));
    }
    {
        /* Steam x86 (#1475) : MoveFileTransactedW et DeleteFileTransactedW
         * trouvées par GetProcAddress, déplacement puis suppression faits. */
        typedef BOOL (WINAPI *mv_t)(LPCWSTR, LPCWSTR, void *, void *, DWORD, HANDLE);
        typedef BOOL (WINAPI *del_t)(LPCWSTR, HANDLE);
        HMODULE k = GetModuleHandleA("kernel32.dll");
        mv_t mv = (mv_t)GetProcAddress(k, "MoveFileTransactedW");
        del_t del = (del_t)GetProcAddress(k, "DeleteFileTransactedW");
        wchar_t dir[MAX_PATH], a[MAX_PATH], b[MAX_PATH];
        GetTempPathW(MAX_PATH, dir);
        GetTempFileNameW(dir, L"tx", 0, a);
        wcscpy(b, a);
        wcscat(b, L"_dest");
        int ok = mv && del && mv(a, b, NULL, NULL, MOVEFILE_REPLACE_EXISTING, INVALID_HANDLE_VALUE)
                 && GetFileAttributesW(a) == INVALID_FILE_ATTRIBUTES
                 && GetFileAttributesW(b) != INVALID_FILE_ATTRIBUTES
                 && del(b, INVALID_HANDLE_VALUE) && GetFileAttributesW(b) == INVALID_FILE_ATTRIBUTES;
        DeleteFileW(a);
        DeleteFileW(b);
        CHECK(41, ok);
    }
    {
        /* #1508 : complexes x86 (pointeur caché pour _Dcomplex, edx:eax
         * pour _Fcomplex, résultat réel dans st0), __pioinfo sous 4 Go et
         * taille de tampon de __stdio_common_vsscanf « _s ». */
        typedef struct { double re, im; } dc;
        typedef struct { float re, im; } fc;
        HMODULE u = LoadLibraryA("ucrtbase.dll");
        HMODULE m = LoadLibraryA("msvcrt.dll");
        dc (*cexp_)(dc) = (dc (*)(dc))GetProcAddress(u, "cexp");
        fc (*cpowf_)(fc, fc) = (fc (*)(fc, fc))GetProcAddress(u, "cpowf");
        double (*cimag_)(dc) = (double (*)(dc))GetProcAddress(u, "cimag");
        float (*cargf_)(fc) = (float (*)(fc))GetProcAddress(u, "cargf");
        void **pio = (void **)GetProcAddress(m, "__pioinfo");
        int (*vss)(unsigned long long, const char *, size_t, const char *, void *, va_list) =
            (int (*)(unsigned long long, const char *, size_t, const char *, void *, va_list))
                GetProcAddress(u, "__stdio_common_vsscanf");
        int ok = cexp_ && cpowf_ && cimag_ && cargf_ && pio && vss;
        if (ok) {
            dc z = {0.0, 3.141592653589793};
            dc e = cexp_(z);
            fc i = {0.0f, 1.0f}, two = {2.0f, 0.0f};
            fc p = cpowf_(i, two);
            dc w = {1.0, 2.5};
            fc q = {0.0f, 1.0f};
            ok = e.re < -0.999999 && e.re > -1.000001 && e.im < 1e-9 && e.im > -1e-9
                 && p.re < -0.99999f && p.re > -1.00001f
                 && cimag_(w) == 2.5 && cargf_(q) > 1.5707f && cargf_(q) < 1.5709f
                 && pio[0] != NULL && (unsigned long)pio[0] < 0x80000000UL;
        }
        if (ok) {
            char word[8], small[3];
            int n = 0;
            struct { char *w; unsigned ws; int *n; } a1 = {word, sizeof word, &n};
            struct { char *s; unsigned ss; } a2 = {small, sizeof small};
            ok = vss(1, "abc 5", (size_t)-1, "%s %d", NULL, (va_list)&a1) == 2
                 && strcmp(word, "abc") == 0 && n == 5
                 && vss(1, "abcdef", (size_t)-1, "%s", NULL, (va_list)&a2) == 0 && small[0] == 0;
        }
        CHECK(42, ok);
    }
    {
        /* #1508 : std::exception de msvcrt x86 (thiscall), table virtuelle
         * { destructeur de suppression, what }. */
        typedef void *(__thiscall *ctor_s)(void *, const char *const *);
        typedef void *(__thiscall *copy_t)(void *, const void *);
        typedef void (__thiscall *dtor_t)(void *);
        typedef const char *(__thiscall *what_t)(const void *);
        HMODULE m = LoadLibraryA("msvcrt.dll");
        ctor_s c = (ctor_s)GetProcAddress(m, "??0exception@@QAE@ABQBD@Z");
        copy_t cp = (copy_t)GetProcAddress(m, "??0bad_cast@@QAE@ABV0@@Z");
        dtor_t d = (dtor_t)GetProcAddress(m, "??1exception@@UAE@XZ");
        what_t w = (what_t)GetProcAddress(m, "?what@exception@@UBEPBDXZ");
        int ok = c && cp && d && w;
        if (ok) {
            void *e[3], *b[3];
            char text[8] = "boum";
            const char *pt = text;
            ok = c(e, &pt) == e;
            text[0] = 'X'; /* la chaîne a été recopiée */
            what_t vw = ok ? ((what_t *)e[0])[1] : 0;
            ok = ok && strcmp(w(e), "boum") == 0 && vw && strcmp(vw(e), "boum") == 0
                 && cp(b, e) == b && b[0] != e[0] && strcmp(w(b), "boum") == 0;
            if (ok) {
                d(b);
                d(e);
            }
        }
        CHECK(43, ok);
    }
    /* #2168 : données du runtime rendues par pointeur, lisibles en x86
       (l'outil de déploiement d'Office lisait ___lc_locale_name_func). */
    {
        typedef wchar_t **(__cdecl *names_t)(void);
        typedef wchar_t *(__cdecl *wcmd_t)(void);
        typedef char *(__cdecl *tmp_t)(char *);
        HMODULE u = LoadLibraryA("ucrtbase.dll");
        HMODULE m = GetModuleHandleA("msvcrt.dll");
        names_t names = u ? (names_t)GetProcAddress(u, "___lc_locale_name_func") : 0;
        wcmd_t wcmd = u ? (wcmd_t)GetProcAddress(u, "_get_wide_winmain_command_line") : 0;
        tmp_t tmp = m ? (tmp_t)GetProcAddress(m, "tmpnam") : 0;
        int ok = names && wcmd && tmp;
        if (ok) {
            wchar_t **n = names();
            for (int i = 0; i < 6; i++)
                ok = ok && n[i] == 0;
            const wchar_t *c = wcmd();
            ok = ok && c && wcsstr(c, L"argument-de-test") != 0;
            char *t = tmp(0);
            ok = ok && t && t[0] != 0 && strlen(t) < 260;
        }
        CHECK(44, ok);
    }
    /* #2512 : enveloppes de shlwapi servies en x86 (l'outil de déploiement
       d'Office mourait en 0xC0000139 sur PathIsUNCW). PathIsSystemFolderW
       dépile deux arguments, wnsprintfW est à arguments variables. */
    {
        typedef int (__stdcall *q_t)(const wchar_t *);
        typedef int (__stdcall *q2_t)(const wchar_t *, unsigned long);
        typedef const wchar_t *(__stdcall *p_t)(const wchar_t *);
        typedef int (__stdcall *ip_t)(wchar_t *);
        typedef int (__cdecl *f_t)(wchar_t *, int, const wchar_t *, ...);
        HMODULE s = LoadLibraryA("shlwapi.dll");
        q_t unc = s ? (q_t)GetProcAddress(s, "PathIsUNCW") : 0;
        q2_t sys = s ? (q2_t)GetProcAddress(s, "PathIsSystemFolderW") : 0;
        p_t args = s ? (p_t)GetProcAddress(s, "PathGetArgsW") : 0;
        ip_t root = s ? (ip_t)GetProcAddress(s, "PathStripToRootW") : 0;
        f_t fmt = s ? (f_t)GetProcAddress(s, "wnsprintfW") : 0;
        int ok = unc && sys && args && root && fmt;
        if (ok) {
            static const wchar_t cmd[] = L"c:\\a.exe -x";
            wchar_t r[260] = L"c:\\dossier\\f.txt";
            wchar_t out[16];
            ok = unc(L"\\\\serveur\\partage") == 1 && unc(L"c:\\x") == 0
                 && sys(L"c:\\x", 0) == 0
                 && args(cmd) == cmd + 9
                 && root(r) == 1 && wcscmp(r, L"c:\\") == 0
                 && fmt(out, 16, L"%d-%s", 42, L"ab") == 5
                 && wcscmp(out, L"42-ab") == 0;
        }
        CHECK(45, ok);
    }
    /* #2554 : InternetCrackUrlA/W sur un URL_COMPONENTS x86 de 60 octets
       (l'outil de déploiement d'Office tombait en faute) : mode pointeurs
       pour l'hôte en A, copie du chemin en W, rien écrit après la
       structure. */
    {
        typedef BOOL (__stdcall *ca_t)(const char *, DWORD, DWORD, URL_COMPONENTSA *);
        typedef BOOL (__stdcall *cw_t)(const wchar_t *, DWORD, DWORD, URL_COMPONENTSW *);
        HMODULE n = LoadLibraryA("wininet.dll");
        ca_t ca = n ? (ca_t)GetProcAddress(n, "InternetCrackUrlA") : 0;
        cw_t cw = n ? (cw_t)GetProcAddress(n, "InternetCrackUrlW") : 0;
        int ok = ca && cw && sizeof(URL_COMPONENTSA) == 60;
        if (ok) {
            static const char url[] = "https://exemple.fr:8443/a/b?c=d";
            struct { URL_COMPONENTSA c; unsigned char garde[16]; } a;
            memset(&a, 0, sizeof a);
            memset(a.garde, 0x5A, sizeof a.garde);
            a.c.dwStructSize = sizeof a.c;
            a.c.dwHostNameLength = 1;
            ok = ca(url, 0, 0, &a.c) && a.c.lpszHostName == url + 8
                 && a.c.dwHostNameLength == 10 && a.c.nPort == 8443
                 && a.c.nScheme == INTERNET_SCHEME_HTTPS
                 && a.garde[0] == 0x5A && a.garde[15] == 0x5A;
            wchar_t chemin[32];
            URL_COMPONENTSW w;
            memset(&w, 0, sizeof w);
            w.dwStructSize = sizeof w;
            w.lpszUrlPath = chemin;
            w.dwUrlPathLength = 32;
            ok = ok && cw(L"http://h.fr/x/y?z", 0, 0, &w) && w.dwUrlPathLength == 4
                 && wcscmp(chemin, L"/x/y") == 0 && w.nPort == 80;
        }
        CHECK(46, ok);
    }
    /* #2574 : WinHttpCrackUrl sur un URL_COMPONENTS x86 de 60 octets (l'outil
       de déploiement d'Office lisait ensuite un pointeur nul). winhttp.h ne
       s'inclut pas avec wininet.h ; son URL_COMPONENTS a la même suite de
       champs que URL_COMPONENTSW (pages URL_COMPONENTS de WinINet et WinHTTP
       sur Microsoft Learn), et nScheme vaut 2 pour https en WinHTTP. */
    {
        typedef BOOL (__stdcall *wc_t)(const wchar_t *, DWORD, DWORD, URL_COMPONENTSW *);
        HMODULE h = LoadLibraryA("winhttp.dll");
        wc_t wc = h ? (wc_t)GetProcAddress(h, "WinHttpCrackUrl") : 0;
        int ok = wc && sizeof(URL_COMPONENTSW) == 60;
        if (ok) {
            static const wchar_t url[] = L"https://exemple.fr:8443/a/b?c=d";
            struct { URL_COMPONENTSW c; unsigned char garde[16]; } a;
            wchar_t chemin[32];
            memset(&a, 0, sizeof a);
            memset(a.garde, 0x5A, sizeof a.garde);
            a.c.dwStructSize = sizeof a.c;
            a.c.dwHostNameLength = 1;
            a.c.lpszUrlPath = chemin;
            a.c.dwUrlPathLength = 32;
            ok = wc(url, 0, 0, &a.c) && a.c.lpszHostName == url + 8
                 && a.c.dwHostNameLength == 10 && a.c.nPort == 8443 && a.c.nScheme == 2
                 && a.c.dwUrlPathLength == 4 && wcscmp(chemin, L"/a/b") == 0
                 && a.garde[0] == 0x5A && a.garde[15] == 0x5A;
        }
        CHECK(47, ok);
    }
    /* #2597 : HttpOpenRequestA/W et WinHttpOpenRequest lisent le tableau des
       types acceptés (lplpszAcceptTypes, terminé par NULL) avec des pointeurs
       de 4 octets (l'outil de déploiement d'Office mourait dans
       HttpOpenRequestA). Ouvrir une requête n'envoie rien. */
    {
        typedef void *(__stdcall *wo_t)(const wchar_t *, DWORD, const wchar_t *, const wchar_t *, DWORD);
        typedef void *(__stdcall *wcn_t)(void *, const wchar_t *, WORD, DWORD);
        typedef void *(__stdcall *wor_t)(void *, const wchar_t *, const wchar_t *, const wchar_t *,
                                         const wchar_t *, const wchar_t **, DWORD);
        typedef BOOL (__stdcall *wcl_t)(void *);
        HMODULE n = LoadLibraryA("wininet.dll");
        HMODULE h = LoadLibraryA("winhttp.dll");
        wo_t wopen = h ? (wo_t)GetProcAddress(h, "WinHttpOpen") : 0;
        wcn_t wconn = h ? (wcn_t)GetProcAddress(h, "WinHttpConnect") : 0;
        wor_t wreq = h ? (wor_t)GetProcAddress(h, "WinHttpOpenRequest") : 0;
        wcl_t wclose = h ? (wcl_t)GetProcAddress(h, "WinHttpCloseHandle") : 0;
        typedef HINTERNET (__stdcall *io_t)(const char *, DWORD, const char *, const char *, DWORD);
        typedef HINTERNET (__stdcall *ic_t)(HINTERNET, const char *, WORD, const char *, const char *,
                                            DWORD, DWORD, DWORD_PTR);
        typedef HINTERNET (__stdcall *ra_t)(HINTERNET, const char *, const char *, const char *,
                                            const char *, const char **, DWORD, DWORD_PTR);
        typedef HINTERNET (__stdcall *rw_t)(HINTERNET, const wchar_t *, const wchar_t *, const wchar_t *,
                                            const wchar_t *, const wchar_t **, DWORD, DWORD_PTR);
        typedef BOOL (__stdcall *icl_t)(HINTERNET);
        io_t iopen = n ? (io_t)GetProcAddress(n, "InternetOpenA") : 0;
        ic_t iconn = n ? (ic_t)GetProcAddress(n, "InternetConnectA") : 0;
        ra_t hra = n ? (ra_t)GetProcAddress(n, "HttpOpenRequestA") : 0;
        rw_t hrw = n ? (rw_t)GetProcAddress(n, "HttpOpenRequestW") : 0;
        icl_t iclose = n ? (icl_t)GetProcAddress(n, "InternetCloseHandle") : 0;
        int ok = iopen && iconn && hra && hrw && iclose && wopen && wconn && wreq && wclose;
        if (ok) {
            static const char *types_a[] = {"text/*", "application/octet-stream", 0};
            static const wchar_t *types_w[] = {L"text/*", L"*/*", 0};
            HINTERNET s = iopen("crt32", INTERNET_OPEN_TYPE_DIRECT, 0, 0, 0);
            HINTERNET c = s ? iconn(s, "127.0.0.1", 80, 0, 0, INTERNET_SERVICE_HTTP, 0, 0) : 0;
            HINTERNET ra = c ? hra(c, "GET", "/v64.cab", 0, 0, types_a, 0, 0) : 0;
            HINTERNET rw = c ? hrw(c, L"GET", L"/v64.cab", 0, 0, types_w, 0, 0) : 0;
            ok = ra && rw;
            if (ra) iclose(ra);
            if (rw) iclose(rw);
            if (c) iclose(c);
            if (s) iclose(s);
            void *ws = wopen(L"crt32", 1, 0, 0, 0);
            void *wc = ws ? wconn(ws, L"127.0.0.1", 80, 0) : 0;
            void *wr = wc ? wreq(wc, L"GET", L"/v64.cab", 0, 0, types_w, 0) : 0;
            ok = ok && wr;
            if (wr) wclose(wr);
            if (wc) wclose(wc);
            if (ws) wclose(ws);
        }
        CHECK(48, ok);
    }
    /* #2619 : WinHttpSetStatusCallback rend le rappel précédent (NULL au
       départ), pas WINHTTP_INVALID_STATUS_CALLBACK (l'outil de déploiement
       d'Office fermait chaque requête avant l'envoi). */
    {
        typedef void *(__stdcall *wo_t)(const wchar_t *, DWORD, const wchar_t *, const wchar_t *, DWORD);
        typedef void *(__stdcall *wcn_t)(void *, const wchar_t *, WORD, DWORD);
        typedef void *(__stdcall *wor_t)(void *, const wchar_t *, const wchar_t *, const wchar_t *,
                                         const wchar_t *, const wchar_t **, DWORD);
        typedef void *(__stdcall *wsc_t)(void *, void *, DWORD, DWORD_PTR);
        typedef BOOL (__stdcall *wcl_t)(void *);
        HMODULE h = LoadLibraryA("winhttp.dll");
        wo_t wopen = h ? (wo_t)GetProcAddress(h, "WinHttpOpen") : 0;
        wcn_t wconn = h ? (wcn_t)GetProcAddress(h, "WinHttpConnect") : 0;
        wor_t wreq = h ? (wor_t)GetProcAddress(h, "WinHttpOpenRequest") : 0;
        wsc_t wcb = h ? (wsc_t)GetProcAddress(h, "WinHttpSetStatusCallback") : 0;
        wcl_t wclose = h ? (wcl_t)GetProcAddress(h, "WinHttpCloseHandle") : 0;
        int ok = wopen && wconn && wreq && wcb && wclose;
        if (ok) {
            void *ws = wopen(L"crt32", 1, 0, 0, 0);
            void *wc = ws ? wconn(ws, L"127.0.0.1", 80, 0) : 0;
            void *wr = wc ? wreq(wc, L"GET", L"/v64.cab", 0, 0, 0, 0) : 0;
            void *rappel = (void *)(DWORD_PTR)0x00401000;
            ok = wr && wcb(wr, rappel, 0x10000, 0) == 0 && wcb(wr, 0, 0, 0) == rappel;
            if (wr) wclose(wr);
            if (wc) wclose(wc);
            if (ws) wclose(ws);
        }
        CHECK(49, ok);
    }
    /* #2623 : webservices.dll (WWSAPI) en x86. L'outil de déploiement
       d'Office la charge en différé pour lire une réponse XML. Structures
       déclarées ici d'après l'ordre des champs de Microsoft Learn et
       windows-rs (aucun en-tête webservices.h). */
    {
        typedef struct { ULONG length; BYTE *bytes; void *dictionary; ULONG id; } wxs;
        typedef struct { ULONG mapping; wxs *localName; wxs *ns; ULONG type; void *typeDescription;
                         ULONG offset; ULONG options; void *defaultValue; ULONG countOffset;
                         wxs *itemLocalName; wxs *itemNs; void *itemRange; } wfd;
        typedef struct { ULONG size; ULONG alignment; wfd **fields; ULONG fieldCount; wxs *typeLocalName;
                         wxs *typeNs; void *parentType; void **subTypes; ULONG subTypeCount;
                         ULONG structOptions; } wsd;
        typedef struct { ULONG version; wchar_t *nom; wchar_t **urls; ULONG n; } cfg_t;
        typedef HRESULT (__stdcall *creader_t)(void *, ULONG, void **, void *);
        typedef HRESULT (__stdcall *cheap_t)(SIZE_T, SIZE_T, void *, ULONG, void **, void *);
        typedef HRESULT (__stdcall *input_t)(void *, void *, void *, void *, ULONG, void *);
        typedef HRESULT (__stdcall *tostart_t)(void *, wxs *, wxs *, BOOL *, void *);
        typedef HRESULT (__stdcall *start_t)(void *, void *);
        typedef HRESULT (__stdcall *rtype_t)(void *, ULONG, ULONG, void *, ULONG, void *, void *, ULONG, void *);
        typedef void (__stdcall *free_t)(void *);
        HMODULE h = LoadLibraryA("webservices.dll");
        creader_t creader = h ? (creader_t)GetProcAddress(h, "WsCreateReader") : 0;
        cheap_t cheap = h ? (cheap_t)GetProcAddress(h, "WsCreateHeap") : 0;
        input_t input = h ? (input_t)GetProcAddress(h, "WsSetInput") : 0;
        tostart_t tostart = h ? (tostart_t)GetProcAddress(h, "WsReadToStartElement") : 0;
        start_t start = h ? (start_t)GetProcAddress(h, "WsReadStartElement") : 0;
        rtype_t rtype = h ? (rtype_t)GetProcAddress(h, "WsReadType") : 0;
        free_t freader = h ? (free_t)GetProcAddress(h, "WsFreeReader") : 0;
        free_t fheap = h ? (free_t)GetProcAddress(h, "WsFreeHeap") : 0;
        int ok = creader && cheap && input && tostart && start && rtype && freader && fheap;
        if (ok) {
            static const char xml[] =
                "<Liste><Cfg Version=\"16\"><Nom>Office</Nom><Url>a</Url><Url>b</Url></Cfg></Liste>";
            wxs liste = { 5, (BYTE *)"Liste", 0, 0 }, version = { 7, (BYTE *)"Version", 0, 0 };
            wxs nom = { 3, (BYTE *)"Nom", 0, 0 }, url = { 3, (BYTE *)"Url", 0, 0 };
            wfd fv = { 1, &version, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0 };
            wfd fn_ = { 2, &nom, 0, 17, 0, 4, 0, 0, 0, 0, 0, 0 };
            wfd fu = { 3, 0, 0, 17, 0, 8, 0, 0, 12, &url, 0, 0 };
            wfd *fields[3] = { &fv, &fn_, &fu };
            wsd sd = { sizeof(cfg_t), 4, fields, 3, 0, 0, 0, 0, 0, 0 };
            ULONG enc[2] = { 1, 1 };
            struct { ULONG type; const void *data; ULONG size; } in = { 1, xml, sizeof xml - 1 };
            void *r = 0, *heap = 0;
            BOOL found = FALSE;
            cfg_t c;
            memset(&c, 0, sizeof c);
            ok = creader(0, 0, &r, 0) == 0 && cheap(1 << 20, 0, 0, 0, &heap, 0) == 0
                 && input(r, enc, &in, 0, 0, 0) == 0
                 && tostart(r, &liste, 0, &found, 0) == 0 && found
                 && start(r, 0) == 0
                 && rtype(r, 1, 26, &sd, 1, heap, &c, sizeof c, 0) == 0
                 && c.version == 16 && c.nom && wcscmp(c.nom, L"Office") == 0
                 && c.n == 2 && c.urls && wcscmp(c.urls[0], L"a") == 0 && wcscmp(c.urls[1], L"b") == 0;
            if (r) freader(r);
            if (heap) fheap(heap);
        }
        CHECK(50, ok);
    }
    return fails;
}

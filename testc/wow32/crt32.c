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
    return fails;
}

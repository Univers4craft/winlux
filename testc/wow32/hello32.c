/* Premier programme Windows 32 bits (x86) de Winlux (#232).
 * Construit par clang --target=i686-pc-windows-msvc et lld-link, sans CRT :
 * seulement kernel32, en stdcall. Rend 0 si tout va bien, sinon le numéro
 * de la vérification qui a échoué. */
typedef unsigned int DWORD;
typedef int BOOL;
typedef void *HANDLE;
typedef unsigned long long ULONGLONG;
typedef union { struct { DWORD lo; int hi; } s; long long q; } LARGE_INTEGER;

#define WINAPI __stdcall
__declspec(dllimport) HANDLE WINAPI GetStdHandle(DWORD);
__declspec(dllimport) BOOL WINAPI WriteFile(HANDLE, const void *, DWORD, DWORD *, void *);
__declspec(dllimport) void WINAPI ExitProcess(DWORD);
__declspec(dllimport) char *WINAPI GetCommandLineA(void);
__declspec(dllimport) int WINAPI lstrlenA(const char *);
__declspec(dllimport) DWORD WINAPI GetTickCount(void);
__declspec(dllimport) ULONGLONG WINAPI GetTickCount64(void);
__declspec(dllimport) ULONGLONG WINAPI VerSetConditionMask(ULONGLONG, DWORD, unsigned char);
__declspec(dllimport) void WINAPI SetLastError(DWORD);
__declspec(dllimport) DWORD WINAPI GetLastError(void);
__declspec(dllimport) HANDLE WINAPI GetProcessHeap(void);
__declspec(dllimport) void *WINAPI HeapAlloc(HANDLE, DWORD, DWORD);
__declspec(dllimport) BOOL WINAPI HeapFree(HANDLE, DWORD, void *);
__declspec(dllimport) void *WINAPI VirtualAlloc(void *, DWORD, DWORD, DWORD);
__declspec(dllimport) HANDLE WINAPI GetModuleHandleA(const char *);
__declspec(dllimport) BOOL WINAPI QueryPerformanceFrequency(LARGE_INTEGER *);
__declspec(dllimport) DWORD WINAPI GetCurrentProcessId(void);
__declspec(dllimport) HANDLE WINAPI GetCurrentProcess(void);
__declspec(dllimport) DWORD WINAPI GetEnvironmentVariableA(const char *, char *, DWORD);
__declspec(dllimport) BOOL WINAPI GetPhysicallyInstalledSystemMemory(ULONGLONG *);

static DWORD fs_read(DWORD off) {
    DWORD v;
    __asm__ volatile("movl %%fs:(%1), %0" : "=r"(v) : "r"(off));
    return v;
}

static void say(const char *s) {
    DWORD n = 0;
    WriteFile(GetStdHandle((DWORD)-11), s, lstrlenA(s), &n, 0);
}

/* Échec : numéro de la vérification et valeur fautive, puis sortie. */
static void fail(DWORD check, DWORD value) {
    static char m[] = "echec n. 00, valeur 0x00000000\r\n";
    m[9] = '0' + check / 10;
    m[10] = '0' + check % 10;
    for (int i = 0; i < 8; i++)
        m[22 + i] = "0123456789abcdef"[(value >> (28 - 4 * i)) & 15];
    say(m);
    ExitProcess(check);
}

static int contains(const char *h, const char *n) {
    for (; *h; h++) {
        const char *a = h, *b = n;
        while (*a && *b && *a == *b) a++, b++;
        if (!*b) return 1;
    }
    return 0;
}

void __stdcall entry(void) {
    say("Bonjour depuis un programme Windows 32 bits\r\n");
    /* Faute dans le code x86 : rapport de plantage, pas un blocage. */
    if (contains(GetCommandLineA(), "plantage")) *(volatile int *)0 = 1;
    /* TEB 32 bits sur FS : Self, PEB->ImageBaseAddress, pile, PID. */
    DWORD teb = fs_read(0x18);
    if (teb == 0 || fs_read(0x00) != 0xffffffffu) ExitProcess(1);
    DWORD peb = fs_read(0x30);
    if (peb == 0 || *(DWORD *)(peb + 8) != (DWORD)GetModuleHandleA(0)) ExitProcess(2);
    DWORD here = (DWORD)&teb;
    if (here >= fs_read(0x04) || here < fs_read(0x08)) ExitProcess(3);
    if (fs_read(0x20) != GetCurrentProcessId()) ExitProcess(4);
    /* Dernière erreur, aller-retour par le TEB 64 bits. */
    SetLastError(1234);
    if (GetLastError() != 1234) ExitProcess(5);
    /* Mémoire rendue par le runtime : sous 2 Go (pas LARGEADDRESSAWARE). */
    char *p = HeapAlloc(GetProcessHeap(), 8 /* HEAP_ZERO_MEMORY */, 4096);
    if (!p || (DWORD)p >= 0x80000000u || p[100] != 0) ExitProcess(6);
    p[4095] = 7;
    HeapFree(GetProcessHeap(), 0, p);
    char *v = VirtualAlloc(0, 65536, 0x3000, 4);
    if (!v || (DWORD)v >= 0x80000000u) ExitProcess(7);
    v[65535] = 1;
    /* Argument de 64 bits (deux cases) et retour dans edx:eax. */
    ULONGLONG m = VerSetConditionMask(0x1234567800000000ull, 2 /* VER_MAJORVERSION */, 3);
    if (m != 0x1234567800000018ull) fail(8, (DWORD)(m >> 32));
    ULONGLONG t = GetTickCount64();
    if ((t >> 32) > 1000) fail(15, (DWORD)(t >> 32));
    LARGE_INTEGER f;
    f.q = 0;
    if (!QueryPerformanceFrequency(&f) || f.q <= 0) ExitProcess(9);
    /* Pseudo-handle -1 conservé avec son signe. */
    if (GetCurrentProcess() != (HANDLE)-1) ExitProcess(10);
    /* Ligne de commande et environnement. */
    if (!contains(GetCommandLineA(), "argument-de-test")) ExitProcess(11);
    char buf[64];
    DWORD n = GetEnvironmentVariableA("WINLUX_WOW32_TEST", buf, sizeof buf);
    if (n != 2 || buf[0] != 'o' || buf[1] != 'k') ExitProcess(12);
    /* stdcall : l'appelé dépile ; mille appels sans dériver la pile. */
    DWORD before = (DWORD)&n;
    for (int i = 0; i < 1000; i++) {
        SetLastError(i);
        if (GetLastError() != (DWORD)i) ExitProcess(13);
        GetTickCount();
    }
    if ((DWORD)&n != before) ExitProcess(14);
    /* Mémoire installée en Kio (#1205 : installeur Creative Cloud). */
    ULONGLONG kb = 0;
    if (!GetPhysicallyInstalledSystemMemory(&kb) || kb < 65536) fail(17, (DWORD)kb);
    /* FS x86 intact après des passerelles (y compris en pas à pas). */
    if (fs_read(0x18) != teb || fs_read(0x00) != 0xffffffffu) ExitProcess(16);
    say("Tout est correct.\r\n");
    ExitProcess(0);
}

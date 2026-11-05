/* Exceptions des programmes Windows 32 bits (#232, étape 2) : SEH par
 * chaîne de cadres fs:[0], blocs __try/__except/__finally compilés par clang
 * (gestionnaire _except_handler3 de msvcrt), fautes matérielles et
 * RaiseException. Rend 0 si tout va bien, sinon le numéro du cas en échec.
 * Arguments : « nonrattrapee » (exception logicielle que personne ne prend),
 * « filtre » (filtre de dernier recours SetUnhandledExceptionFilter). */
typedef unsigned long DWORD;
typedef void *HANDLE;
typedef struct {
    DWORD ExceptionCode, ExceptionFlags;
    void *ExceptionRecord, *ExceptionAddress;
    DWORD NumberParameters, ExceptionInformation[15];
} EXCEPTION_RECORD;
typedef struct { EXCEPTION_RECORD *ExceptionRecord; void *ContextRecord; } EXCEPTION_POINTERS;
typedef long(__stdcall *TOP_FILTER)(EXCEPTION_POINTERS *);

#define WINAPI __stdcall
__declspec(dllimport) void WINAPI RaiseException(DWORD, DWORD, DWORD, const DWORD *);
__declspec(dllimport) void WINAPI ExitProcess(DWORD);
__declspec(dllimport) HANDLE WINAPI GetStdHandle(DWORD);
__declspec(dllimport) int WINAPI WriteFile(HANDLE, const void *, DWORD, DWORD *, void *);
__declspec(dllimport) char *WINAPI GetCommandLineA(void);
__declspec(dllimport) TOP_FILTER WINAPI SetUnhandledExceptionFilter(TOP_FILTER);

static void say(const char *s) {
    DWORD n = 0, len = 0;
    while (s[len]) len++;
    WriteFile(GetStdHandle((DWORD)-11), s, len, &n, 0);
}

static int contains(const char *h, const char *n) {
    for (; *h; h++) {
        const char *a = h, *b = n;
        while (*a && *b && *a == *b) a++, b++;
        if (!*b) return 1;
    }
    return 0;
}

static volatile int zero = 0;
static volatile DWORD seen_param;
static volatile int finally_ran, depth;

static int filter_params(EXCEPTION_POINTERS *p) {
    if (p->ExceptionRecord->ExceptionCode != 0xE0000001) return 0;
    if (p->ExceptionRecord->NumberParameters != 2) return 0;
    seen_param = p->ExceptionRecord->ExceptionInformation[1];
    return 1;
}

static __declspec(noinline) void raise_deep(int n) {
    volatile char pad[64];
    pad[0] = (char)n;
    depth++;
    if (n == 0) RaiseException(0xE0000002, 0, 0, 0);
    raise_deep(n - 1);
    depth += pad[0];
}

/* Dans une fonction à part : clang ne compte pas l'assembleur en ligne
 * comme une instruction qui peut lever dans le bloc __try. */
static __declspec(noinline) void illegal(void) { __asm__ volatile("ud2"); }

static long WINAPI last_resort(EXCEPTION_POINTERS *p) {
    say(p->ExceptionRecord->ExceptionCode == 0xE0000043 ? "filtre de dernier recours appele\r\n" : "mauvais code\r\n");
    ExitProcess(77);
    return 0;
}

void __stdcall entry(void) {
    const char *cmd = GetCommandLineA();
    if (contains(cmd, "nonrattrapee")) {
        RaiseException(0xE0000042, 0, 0, 0);
        ExitProcess(99);
    }
    if (contains(cmd, "filtre")) {
        SetUnhandledExceptionFilter(last_resort);
        RaiseException(0xE0000043, 0, 0, 0);
        ExitProcess(98);
    }
    int ok = 0;
    /* 1. Accès à NULL rattrapé par __except. */
    __try { *(volatile int *)0 = 1; } __except (_exception_code() == 0xC0000005) { ok = 1; }
    if (!ok) ExitProcess(1);
    /* 2. Division entière par zéro. */
    ok = 0;
    __try { ok = 10 / zero; } __except (_exception_code() == 0xC0000094) { ok = 2; }
    if (ok != 2) ExitProcess(2);
    /* 3. RaiseException avec paramètres lus par le filtre. */
    ok = 0;
    {
        DWORD args[2] = {11, 0xCAFE};
        __try { RaiseException(0xE0000001, 0, 2, args); } __except (filter_params(_exception_info())) { ok = 3; }
    }
    if (ok != 3 || seen_param != 0xCAFE) ExitProcess(3);
    /* 4. __finally exécuté pendant le déroulement vers un __except extérieur. */
    ok = 0;
    __try {
        __try { RaiseException(0xE0000003, 0, 0, 0); } __finally { finally_ran = 1; }
    } __except (1) { ok = 4; }
    if (ok != 4 || !finally_ran) ExitProcess(4);
    /* 5. Filtre qui refuse : le __except extérieur prend l'exception. */
    ok = 0;
    __try {
        __try { RaiseException(0xE0000004, 0, 0, 0); } __except (0) { ok = -1; }
    } __except (_exception_code() == 0xE0000004) { ok = 5; }
    if (ok != 5) ExitProcess(5);
    /* 6. Exception levée dix appels plus bas : cadres x86 traversés. */
    ok = 0;
    __try { raise_deep(10); } __except (_exception_code() == 0xE0000002) { ok = 6; }
    if (ok != 6 || depth != 11) ExitProcess(6);
    /* 7. Instruction illégale (ud2) : STATUS_ILLEGAL_INSTRUCTION. */
    ok = 0;
    __try { illegal(); } __except (_exception_code() == 0xC000001D) { ok = 7; }
    if (ok != 7) ExitProcess(7);
    /* 8. Continuer après RaiseException (EXCEPTION_CONTINUE_EXECUTION). */
    ok = 0;
    __try { RaiseException(0xE0000005, 0, 0, 0); ok = 8; } __except (-1) { ok = -8; }
    if (ok != 8) ExitProcess(8);
    /* 9. Chaîne fs:[0] revenue à son état : une deuxième faute est rattrapée. */
    ok = 0;
    __try { *(volatile int *)4 = 1; } __except (1) { ok = 9; }
    if (ok != 9) ExitProcess(9);
    say("SEH 32 bits : tout est correct.\r\n");
    ExitProcess(0);
}

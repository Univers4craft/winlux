// Fautes matérielles en exceptions SEH (#269), au format MSVC (clang en mode
// x86_64-pc-windows-msvc, sans CRT) : accès à NULL, division par zéro,
// int3, ud2, pas à pas, page de garde, débordement de pile, gestionnaire
// vectorisé qui corrige le contexte et reprend.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
// Avec l'argument « crash » : accès à NULL non rattrapé (rapport de plantage).
typedef unsigned long long u64;
typedef unsigned int u32;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall AddVectoredExceptionHandler(u32, long(__stdcall *)(void *));
__declspec(dllimport) u32 __stdcall RemoveVectoredExceptionHandler(void *);
__declspec(dllimport) void *__stdcall VirtualAlloc(void *, u64, u32, u32);
__declspec(dllimport) void *__stdcall CreateThread(void *, u64, u32(__stdcall *)(void *), void *, u32, u32 *);
__declspec(dllimport) u32 __stdcall WaitForSingleObject(void *, u32);
__declspec(dllimport) int __stdcall GetExitCodeThread(void *, u32 *);
__declspec(dllimport) const char *__stdcall GetCommandLineA(void);

typedef struct {
    u32 code, flags;
    void *record;
    void *address;
    u32 nparams;
    u64 info[15];
} Record;
typedef struct {
    Record *record;
    unsigned char *context;
} Pointers;

#define CTX_RCX 0x80
#define CTX_RIP 0xF8
#define CTX_EFLAGS 0x44
static u64 ctx_get(unsigned char *c, int off) { return *(u64 *)(c + off); }
static void ctx_set(unsigned char *c, int off, u64 v) { *(u64 *)(c + off) = v; }

// Sans CRT : les copies de structures appellent memcpy.
void *memcpy(void *d, const void *s, u64 n) {
    unsigned char *o = d;
    const unsigned char *i = s;
    while (n--) *o++ = *i++;
    return d;
}

static volatile int zero = 0;
static Record last;

static int keep(Pointers *p, u32 code) {
    last = *p->record;
    return p->record->code == code;
}

// 1. Écriture à NULL : EXCEPTION_ACCESS_VIOLATION, écriture, adresse 0.
__declspec(noinline) int t_null_write() {
    __try { *(volatile int *)(u64)zero = 1; return 1; }
    __except (keep(__exception_info(), 0xC0000005)) {
        return (last.nparams == 2 && last.info[0] == 1 && last.info[1] == 0) ? 0 : 1;
    }
}

// 2. Lecture à 0x10 : lecture, adresse exacte.
__declspec(noinline) int t_read_address() {
    __try { return *(volatile int *)(u64)(0x10 + zero); }
    __except (keep(__exception_info(), 0xC0000005)) {
        return (last.info[0] == 0 && last.info[1] == 0x10) ? 0 : 1;
    }
}

// 3. Division entière par zéro.
__declspec(noinline) int t_divide() {
    __try { return 10 / zero + 1; }
    __except (keep(__exception_info(), 0xC0000094)) { return 0; }
}

// 4. int3 : EXCEPTION_BREAKPOINT, adresse = l'instruction int3.
__declspec(noinline) int t_breakpoint() {
    __try { __debugbreak(); return 1; }
    __except (keep(__exception_info(), 0x80000003)) {
        return *(unsigned char *)last.address == 0xCC ? 0 : 1;
    }
}

// 5. ud2 : EXCEPTION_ILLEGAL_INSTRUCTION.
__declspec(noinline) int t_illegal() {
    __try { __builtin_trap(); return 1; }
    __except (keep(__exception_info(), 0xC000001D)) { return 0; }
}

// 6. Gestionnaire vectorisé qui corrige RCX et reprend : l'instruction
//    fautive est rejouée avec une adresse valide.
static u64 good_value = 0x1234;
static int veh_calls;
static long __stdcall fix_rcx(void *arg) {
    Pointers *p = arg;
    if (p->record->code != 0xC0000005 || ctx_get(p->context, CTX_RCX) != 0) return 0;
    veh_calls++;
    ctx_set(p->context, CTX_RCX, (u64)&good_value);
    return -1; // EXCEPTION_CONTINUE_EXECUTION
}
__declspec(noinline) int t_veh_resume() {
    void *h = AddVectoredExceptionHandler(1, fix_rcx);
    u64 v;
    __asm__ volatile("xor %%ecx, %%ecx\n\tmov (%%rcx), %0" : "=r"(v) : : "rcx");
    RemoveVectoredExceptionHandler(h);
    return (v == 0x1234 && veh_calls == 1) ? 0 : 1;
}

// 7. Filtre qui corrige le contexte et renvoie CONTINUE_EXECUTION.
static int fix_in_filter(Pointers *p) {
    if (p->record->code != 0xC0000005) return 0;
    ctx_set(p->context, CTX_RCX, (u64)&good_value);
    return -1;
}
__declspec(noinline) int t_filter_continue() {
    u64 v = 0;
    __try { __asm__ volatile("xor %%ecx, %%ecx\n\tmov (%%rcx), %0" : "=r"(v) : : "rcx"); }
    __except (fix_in_filter(__exception_info())) { return 1; }
    return v == 0x1234 ? 0 : 1;
}

// 8. Page de garde : une seule levée, puis la page est accessible.
__declspec(noinline) int t_guard_page() {
    volatile int *p = VirtualAlloc(0, 4096, 0x3000, 0x04 | 0x100);
    if (!p) return 1;
    int seen = 0;
    __try { (void)*p; }
    __except (keep(__exception_info(), 0x80000001)) { seen = 1; }
    if (!seen || last.info[1] != (u64)p) return 1;
    *p = 7;
    return *p == 7 ? 0 : 1;
}

// 9. Pas à pas : TF levé par popfq, EXCEPTION_SINGLE_STEP après une
//    instruction, contexte rendu sans TF.
__declspec(noinline) int t_single_step() {
    __try {
        __asm__ volatile("pushfq\n\torl $0x100, (%%rsp)\n\tpopfq\n\tnop\n\tnop" ::: "memory", "cc");
        return 1;
    }
    __except (keep(__exception_info(), 0x80000004)) { return 0; }
}

// 10. Débordement de pile dans un thread à petite pile : STACK_OVERFLOW
//     rattrapé, le thread continue et se termine normalement.
static int depth;
__declspec(noinline) int recurse(volatile char *prev) {
    volatile char buf[512];
    buf[0] = prev ? prev[0] + 1 : 0;
    depth++;
    return recurse(buf) + buf[1];
}
static u32 __stdcall overflow_thread(void *arg) {
    (void)arg;
    __try { return recurse(0); }
    __except (keep(__exception_info(), 0xC00000FD)) { return depth > 100 ? 77 : 2; }
}
__declspec(noinline) int t_stack_overflow() {
    void *t = CreateThread(0, 256 * 1024, overflow_thread, 0, 0, 0);
    if (!t) return 1;
    WaitForSingleObject(t, 0xFFFFFFFF);
    u32 code = 0;
    GetExitCodeThread(t, &code);
    return code == 77 ? 0 : 1;
}

// 12. Débordement de pile sur le thread principal (#15) : rattrapé aussi,
//     puis une seconde fois, preuve que la pile de secours a été rendue.
__declspec(noinline) int t_main_overflow() {
    for (int round = 0; round < 2; round++) {
        depth = 0;
        int got = 0;
        __try { recurse(0); }
        __except (keep(__exception_info(), 0xC00000FD)) { got = depth > 100; }
        if (!got) return 1;
    }
    return 0;
}

// 11. Faute matérielle sous un __finally, rattrapée plus haut : le
//     déroulement exécute le bloc __finally.
static int finally_ran;
__declspec(noinline) void inner_finally() {
    __try { *(volatile int *)(u64)zero = 0; }
    __finally { finally_ran++; }
}
__declspec(noinline) int t_finally_on_fault() {
    finally_ran = 0;
    __try { inner_finally(); }
    __except (1) { return finally_ran == 1 ? 0 : 1; }
    return 1;
}

static int has_arg(const char *line, const char *word) {
    for (; *line; line++) {
        const char *a = line, *b = word;
        while (*b && *a == *b) a++, b++;
        if (!*b) return 1;
    }
    return 0;
}

int entry() {
    if (has_arg(GetCommandLineA(), "crash")) {
        *(volatile int *)(u64)zero = 1;
        ExitProcess(99);
    }
    // Débordement non rattrapé (#15), sur le thread principal ou sur un
    // thread secondaire : rapport de plantage EXCEPTION_STACK_OVERFLOW.
    if (has_arg(GetCommandLineA(), "overflow-thread")) {
        void *t = CreateThread(0, 256 * 1024, (u32(__stdcall *)(void *))recurse, 0, 0, 0);
        WaitForSingleObject(t, 0xFFFFFFFF);
        ExitProcess(98);
    }
    if (has_arg(GetCommandLineA(), "overflow")) {
        recurse(0);
        ExitProcess(97);
    }
    int (*tests[])() = {t_null_write, t_read_address, t_divide, t_breakpoint, t_illegal, t_veh_resume,
                        t_filter_continue, t_guard_page, t_single_step, t_stack_overflow, t_finally_on_fault, t_main_overflow};
    for (unsigned i = 0; i < sizeof tests / sizeof *tests; i++)
        if (tests[i]()) ExitProcess(i + 1);
    ExitProcess(0);
    return 0;
}

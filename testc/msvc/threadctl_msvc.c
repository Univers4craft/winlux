// Suspension de threads et contexte (#269), ce dont le ramasse-miettes de
// coreclr a besoin (.NET autonome, WinForms), au format MSVC (clang en mode
// x86_64-pc-windows-msvc, sans CRT) :
//   1. SuspendThread arrête vraiment un thread qui tourne, avec le compteur
//      de suspension de Windows, et ResumeThread le relance ;
//   2. DuplicateHandle(GetCurrentThread()) donne un vrai handle, utilisable
//      depuis un autre thread ;
//   3. GetThreadContext lit RIP et RSP du thread suspendu ;
//   4. SetThreadContext le détourne vers une autre fonction (la
//      « redirection » de coreclr) ;
//   5. VirtualQuery montre une page de garde dans la pile ;
//   6. MapViewOfFileEx exécutable puis VirtualAlloc(MEM_COMMIT) sur la vue
//      (mémoire W^X de coreclr, #665) ;
//   7. RegisterWindowMessage : un numéro par nom ;
//   8. GetObjectType reconnaît un DC mémoire ;
//   9. PeekMessage(PM_NOREMOVE) voit un WM_TIMER échu et WaitMessage rend
//      la main quand il l'est.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void __stdcall ExitThread(u32);
__declspec(dllimport) void *__stdcall CreateThread(void *, u64, u32(__stdcall *)(void *), void *, u32, u32 *);
__declspec(dllimport) u32 __stdcall SuspendThread(void *);
__declspec(dllimport) u32 __stdcall ResumeThread(void *);
__declspec(dllimport) int __stdcall GetThreadContext(void *, void *);
__declspec(dllimport) int __stdcall SetThreadContext(void *, void *);
__declspec(dllimport) void *__stdcall GetCurrentThread(void);
__declspec(dllimport) void *__stdcall GetCurrentProcess(void);
__declspec(dllimport) int __stdcall DuplicateHandle(void *, void *, void *, void **, u32, int, u32);
__declspec(dllimport) u32 __stdcall WaitForSingleObject(void *, u32);
__declspec(dllimport) int __stdcall GetExitCodeThread(void *, u32 *);
__declspec(dllimport) void __stdcall Sleep(u32);
__declspec(dllimport) u64 __stdcall VirtualQuery(const void *, void *, u64);
__declspec(dllimport) void *__stdcall VirtualAlloc(void *, u64, u32, u32);
__declspec(dllimport) void *__stdcall CreateFileMappingW(void *, void *, u32, u32, u32, const u16 *);
__declspec(dllimport) void *__stdcall MapViewOfFileEx(void *, u32, u32, u32, u64, void *);
__declspec(dllimport) u32 __stdcall RegisterWindowMessageW(const u16 *);
__declspec(dllimport) u64 __stdcall SetTimer(void *, u64, u32, void *);
__declspec(dllimport) int __stdcall KillTimer(void *, u64);
__declspec(dllimport) int __stdcall PeekMessageW(void *, void *, u32, u32, u32);
__declspec(dllimport) int __stdcall WaitMessage(void);
__declspec(dllimport) void *__stdcall CreateCompatibleDC(void *);
__declspec(dllimport) u32 __stdcall GetObjectType(void *);
__declspec(dllimport) int __stdcall DeleteDC(void *);

void *memset(void *d, int c, u64 n) {
    unsigned char *o = d;
    while (n--) *o++ = (unsigned char)c;
    return d;
}

#define CONTEXT_AMD64 0x100000u
#define CONTEXT_CONTROL (CONTEXT_AMD64 | 1u)
#define CONTEXT_INTEGER (CONTEXT_AMD64 | 2u)
#define CTX_FLAGS 0x30
#define CTX_RSP 0x98
#define CTX_RIP 0xF8

typedef struct {
    u64 base, alloc_base;
    u32 alloc_protect;
    u16 partition, pad;
    u64 size;
    u32 state, protect, type, pad2;
} Mbi;

static __declspec(align(16)) unsigned char ctx[0x4D0];
static volatile u64 counter;
static volatile int stop;
static volatile int redirected;
static void *volatile own_handle;

static u32 __stdcall spin(void *arg) {
    (void)arg;
    void *self = 0;
    DuplicateHandle(GetCurrentProcess(), GetCurrentThread(), GetCurrentProcess(), &self, 0, 0, 2);
    own_handle = self;
    while (!stop) counter++;
    return 7;
}

// Point d'arrivée du détournement : ne revient jamais à l'appelant.
static void detour(void) {
    redirected = 1;
    ExitThread(42);
}

static u64 ctx_get(int off) { return *(u64 *)(ctx + off); }
static void ctx_set(int off, u64 v) { *(u64 *)(ctx + off) = v; }

static u32 suspend_tests(void) {
    void *t = CreateThread(0, 0, spin, 0, 0, 0);
    if (!t) return 1;
    while (!own_handle || counter < 1000) Sleep(1);
    void *h = own_handle;
    // 1-2. Suspension par le handle dupliqué, compteur de suspension.
    if (SuspendThread(h) != 0) return 1;
    u64 a = counter;
    Sleep(30);
    if (counter != a) return 1;
    if (SuspendThread(t) != 1) return 2;
    // 3. Registres du thread arrêté.
    memset(ctx, 0, sizeof ctx);
    *(u32 *)(ctx + CTX_FLAGS) = CONTEXT_CONTROL | CONTEXT_INTEGER;
    if (!GetThreadContext(t, ctx) || ctx_get(CTX_RIP) == 0 || ctx_get(CTX_RSP) == 0) return 3;
    if (ResumeThread(t) != 2 || counter != a) return 1;
    if (ResumeThread(h) != 1) return 1;
    Sleep(20);
    if (counter == a) return 1;
    // 4. Détournement : RIP vers detour, pile alignée comme après un call.
    if (SuspendThread(t) != 0) return 4;
    memset(ctx, 0, sizeof ctx);
    *(u32 *)(ctx + CTX_FLAGS) = CONTEXT_CONTROL;
    if (!GetThreadContext(t, ctx)) return 4;
    ctx_set(CTX_RIP, (u64)detour);
    ctx_set(CTX_RSP, ((ctx_get(CTX_RSP) - 256) & ~15ull) - 8);
    if (!SetThreadContext(t, ctx)) return 4;
    if (ResumeThread(t) != 1) return 4;
    if (WaitForSingleObject(t, 5000) != 0) return 4;
    u32 code = 0;
    if (!GetExitCodeThread(t, &code) || code != 42 || !redirected) return 4;
    return 0;
}

static int stack_guard(void) {
    Mbi m;
    volatile int local = 0;
    if (VirtualQuery((const void *)&local, &m, sizeof m) != sizeof m) return 5;
    u64 at = m.alloc_base, top = (u64)&local;
    while (at < top) {
        Mbi r;
        if (VirtualQuery((const void *)at, &r, sizeof r) != sizeof r || r.size == 0) return 5;
        if (r.alloc_base != m.alloc_base) return 5;
        if (r.state == 0x1000 && (r.protect & 0x100)) return 0; // PAGE_GUARD
        at = r.base + r.size;
    }
    return 5;
}

static int wx_view(void) {
    const u64 size = 1 << 20;
    // PAGE_EXECUTE_READWRITE | SEC_RESERVE
    void *s = CreateFileMappingW((void *)-1, 0, 0x40 | 0x4000000, 0, (u32)size, 0);
    if (!s) return 6;
    // FILE_MAP_WRITE | FILE_MAP_READ | FILE_MAP_EXECUTE
    unsigned char *v = MapViewOfFileEx(s, 0x2 | 0x4 | 0x20, 0, 0, size, 0);
    if (!v) return 6;
    if (VirtualAlloc(v, 4096, 0x1000, 0x04) != v) return 6;
    v[0] = 0xC3;
    return v[0] == 0xC3 ? 0 : 6;
}

static int messages(void) {
    static const u16 a[] = {'W', 'l', 'x', 'A', 0}, b[] = {'W', 'l', 'x', 'B', 0};
    u32 ma = RegisterWindowMessageW(a), mb = RegisterWindowMessageW(b);
    if (ma < 0xC000 || mb < 0xC000 || ma == mb || RegisterWindowMessageW(a) != ma) return 7;
    void *dc = CreateCompatibleDC(0);
    if (!dc || GetObjectType(dc) != 10) return 8; // OBJ_MEMDC
    DeleteDC(dc);
    unsigned char msg[48];
    u64 id = SetTimer(0, 0, 10, 0);
    if (!id) return 9;
    WaitMessage();
    Sleep(30);
    if (!PeekMessageW(msg, 0, 0, 0, 0) || *(u32 *)(msg + 8) != 0x113) return 9;
    if (!PeekMessageW(msg, 0, 0x113, 0x113, 1) || *(u32 *)(msg + 8) != 0x113) return 9;
    KillTimer(0, id);
    return 0;
}

void entry(void) {
    u32 r = suspend_tests();
    if (!r) r = stack_guard();
    if (!r) r = wx_view();
    if (!r) r = messages();
    stop = 1;
    ExitProcess(r);
}

// DuplicateHandle et mémoire entre deux processus, comme le bac à sable de
// Chromium et Mojo (Electron, #236). Format MSVC (clang, sans CRT).
// Le parent se relance lui-même SUSPENDU, puis, avant ResumeThread :
//   - lit le PEB de l'enfant (NtQueryInformationProcess + ReadProcessMemory)
//     pour trouver son image, et vérifie la signature MZ ;
//   - copie dans l'enfant un mapping de mémoire partagée (DuplicateHandle
//     vers un autre processus) ;
//   - écrit dans une variable de l'enfant (WriteProcessMemory) le numéro du
//     handle copié, celui d'un de ses propres événements et son PID.
// L'enfant lit la mémoire partagée, y répond, reprend l'événement du parent
// (DuplicateHandle DEPUIS un autre processus) et le signale.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté
// (à partir de 40 : échec dans l'enfant, renvoyé par le parent + 100).
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall CreateEventW(void *, int, int, const u16 *);
__declspec(dllimport) int __stdcall SetEvent(void *);
__declspec(dllimport) u32 __stdcall WaitForSingleObject(void *, u32);
__declspec(dllimport) int __stdcall DuplicateHandle(void *, void *, void *, void **, u32, int, u32);
__declspec(dllimport) void *__stdcall GetCurrentProcess(void);
__declspec(dllimport) u32 __stdcall GetCurrentProcessId(void);
__declspec(dllimport) void *__stdcall OpenProcess(u32, int, u32);
__declspec(dllimport) int __stdcall CreateProcessW(const u16 *, u16 *, void *, void *, int, u32, void *, const u16 *, void *, void *);
__declspec(dllimport) u32 __stdcall ResumeThread(void *);
__declspec(dllimport) int __stdcall GetExitCodeProcess(void *, u32 *);
__declspec(dllimport) u32 __stdcall GetModuleFileNameW(void *, u16 *, u32);
__declspec(dllimport) void *__stdcall GetModuleHandleW(const u16 *);
__declspec(dllimport) u16 *__stdcall GetCommandLineW(void);
__declspec(dllimport) void *__stdcall CreateFileMappingW(void *, void *, u32, u32, u32, const u16 *);
__declspec(dllimport) void *__stdcall MapViewOfFile(void *, u32, u32, u32, u64);
__declspec(dllimport) int __stdcall ReadProcessMemory(void *, const void *, void *, u64, u64 *);
__declspec(dllimport) int __stdcall WriteProcessMemory(void *, void *, const void *, u64, u64 *);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) int __stdcall NtQueryInformationProcess(void *, u32, void *, u32, u32 *);

void *memset(void *d, int c, u64 n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}
void *memcpy(void *d, const void *s, u64 n) {
    u8 *o = d;
    const u8 *i = s;
    while (n--) *o++ = *i++;
    return d;
}

#define MAGIC 0x57696e6c75784455ull
// Écrite par le parent pendant que l'enfant est suspendu.
struct transfer {
    u64 magic;
    u64 mapping;
    u64 parent_event;
    u64 parent_pid;
};
volatile struct transfer g_transfer;

struct startup {
    u32 cb;
    u32 pad0;
    void *reserved, *desktop, *title;
    u32 x, y, xs, ys, xc, yc, fill, flags;
    u16 show, r2;
    u32 pad1;
    void *r3, *in, *out, *err;
};
struct proc_info {
    void *process, *thread;
    u32 pid, tid;
};

static int same(const char *a, const char *b, int n) {
    for (int i = 0; i < n; i++)
        if (a[i] != b[i]) return 0;
    return 1;
}
static int is_child(const u16 *s) {
    static const char w[] = "--enfant-dupx";
    for (; *s; s++) {
        int i = 0;
        while (w[i] && s[i] == (u16)w[i]) i++;
        if (!w[i]) return 1;
    }
    return 0;
}

static void child(void) {
    if (g_transfer.magic != MAGIC) ExitProcess(40);
    char *view = MapViewOfFile((void *)g_transfer.mapping, 6, 0, 0, 0);
    if (!view) ExitProcess(41);
    if (!same(view, "bonjour", 8)) ExitProcess(42);
    memcpy(view, "merci", 6);
    void *parent = OpenProcess(0x40 /* PROCESS_DUP_HANDLE */, 0, (u32)g_transfer.parent_pid);
    if (!parent) ExitProcess(43);
    void *ev = 0;
    if (!DuplicateHandle(parent, (void *)g_transfer.parent_event, GetCurrentProcess(), &ev, 0, 0, 2))
        ExitProcess(44);
    if (!SetEvent(ev)) ExitProcess(45);
    ExitProcess(0);
}

void entry(void) {
    if (is_child(GetCommandLineW())) child();
    // 1. Mémoire partagée et événement du parent.
    void *map = CreateFileMappingW((void *)-1, 0, 4 /* PAGE_READWRITE */, 0, 4096, 0);
    char *view = map ? MapViewOfFile(map, 6, 0, 0, 0) : 0;
    if (!view) ExitProcess(1);
    memcpy(view, "bonjour", 8);
    void *ev = CreateEventW(0, 1, 0, 0);
    if (!ev) ExitProcess(2);
    // 2. Enfant suspendu.
    u16 exe[512];
    if (!GetModuleFileNameW(0, exe, 512)) ExitProcess(3);
    u16 line[600];
    u32 n = 0;
    line[n++] = '"';
    for (u32 i = 0; exe[i]; i++) line[n++] = exe[i];
    line[n++] = '"';
    const char *arg = " --enfant-dupx";
    for (u32 i = 0; arg[i]; i++) line[n++] = arg[i];
    line[n] = 0;
    struct startup si;
    memset(&si, 0, sizeof si);
    si.cb = sizeof si;
    struct proc_info pi;
    if (!CreateProcessW(exe, line, 0, 0, 0, 4 /* CREATE_SUSPENDED */, 0, 0, &si, &pi)) ExitProcess(4);
    // 3. PEB et image de l'enfant.
    u64 basic[6];
    u32 got = 0;
    if (NtQueryInformationProcess(pi.process, 0, basic, sizeof basic, &got) != 0) ExitProcess(5);
    if ((u32)basic[2] != pi.pid) ExitProcess(6);
    u64 base = 0, done = 0;
    if (!ReadProcessMemory(pi.process, (void *)(basic[1] + 0x10), &base, 8, &done) || done != 8)
        ExitProcess(7);
    char mz[2] = {0, 0};
    if (!ReadProcessMemory(pi.process, (void *)base, mz, 2, &done) || !same(mz, "MZ", 2)) ExitProcess(8);
    // 4. Le mapping copié dans l'enfant, par DuplicateHandle.
    void *remote = 0;
    if (!DuplicateHandle(GetCurrentProcess(), map, pi.process, &remote, 6, 0, 0) || !remote)
        ExitProcess(9);
    // 5. Variables de l'enfant, à la même place relative dans son image.
    struct transfer t = {MAGIC, (u64)remote, (u64)ev, GetCurrentProcessId()};
    u64 mine = (u64)GetModuleHandleW(0);
    void *where = (void *)(base + ((u64)&g_transfer - mine));
    if (!WriteProcessMemory(pi.process, where, &t, sizeof t, &done) || done != sizeof t) ExitProcess(10);
    // 6. Départ de l'enfant.
    if (ResumeThread(pi.thread) == (u32)-1) ExitProcess(11);
    if (WaitForSingleObject(ev, 15000) != 0) ExitProcess(12);
    if (WaitForSingleObject(pi.process, 15000) != 0) ExitProcess(13);
    u32 code = 99;
    GetExitCodeProcess(pi.process, &code);
    if (code != 0) ExitProcess(100 + code);
    if (!same(view, "merci", 6)) ExitProcess(14);
    CloseHandle(pi.process);
    ExitProcess(0);
}

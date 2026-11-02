// Héritage des handles entre processus (crashpad de Chromium et d'Electron),
// au format MSVC (clang en mode x86_64-pc-windows-msvc, sans CRT).
// Le parent crée des handles héritables (SECURITY_ATTRIBUTES,
// SetHandleInformation, DuplicateHandle du processus courant), restreint la
// liste par PROC_THREAD_ATTRIBUTE_HANDLE_LIST, puis se relance lui-même avec
// les valeurs des handles en texte, comme crashpad avec son gestionnaire :
//   - deux événements, signalés d'un processus à l'autre ;
//   - la première instance d'un tube nommé, que l'enfant attend
//     (ConnectNamedPipe) et à laquelle le parent se connecte par son nom ;
//   - le bout écriture d'un tube anonyme ;
//   - un handle vers le parent (GetProcessId).
// Un handle non héritable et un handle héritable mais hors de la liste
// doivent rester invalides chez l'enfant.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté
// (à partir de 40 : échec dans l'enfant).
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall CreateEventW(void *, int, int, const u16 *);
__declspec(dllimport) int __stdcall SetEvent(void *);
__declspec(dllimport) u32 __stdcall WaitForSingleObject(void *, u32);
__declspec(dllimport) void *__stdcall CreateNamedPipeW(const u16 *, u32, u32, u32, u32, u32, u32, void *);
__declspec(dllimport) int __stdcall ConnectNamedPipe(void *, void *);
__declspec(dllimport) int __stdcall WaitNamedPipeW(const u16 *, u32);
__declspec(dllimport) int __stdcall CreatePipe(void **, void **, void *, u32);
__declspec(dllimport) int __stdcall DuplicateHandle(void *, void *, void *, void **, u32, int, u32);
__declspec(dllimport) void *__stdcall GetCurrentProcess(void);
__declspec(dllimport) u32 __stdcall GetCurrentProcessId(void);
__declspec(dllimport) u32 __stdcall GetProcessId(void *);
__declspec(dllimport) int __stdcall GetHandleInformation(void *, u32 *);
__declspec(dllimport) int __stdcall SetHandleInformation(void *, u32, u32);
__declspec(dllimport) int __stdcall InitializeProcThreadAttributeList(void *, u32, u32, u64 *);
__declspec(dllimport) int __stdcall UpdateProcThreadAttribute(void *, u32, u64, void *, u64, void *, void *);
__declspec(dllimport) int __stdcall CreateProcessW(const u16 *, u16 *, void *, void *, int, u32, void *, const u16 *, void *, void *);
__declspec(dllimport) int __stdcall GetExitCodeProcess(void *, u32 *);
__declspec(dllimport) u32 __stdcall GetModuleFileNameW(void *, u16 *, u32);
__declspec(dllimport) u16 *__stdcall GetCommandLineW(void);
__declspec(dllimport) void *__stdcall CreateFileW(const u16 *, u32, u32, void *, u32, u32, void *);
__declspec(dllimport) int __stdcall ReadFile(void *, void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall WriteFile(void *, const void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) void __stdcall Sleep(u32);

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
u64 wcslen(const u16 *s) {
    u64 n = 0;
    while (s[n]) n++;
    return n;
}

#define INVALID ((void *)-1)
#define WAIT_FAILED 0xFFFFFFFFu

typedef struct { u32 length; void *descriptor; int inherit; } SecurityAttributes;
typedef struct { void *process, *thread; u32 pid, tid; } ProcessInformation;

static u16 *append(u16 *p, const char *s) {
    while (*s) *p++ = (u16)*s++;
    *p = 0;
    return p;
}
static u16 *append_hex(u16 *p, u64 v) {
    char tmp[17];
    int n = 0;
    do {
        tmp[n++] = "0123456789abcdef"[v & 15];
        v >>= 4;
    } while (v);
    while (n) *p++ = (u16)tmp[--n];
    *p++ = ' ';
    *p = 0;
    return p;
}
static const u16 *read_hex(const u16 *p, u64 *v) {
    while (*p == ' ') p++;
    *v = 0;
    for (;; p++) {
        u16 c = *p;
        if (c >= '0' && c <= '9') *v = *v * 16 + (c - '0');
        else if (c >= 'a' && c <= 'f') *v = *v * 16 + (c - 'a' + 10);
        else break;
    }
    return p;
}
static void pipe_name(u16 *out, u32 pid) {
    u16 *p = append(out, "\\\\.\\pipe\\test_heritage_");
    p = append_hex(p, pid);
    p[-1] = 0; // pas d'espace final
}
static const u16 *find(const u16 *s, const char *word) {
    for (; *s; s++) {
        const u16 *x = s;
        const char *y = word;
        while (*y && *x == (u16)*y) x++, y++;
        if (!*y) return x;
    }
    return 0;
}

static void child(const u16 *args) {
    u64 req, done, pipe, wr, parent, excluded, priv, ppid;
    args = read_hex(args, &req);
    args = read_hex(args, &done);
    args = read_hex(args, &pipe);
    args = read_hex(args, &wr);
    args = read_hex(args, &parent);
    args = read_hex(args, &excluded);
    args = read_hex(args, &priv);
    read_hex(args, &ppid);
    u32 flags = 0;
    if (!GetHandleInformation((void *)req, &flags) || !(flags & 1)) ExitProcess(40);
    // Hors liste ou non héritable : absents.
    if (WaitForSingleObject((void *)excluded, 0) != WAIT_FAILED) ExitProcess(41);
    if (WaitForSingleObject((void *)priv, 0) != WAIT_FAILED) ExitProcess(42);
    if (GetProcessId((void *)parent) != (u32)ppid) ExitProcess(43);
    // Événement signalé par le parent.
    if (WaitForSingleObject((void *)req, 10000) != 0) ExitProcess(44);
    u32 n = 0;
    if (!WriteFile((void *)wr, "ok", 2, &n, 0) || n != 2) ExitProcess(45);
    // Tube nommé : le parent se connecte par son nom.
    if (!ConnectNamedPipe((void *)pipe, 0)) ExitProcess(46);
    char buf[8];
    u32 got = 0;
    while (got < 4) {
        if (!ReadFile((void *)pipe, buf + got, 4 - got, &n, 0) || n == 0) ExitProcess(47);
        got += n;
    }
    if (buf[0] != 'p' || buf[1] != 'i' || buf[2] != 'n' || buf[3] != 'g') ExitProcess(48);
    if (!WriteFile((void *)pipe, "PONG", 4, &n, 0) || n != 4) ExitProcess(49);
    if (!SetEvent((void *)done)) ExitProcess(50);
    ExitProcess(0);
}

void entry(void) {
    const u16 *line = GetCommandLineW();
    const u16 *args = find(line, " enfant ");
    if (args) child(args);

    SecurityAttributes sa = {sizeof sa, 0, 1};
    // Créés en premier : leurs valeurs sont sous celles des handles hérités.
    void *priv = CreateEventW(0, 1, 1, 0);
    void *excluded = CreateEventW(0, 1, 1, 0);
    if (!priv || !excluded) ExitProcess(1);
    u32 flags = 7;
    if (!GetHandleInformation(excluded, &flags) || flags != 0) ExitProcess(2);
    if (!SetHandleInformation(excluded, 1, 1) || !GetHandleInformation(excluded, &flags) || flags != 1)
        ExitProcess(3);
    void *req = CreateEventW(&sa, 0, 0, 0);
    void *done = CreateEventW(&sa, 1, 0, 0);
    if (!req || !done) ExitProcess(4);
    u16 name[64];
    pipe_name(name, GetCurrentProcessId());
    void *pipe = CreateNamedPipeW(name, 3 /* PIPE_ACCESS_DUPLEX */ | 0x80000 /* FIRST_PIPE_INSTANCE */,
                                  0, 1, 4096, 4096, 0, &sa);
    if (pipe == INVALID) ExitProcess(5);
    void *rd = 0, *wr = 0;
    if (!CreatePipe(&rd, &wr, &sa, 0)) ExitProcess(6);
    void *self = 0;
    if (!DuplicateHandle(GetCurrentProcess(), GetCurrentProcess(), GetCurrentProcess(), &self, 0, 1, 2) ||
        self == GetCurrentProcess())
        ExitProcess(7);

    // Liste restreinte : tout sauf `excluded` (et `rd`).
    static u8 attrs[256];
    u64 size = sizeof attrs;
    if (!InitializeProcThreadAttributeList(attrs, 1, 0, &size)) ExitProcess(8);
    void *list[5] = {req, done, pipe, wr, self};
    if (!UpdateProcThreadAttribute(attrs, 0, 0x20002, list, sizeof list, 0, 0)) ExitProcess(9);
    u8 si[112];
    memset(si, 0, sizeof si);
    *(u32 *)si = sizeof si;
    *(void **)(si + 104) = attrs;

    static u16 exe[512];
    if (!GetModuleFileNameW(0, exe, 512)) ExitProcess(10);
    static u16 cmd[1024];
    u16 *p = append(cmd, "\"");
    p = append(p, "");
    for (u16 *e = exe; *e; e++) *p++ = *e;
    p = append(p, "\" enfant ");
    p = append_hex(p, (u64)req);
    p = append_hex(p, (u64)done);
    p = append_hex(p, (u64)pipe);
    p = append_hex(p, (u64)wr);
    p = append_hex(p, (u64)self);
    p = append_hex(p, (u64)excluded);
    p = append_hex(p, (u64)priv);
    append_hex(p, GetCurrentProcessId());
    ProcessInformation pi;
    memset(&pi, 0, sizeof pi);
    if (!CreateProcessW(exe, cmd, 0, 0, 1, 0x80000 /* EXTENDED_STARTUPINFO_PRESENT */, 0, 0, si, &pi))
        ExitProcess(11);

    if (!SetEvent(req)) ExitProcess(12);
    char buf[8];
    u32 n = 0;
    if (!ReadFile(rd, buf, 2, &n, 0) || n != 2 || buf[0] != 'o' || buf[1] != 'k') ExitProcess(13);
    // L'instance est passée à l'enfant : connexion par le nom, comme un
    // processus de rendu de Chromium.
    void *client = INVALID;
    for (int i = 0; i < 500 && client == INVALID; i++) {
        WaitNamedPipeW(name, 20);
        client = CreateFileW(name, 0xC0000000, 0, 0, 3 /* OPEN_EXISTING */, 0, 0);
        if (client == INVALID) Sleep(10);
    }
    if (client == INVALID) ExitProcess(14);
    if (!WriteFile(client, "ping", 4, &n, 0) || n != 4) ExitProcess(15);
    u32 got = 0;
    while (got < 4) {
        if (!ReadFile(client, buf + got, 4 - got, &n, 0) || n == 0) ExitProcess(16);
        got += n;
    }
    if (buf[0] != 'P' || buf[1] != 'O' || buf[2] != 'N' || buf[3] != 'G') ExitProcess(17);
    if (WaitForSingleObject(done, 10000) != 0) ExitProcess(18);
    if (WaitForSingleObject(pi.process, 10000) != 0) ExitProcess(19);
    u32 code = 1;
    if (!GetExitCodeProcess(pi.process, &code)) ExitProcess(20);
    if (code != 0) ExitProcess(code);
    CloseHandle(client);
    ExitProcess(0);
}

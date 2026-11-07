/* Personnalité WOW64 d'un programme x86 sous Winlux (#232, étape 7).
 * Construit par clang --target=i686-pc-windows-msvc et lld-link, sans CRT.
 * Vérifie ce que voit un programme 32 bits sous un Windows 64 bits, et que
 * les structures remplies par kernel32 gardent leur taille x86 (une garde
 * placée juste après chaque structure ne doit pas bouger) :
 *  - IsWow64Process, IsWow64Process2, GetSystemInfo / GetNativeSystemInfo ;
 *  - VirtualQuery, GetStartupInfoA, GlobalMemoryStatus ;
 *  - CRITICAL_SECTION (24 octets), SRWLOCK et INIT_ONCE (4), SLIST_HEADER (8) ;
 *  - handles rendus par pointeur (RegCreateKeyExA, CreatePipe) ;
 *  - registre : HKLM\Software\X écrit sous Wow6432Node ;
 *  - fichiers : System32 → SysWOW64, Wow64DisableWow64FsRedirection ;
 *  - variables PROCESSOR_ARCHITECTURE / PROCESSOR_ARCHITEW6432 / ProgramFiles ;
 *  - CreateProcessA : PROCESS_INFORMATION de 16 octets.
 * Rend 0 si tout va bien, sinon le numéro de la vérification en échec. */
typedef unsigned int DWORD;
typedef unsigned short WORD;
typedef unsigned char BYTE;
typedef int BOOL;
typedef void *HANDLE;
typedef void *HKEY;

#define WINAPI __stdcall
#define GUARD 0x5A5A5A5Au
__declspec(dllimport) HANDLE WINAPI GetStdHandle(DWORD);
__declspec(dllimport) BOOL WINAPI WriteFile(HANDLE, const void *, DWORD, DWORD *, void *);
__declspec(dllimport) void WINAPI ExitProcess(DWORD);
__declspec(dllimport) char *WINAPI GetCommandLineA(void);
__declspec(dllimport) int WINAPI lstrlenA(const char *);
__declspec(dllimport) HANDLE WINAPI GetCurrentProcess(void);
__declspec(dllimport) BOOL WINAPI IsWow64Process(HANDLE, BOOL *);
__declspec(dllimport) BOOL WINAPI IsWow64Process2(HANDLE, WORD *, WORD *);
__declspec(dllimport) void WINAPI GetSystemInfo(void *);
__declspec(dllimport) void WINAPI GetNativeSystemInfo(void *);
__declspec(dllimport) DWORD WINAPI VirtualQuery(const void *, void *, DWORD);
__declspec(dllimport) void WINAPI GetStartupInfoA(void *);
__declspec(dllimport) void WINAPI GlobalMemoryStatus(void *);
__declspec(dllimport) void WINAPI InitializeCriticalSection(void *);
__declspec(dllimport) BOOL WINAPI InitializeCriticalSectionAndSpinCount(void *, DWORD);
__declspec(dllimport) void WINAPI EnterCriticalSection(void *);
__declspec(dllimport) void WINAPI LeaveCriticalSection(void *);
__declspec(dllimport) void WINAPI DeleteCriticalSection(void *);
__declspec(dllimport) void WINAPI InitializeSRWLock(void *);
__declspec(dllimport) void WINAPI AcquireSRWLockExclusive(void *);
__declspec(dllimport) void WINAPI ReleaseSRWLockExclusive(void *);
__declspec(dllimport) void WINAPI InitOnceInitialize(void *);
__declspec(dllimport) BOOL WINAPI InitOnceBeginInitialize(void *, DWORD, BOOL *, void **);
__declspec(dllimport) BOOL WINAPI InitOnceComplete(void *, DWORD, void *);
__declspec(dllimport) void WINAPI InitializeSListHead(void *);
__declspec(dllimport) void *WINAPI InterlockedPushEntrySList(void *, void *);
__declspec(dllimport) void *WINAPI InterlockedPopEntrySList(void *);
__declspec(dllimport) WORD WINAPI QueryDepthSList(void *);
__declspec(dllimport) BOOL WINAPI CreatePipe(HANDLE *, HANDLE *, void *, DWORD);
__declspec(dllimport) BOOL WINAPI CloseHandle(HANDLE);
__declspec(dllimport) DWORD WINAPI GetEnvironmentVariableA(const char *, char *, DWORD);
__declspec(dllimport) DWORD WINAPI GetSystemWow64DirectoryA(char *, DWORD);
__declspec(dllimport) BOOL WINAPI Wow64DisableWow64FsRedirection(void **);
__declspec(dllimport) BOOL WINAPI Wow64RevertWow64FsRedirection(void *);
__declspec(dllimport) HANDLE WINAPI CreateFileA(const char *, DWORD, DWORD, void *, DWORD, DWORD, HANDLE);
__declspec(dllimport) BOOL WINAPI CreateProcessA(const char *, char *, void *, void *, BOOL, DWORD,
                                                 void *, const char *, void *, void *);
__declspec(dllimport) DWORD WINAPI WaitForSingleObject(HANDLE, DWORD);
__declspec(dllimport) BOOL WINAPI GetExitCodeProcess(HANDLE, DWORD *);
__declspec(dllimport) DWORD WINAPI GetModuleFileNameA(HANDLE, char *, DWORD);
__declspec(dllimport) long WINAPI RegCreateKeyExA(HKEY, const char *, DWORD, char *, DWORD, DWORD, void *,
                                                  HKEY *, DWORD *);
__declspec(dllimport) long WINAPI RegOpenKeyExA(HKEY, const char *, DWORD, DWORD, HKEY *);
__declspec(dllimport) long WINAPI RegSetValueExA(HKEY, const char *, DWORD, DWORD, const void *, DWORD);
__declspec(dllimport) long WINAPI RegCloseKey(HKEY);
__declspec(dllimport) long WINAPI RegDeleteKeyA(HKEY, const char *);
__declspec(dllimport) long WINAPI InterlockedIncrement(long volatile *);
__declspec(dllimport) long WINAPI InterlockedDecrement(long volatile *);
__declspec(dllimport) long WINAPI InterlockedExchange(long volatile *, long);
__declspec(dllimport) long WINAPI InterlockedCompareExchange(long volatile *, long, long);
__declspec(dllimport) WORD WINAPI GlobalAddAtomA(const char *);
__declspec(dllimport) WORD WINAPI GlobalFindAtomA(const char *);
__declspec(dllimport) WORD WINAPI GlobalDeleteAtom(WORD);
__declspec(dllimport) DWORD WINAPI SetHandleCount(DWORD);
__declspec(dllimport) HANDLE WINAPI LoadLibraryA(const char *);
__declspec(dllimport) void *WINAPI GetProcAddress(HANDLE, const char *);
/* msvcrt x86 : aides du compilateur et données. */
__declspec(dllimport) extern int _adjust_fdiv;
__declspec(dllimport) unsigned int __cdecl _controlfp(unsigned int, unsigned int);
long long __cdecl _ftol(void);
/* Sans CRT : symbole attendu par l'éditeur de liens dès qu'un double apparaît. */
int _fltused = 0;

#define HKLM ((HKEY)0x80000002)
#define KEY_ALL_ACCESS 0xF003F
#define KEY_READ 0x20019
#define KEY_WOW64_64KEY 0x100
#define KEY_WOW64_32KEY 0x200

static void say(const char *s) {
    DWORD n = 0;
    WriteFile(GetStdHandle((DWORD)-11), s, lstrlenA(s), &n, 0);
}

static void fail(DWORD check, DWORD value) {
    static char m[] = "echec n. 00, valeur 0x00000000\r\n";
    m[9] = '0' + check / 10;
    m[10] = '0' + check % 10;
    for (int i = 0; i < 8; i++)
        m[22 + i] = "0123456789abcdef"[(value >> (28 - 4 * i)) & 15];
    say(m);
    ExitProcess(check);
}

static int same(const char *a, const char *b) {
    while (*a && *a == *b) a++, b++;
    return *a == *b;
}

static int contains(const char *h, const char *n) {
    for (; *h; h++) {
        const char *a = h, *b = n;
        while (*a && *b && *a == *b) a++, b++;
        if (!*b) return 1;
    }
    return 0;
}

static void check_env(DWORD check, const char *name, const char *want) {
    char buf[128];
    DWORD n = GetEnvironmentVariableA(name, buf, sizeof buf);
    if (n == 0 || n >= sizeof buf || !same(buf, want)) {
        say(name);
        say(" = ");
        say(n ? buf : "(absente)");
        say("\r\n");
        fail(check, n);
    }
}

/* Une structure x86 suivie d'une garde. */
struct guarded {
    DWORD body[32];
};

static void fill(struct guarded *g) {
    for (int i = 0; i < 32; i++) g->body[i] = GUARD;
}

static void guard_at(DWORD check, struct guarded *g, int words) {
    if (g->body[words] != GUARD) fail(check, words);
}

static void touch(const char *path) {
    HANDLE h = CreateFileA(path, 0x40000000, 0, 0, 2 /* CREATE_ALWAYS */, 0x80, 0);
    if (h == (HANDLE)-1) {
        say(path);
        say("\r\n");
        fail(40, 0);
    }
    CloseHandle(h);
}

__attribute__((used)) void __stdcall entry(void) {
    struct guarded g;
    BOOL wow = 0;
    char *cmd = GetCommandLineA();
    if (contains(cmd, "enfant")) {
        /* Processus fils : il hérite de la personnalité 32 bits. */
        check_env(60, "PROCESSOR_ARCHITECTURE", "x86");
        ExitProcess(42);
    }

    /* 1-3 : IsWow64Process / IsWow64Process2. */
    if (!IsWow64Process(GetCurrentProcess(), &wow) || wow != 1) fail(1, wow);
    {
        WORD m[4] = {0, 0x5A5A, 0, 0x5A5A};
        if (!IsWow64Process2(GetCurrentProcess(), &m[0], &m[2])) fail(2, 0);
        if (m[0] != 0x14c || m[2] != 0x8664 || m[1] != 0x5A5A || m[3] != 0x5A5A)
            fail(3, m[0] | (DWORD)m[2] << 16);
    }

    /* 4-7 : SYSTEM_INFO de 36 octets, processeur x86 puis AMD64. */
    fill(&g);
    GetSystemInfo(&g);
    guard_at(4, &g, 9);
    if ((g.body[0] & 0xffff) != 0 || g.body[1] != 4096 || g.body[3] >= 0x80000000u ||
        g.body[3] < 0x7ffe0000u || g.body[5] == 0)
        fail(5, g.body[3]);
    fill(&g);
    GetNativeSystemInfo(&g);
    guard_at(6, &g, 9);
    if ((g.body[0] & 0xffff) != 9) fail(7, g.body[0]);

    /* 8-9 : MEMORY_BASIC_INFORMATION de 28 octets. */
    fill(&g);
    if (VirtualQuery((void *)&entry, &g, 28) != 28) fail(8, 0);
    guard_at(9, &g, 7);
    if (g.body[0] > (DWORD)&entry || g.body[4] != 0x1000 /* MEM_COMMIT */) fail(9, g.body[4]);

    /* 10-11 : STARTUPINFOA de 68 octets. */
    fill(&g);
    GetStartupInfoA(&g);
    guard_at(10, &g, 17);
    if (g.body[0] != 68) fail(11, g.body[0]);

    /* 12-13 : MEMORYSTATUS de 32 octets. */
    fill(&g);
    GlobalMemoryStatus(&g);
    guard_at(12, &g, 8);
    if (g.body[0] != 32 || g.body[6] == 0) fail(13, g.body[0]);

    /* 14-17 : CRITICAL_SECTION de 24 octets. */
    fill(&g);
    InitializeCriticalSection(&g);
    guard_at(14, &g, 6);
    EnterCriticalSection(&g);
    EnterCriticalSection(&g);
    guard_at(15, &g, 6);
    if (g.body[2] != 2 || g.body[3] == 0) fail(16, g.body[2]); /* RecursionCount, OwningThread */
    LeaveCriticalSection(&g);
    LeaveCriticalSection(&g);
    DeleteCriticalSection(&g);
    guard_at(17, &g, 6);
    fill(&g);
    if (!InitializeCriticalSectionAndSpinCount(&g, 4000)) fail(18, 0);
    guard_at(18, &g, 6);
    if (g.body[5] != 4000) fail(18, g.body[5]);
    DeleteCriticalSection(&g);

    /* 19-20 : SRWLOCK et INIT_ONCE de 4 octets. */
    fill(&g);
    InitializeSRWLock(&g);
    guard_at(19, &g, 1);
    AcquireSRWLockExclusive(&g);
    ReleaseSRWLockExclusive(&g);
    guard_at(19, &g, 1);
    fill(&g);
    InitOnceInitialize(&g);
    guard_at(20, &g, 1);
    {
        BOOL pending = 0;
        void *ctx = (void *)GUARD;
        if (!InitOnceBeginInitialize(&g, 0, &pending, &ctx) || !pending) fail(21, pending);
        if (!InitOnceComplete(&g, 0, (void *)0x1234560)) fail(21, 1);
        guard_at(21, &g, 1);
        if (!InitOnceBeginInitialize(&g, 0, &pending, &ctx) || pending || ctx != (void *)0x1234560)
            fail(22, (DWORD)ctx);
    }

    /* 23-25 : SLIST_HEADER de 8 octets, entrées alignées sur 8. */
    {
        __attribute__((aligned(16))) DWORD list[4] = {GUARD, GUARD, GUARD, GUARD};
        __attribute__((aligned(16))) DWORD a[2] = {0, 0}, b[2] = {0, 0};
        InitializeSListHead(list);
        if (list[2] != GUARD || list[3] != GUARD) fail(23, list[2]);
        InterlockedPushEntrySList(list, a);
        InterlockedPushEntrySList(list, b);
        if (list[0] != (DWORD)b || QueryDepthSList(list) != 2 || a[1] != 0 || b[1] != 0)
            fail(24, list[0]);
        if (InterlockedPopEntrySList(list) != b || InterlockedPopEntrySList(list) != a)
            fail(25, list[0]);
        if (list[2] != GUARD) fail(25, list[2]);
    }

    /* 26-27 : handles rendus par pointeur, 4 octets chacun. */
    {
        HANDLE pipe[4] = {(HANDLE)GUARD, (HANDLE)GUARD, (HANDLE)GUARD, (HANDLE)GUARD};
        if (!CreatePipe(&pipe[0], &pipe[2], 0, 0)) fail(26, 0);
        if (pipe[1] != (HANDLE)GUARD || pipe[3] != (HANDLE)GUARD) fail(27, (DWORD)pipe[1]);
        CloseHandle(pipe[0]);
        CloseHandle(pipe[2]);
    }

    /* 28-33 : registre, HKLM\Software sous Wow6432Node. */
    {
        HKEY key[2] = {0, (HKEY)GUARD};
        DWORD disp = 0;
        if (RegCreateKeyExA(HKLM, "Software\\WinluxWow32Test", 0, 0, 0, KEY_ALL_ACCESS, 0, &key[0],
                            &disp))
            fail(28, 0);
        if (key[1] != (HKEY)GUARD) fail(29, (DWORD)key[1]);
        RegSetValueExA(key[0], "Vue", 0, 4, &disp, 4);
        RegCloseKey(key[0]);
        if (RegOpenKeyExA(HKLM, "Software\\Wow6432Node\\WinluxWow32Test", 0, KEY_READ, &key[0]))
            fail(30, 0);
        RegCloseKey(key[0]);
        if (!RegOpenKeyExA(HKLM, "Software\\WinluxWow32Test", 0, KEY_READ | KEY_WOW64_64KEY,
                           &key[0]))
            fail(31, 0);
        if (RegOpenKeyExA(HKLM, "Software\\WinluxWow32Test", 0, KEY_READ, &key[0])) fail(32, 0);
        RegCloseKey(key[0]);
        if (RegDeleteKeyA(HKLM, "Software\\WinluxWow32Test")) fail(33, 0);
    }

    /* 34-37 : variables d'environnement d'un processus 32 bits. */
    check_env(34, "PROCESSOR_ARCHITECTURE", "x86");
    check_env(35, "PROCESSOR_ARCHITEW6432", "AMD64");
    check_env(36, "ProgramFiles", "C:\\Program Files (x86)");
    check_env(37, "ProgramW6432", "C:\\Program Files");

    /* 38-41 : fichiers, System32 → SysWOW64. */
    {
        char dir[64];
        void *old = (void *)GUARD;
        if (GetSystemWow64DirectoryA(dir, sizeof dir) != 19 || !same(dir, "C:\\Windows\\SysWOW64"))
            fail(38, 0);
        touch("C:\\Windows\\System32\\winlux-wow64-redirige.txt");
        if (!Wow64DisableWow64FsRedirection(&old)) fail(39, 0);
        touch("C:\\Windows\\System32\\winlux-wow64-direct.txt");
        if (!Wow64RevertWow64FsRedirection(old)) fail(41, 0);
        touch("C:\\Windows\\Sysnative\\winlux-wow64-sysnative.txt");
    }

    /* 42-45 : CreateProcessA, PROCESS_INFORMATION de 16 octets. */
    {
        char self[260], line[300];
        DWORD si[17 + 1], pi[5], code = 0;
        int n = (int)GetModuleFileNameA(0, self, sizeof self), i = 0, j = 0;
        if (n <= 0) fail(42, 0);
        line[j++] = '"';
        for (i = 0; i < n; i++) line[j++] = self[i];
        line[j++] = '"';
        for (const char *t = " enfant"; *t; t++) line[j++] = *t;
        line[j] = 0;
        for (i = 0; i < 18; i++) si[i] = 0;
        si[0] = 68;
        si[17] = GUARD;
        for (i = 0; i < 5; i++) pi[i] = GUARD;
        if (!CreateProcessA(0, line, 0, 0, 0, 0, 0, 0, si, pi)) fail(43, 0);
        if (pi[4] != GUARD || si[17] != GUARD || pi[2] == 0) fail(44, pi[4]);
        WaitForSingleObject((HANDLE)pi[0], 0xFFFFFFFF);
        if (!GetExitCodeProcess((HANDLE)pi[0], &code) || code != 42) fail(45, code);
        CloseHandle((HANDLE)pi[0]);
        CloseHandle((HANDLE)pi[1]);
    }

    /* 46-49 : Interlocked* (exportées par le kernel32 x86 seulement). */
    {
        volatile long v = 5;
        if (InterlockedIncrement(&v) != 6 || v != 6) fail(46, v);
        if (InterlockedDecrement(&v) != 5) fail(47, v);
        if (InterlockedExchange(&v, 9) != 5 || v != 9) fail(48, v);
        if (InterlockedCompareExchange(&v, 1, 9) != 9 || v != 1) fail(49, v);
    }

    /* 50-52 : atomes globaux, SetHandleCount. */
    {
        WORD a = GlobalAddAtomA("WinluxAtome32");
        if (a < 0xC000 || GlobalFindAtomA("winluxatome32") != a) fail(50, a);
        if (GlobalDeleteAtom(a) != 0 || GlobalFindAtomA("WinluxAtome32") != 0) fail(51, a);
        if (SetHandleCount(64) != 64) fail(52, 0);
    }

    /* 53-55 : msvcrt x86 : _ftol (valeur dans ST0, edx:eax), _adjust_fdiv, _controlfp. */
    {
        double d = -1234567.75;
        long long q;
        __asm__ volatile("fldl %1\n\tcall *%2" : "=A"(q) : "m"(d), "r"(&_ftol) : "memory", "ecx", "st");
        if (q != -1234567) fail(53, (DWORD)q);
        if (_adjust_fdiv != 0) fail(54, _adjust_fdiv);
        if ((_controlfp(0, 0) & 0x1F) != 0x1F) fail(55, _controlfp(0, 0));
    }

    /* 56-58 : DLL système importées par WinSCP et 7-Zip. */
    {
        HANDLE mpr = LoadLibraryA("mpr.dll"), msi = LoadLibraryA("msi.dll");
        HANDLE shf = LoadLibraryA("shfolder.dll");
        if (!mpr || !GetProcAddress(mpr, "WNetGetConnectionW")) fail(56, 0);
        if (!msi || !GetProcAddress(msi, (const char *)205)) fail(57, 0);
        if (!shf || !GetProcAddress(shf, "SHGetFolderPathW")) fail(58, 0);
    }

    say("Tout est correct : personnalite WOW64\r\n");
    ExitProcess(0);
}

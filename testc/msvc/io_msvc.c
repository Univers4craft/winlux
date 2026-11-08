// Annulation des entrées-sorties et handles ou tampons invalides (#18), au
// format MSVC (clang x86_64-pc-windows-msvc, sans CRT).
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned char u8;
typedef unsigned short u16;
typedef struct {
    u64 internal, internal_high;
    u32 offset, offset_high;
    void *event;
} OVERLAPPED;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall CreateNamedPipeW(const u16 *, u32, u32, u32, u32, u32, u32, void *);
__declspec(dllimport) void *__stdcall CreateFileW(const u16 *, u32, u32, void *, u32, u32, void *);
__declspec(dllimport) int __stdcall ReadFile(void *, void *, u32, u32 *, OVERLAPPED *);
__declspec(dllimport) int __stdcall WriteFile(void *, const void *, u32, u32 *, OVERLAPPED *);
__declspec(dllimport) int __stdcall GetOverlappedResult(void *, OVERLAPPED *, u32 *, int);
__declspec(dllimport) int __stdcall CancelIo(void *);
__declspec(dllimport) int __stdcall CancelIoEx(void *, OVERLAPPED *);
__declspec(dllimport) int __stdcall CancelSynchronousIo(void *);
__declspec(dllimport) void *__stdcall GetCurrentThread(void);
__declspec(dllimport) void *__stdcall CreateEventW(void *, int, int, const u16 *);
__declspec(dllimport) u32 __stdcall WaitForSingleObject(void *, u32);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) u32 __stdcall GetLastError(void);
__declspec(dllimport) void *__stdcall VirtualAlloc(void *, u64, u32, u32);
__declspec(dllimport) int __stdcall DeleteFileW(const u16 *);

void *memset(void *d, int c, u64 n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}

#define INVALID ((void *)~0ull)
#define BOGUS ((void *)0x7A5C)
#define CHECK(n, c) do { if (!(c)) ExitProcess(n); } while (0)
static const u16 pipe_name[] = {'\\', '\\', '.', '\\', 'p', 'i', 'p', 'e', '\\', 'w', 'i', 'n', 'l',
                                'u', 'x', '-', 'i', 'o', '-', '1', '8', 0};
static const u16 file_name[] = {'i', 'o', '-', '1', '8', '.', 'b', 'i', 'n', 0};
static u8 buf[256];

int entry() {
    OVERLAPPED ov;
    u32 n = 0;
    void *ev = CreateEventW(0, 1, 0, 0);
    // Tube nommé en mode recouvert : une lecture reste en attente.
    void *server = CreateNamedPipeW(pipe_name, 3 | 0x40000000, 0, 1, 4096, 4096, 0, 0);
    CHECK(1, server != INVALID);
    void *client = CreateFileW(pipe_name, 0xC0000000, 0, 0, 3, 0x40000000, 0);
    CHECK(2, client != INVALID);

    // 3-6. Lecture commencée puis annulée par CancelIoEx : ERROR_OPERATION_ABORTED,
    // événement signalé ; une seconde annulation ne trouve plus rien.
    memset(&ov, 0, sizeof ov);
    ov.event = ev;
    CHECK(3, ReadFile(server, buf, 64, 0, &ov) == 0 && GetLastError() == 997);
    CHECK(4, CancelIoEx(server, &ov) == 1);
    CHECK(5, GetOverlappedResult(server, &ov, &n, 1) == 0 && GetLastError() == 995 &&
                 WaitForSingleObject(ev, 1000) == 0);
    CHECK(6, CancelIoEx(server, &ov) == 0 && GetLastError() == 1168);

    // 7. CancelIo annule les lectures commencées par ce thread.
    memset(&ov, 0, sizeof ov);
    CHECK(7, ReadFile(server, buf, 64, 0, &ov) == 0 && GetLastError() == 997 &&
                 CancelIo(server) == 1 && GetOverlappedResult(server, &ov, &n, 1) == 0 &&
                 GetLastError() == 995);

    // 8. Le tube reste utilisable après l'annulation.
    memset(&ov, 0, sizeof ov);
    CHECK(8, ReadFile(server, buf, 64, 0, &ov) == 0 && GetLastError() == 997);
    {
        OVERLAPPED w;
        memset(&w, 0, sizeof w);
        u32 written = 0;
        int ok = WriteFile(client, "abc", 3, &written, &w);
        CHECK(9, ok || GetLastError() == 997);
        CHECK(10, GetOverlappedResult(client, &w, &written, 1) == 1 && written == 3);
    }
    CHECK(11, GetOverlappedResult(server, &ov, &n, 1) == 1 && n == 3 && buf[0] == 'a');

    // 12. CancelIoEx sans rien en attente, sur un autre OVERLAPPED.
    {
        OVERLAPPED other;
        memset(&other, 0, sizeof other);
        CHECK(12, CancelIoEx(server, &other) == 0 && GetLastError() == 1168);
    }
    // 13. CancelSynchronousIo : aucune E/S synchrone en cours sur ce thread.
    CHECK(13, CancelSynchronousIo(GetCurrentThread()) == 0 && GetLastError() == 1168);

    // 14-19. Handles invalides : ERROR_INVALID_HANDLE, sans plantage.
    CHECK(14, ReadFile(BOGUS, buf, 4, &n, 0) == 0 && GetLastError() == 6);
    CHECK(15, WriteFile(BOGUS, buf, 4, &n, 0) == 0 && GetLastError() == 6);
    CHECK(16, CancelIo(BOGUS) == 0 && GetLastError() == 6);
    CHECK(17, CancelIoEx(BOGUS, 0) == 0 && GetLastError() == 6);
    // GetOverlappedResult sur une opération terminée ne lit que l'OVERLAPPED,
    // comme Windows : le handle n'est pas consulté.
    CHECK(18, GetOverlappedResult(BOGUS, &ov, &n, 0) == 1 && n == 3);
    CHECK(19, CloseHandle(BOGUS) == 0 && GetLastError() == 6);

    // 20-24. Tampons invalides sur un vrai fichier : ERROR_NOACCESS, sans plantage.
    void *f = CreateFileW(file_name, 0xC0000000, 0, 0, 2, 0, 0);
    CHECK(20, f != INVALID);
    CHECK(21, WriteFile(f, "0123456789", 10, &n, 0) == 1 && n == 10);
    CHECK(22, WriteFile(f, 0, 10, &n, 0) == 0 && GetLastError() == 998);
    CHECK(23, WriteFile(f, 0, 0, &n, 0) == 1 && n == 0);
    {
        // Lecture vers une page en lecture seule, puis vers NULL.
        u8 *ro = VirtualAlloc(0, 4096, 0x3000, 0x02);
        u32 pos = 0;
        CHECK(24, ro != 0);
        OVERLAPPED at;
        memset(&at, 0, sizeof at);
        at.offset = pos;
        CHECK(25, ReadFile(f, ro, 10, &n, &at) == 0 && GetLastError() == 998);
        memset(&at, 0, sizeof at);
        CHECK(26, ReadFile(f, 0, 10, &n, &at) == 0 && GetLastError() == 998);
    }
    CHECK(27, CloseHandle(f) == 1 && DeleteFileW(file_name) == 1);
    CHECK(28, CloseHandle(client) == 1 && CloseHandle(server) == 1 && CloseHandle(ev) == 1);
    ExitProcess(0);
    return 0;
}

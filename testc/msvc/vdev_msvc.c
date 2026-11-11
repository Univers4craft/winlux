// Appareil virtuel d'un pilote constructeur (#1533), au format MSVC (clang
// en mode x86_64-pc-windows-msvc, sans CRT) : le logiciel ouvre son pilote
// par son nom puis lui parle avec DeviceIoControl, comme iCUE ou GG. Lancé
// par scripts/test-vdev.sh avec WINLUX_VDEV_ESSAI=1 (appareil « WinluxEssai »).
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned char u8;
typedef unsigned short u16;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall CreateFileW(const u16 *, u32, u32, void *, u32, u32, void *);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) u32 __stdcall GetLastError(void);
__declspec(dllimport) int __stdcall DeviceIoControl(void *, u32, void *, u32, void *, u32, u32 *, void *);
__declspec(dllimport) void *__stdcall CreateEventW(void *, int, int, const u16 *);
__declspec(dllimport) u32 __stdcall WaitForSingleObject(void *, u32);
__declspec(dllimport) int __stdcall GetOverlappedResult(void *, void *, u32 *, int);
__declspec(dllimport) void *__stdcall CreateIoCompletionPort(void *, void *, u64, u32);
__declspec(dllimport) int __stdcall GetQueuedCompletionStatus(void *, u32 *, u64 *, void **, u32);

typedef struct { u64 internal, internal_high; u32 offset, offset_high; void *event; } Overlapped;

#define CODE(f) ((0x8000u << 16) | ((f) << 2))
#define INVALID ((void *)~0ull)

void *memset(void *d, int c, u64 n) {
    volatile u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}

static u16 essai[] = L"\\\\.\\WinluxEssai";
static u16 global[] = L"\\\\.\\Global\\winluxessai";
static u16 inconnu[] = L"\\\\.\\WinluxInconnu";

static u32 le32(const u8 *b) { return b[0] | b[1] << 8 | b[2] << 16 | (u32)b[3] << 24; }

void entry(void) {
    u8 in[16], out[64];
    u32 n = 0;

    // 1. Nom inconnu : ERROR_FILE_NOT_FOUND, comme un pilote absent.
    if (CreateFileW(inconnu, 0xC0000000, 3, 0, 3, 0, 0) != INVALID || GetLastError() != 2) ExitProcess(1);

    // 2. Ouverture par \\.\Nom et \\.\Global\nom (casse indifférente).
    void *h = CreateFileW(essai, 0xC0000000, 3, 0, 3, 0, 0);
    if (h == INVALID) ExitProcess(2);
    void *g = CreateFileW(global, 0, 3, 0, 3, 0, 0);
    if (g == INVALID) ExitProcess(2);
    CloseHandle(g);

    // 3. Version : 8 octets, version 1.0 et taille d'entrée reçue.
    for (int i = 0; i < 16; i++) in[i] = (u8)(i + 1);
    if (!DeviceIoControl(h, CODE(0x800), in, 5, out, sizeof out, &n, 0) || n != 8 ||
        le32(out) != 0x10000 || le32(out + 4) != 5)
        ExitProcess(3);

    // 4. Écho inversé, puis tampon de sortie trop petit.
    if (!DeviceIoControl(h, CODE(0x801), in, 4, out, sizeof out, &n, 0) || n != 4 || out[0] != 4 || out[3] != 1)
        ExitProcess(4);
    if (DeviceIoControl(h, CODE(0x801), in, 8, out, 4, &n, 0) || GetLastError() != 122) ExitProcess(4);

    // 5. Code inconnu : ERROR_INVALID_FUNCTION ; code refusé : son erreur.
    if (DeviceIoControl(h, CODE(0xBFF), in, 6, out, sizeof out, &n, 0) || GetLastError() != 1) ExitProcess(5);
    if (DeviceIoControl(h, CODE(0x804), 0, 0, 0, 0, &n, 0) || GetLastError() != 5) ExitProcess(5);

    // 6. Réponse partielle : ERROR_MORE_DATA et l'en-tête (taille totale).
    if (DeviceIoControl(h, CODE(0x803), 0, 0, out, 4, &n, 0) || GetLastError() != 234 || n != 4 ||
        le32(out) != 64)
        ExitProcess(6);
    CloseHandle(h);

    // 7. OVERLAPPED avec événement : fini au retour, résultat lisible.
    h = CreateFileW(essai, 0xC0000000, 3, 0, 3, 0x40000000, 0);
    if (h == INVALID) ExitProcess(7);
    Overlapped ov;
    memset(&ov, 0, sizeof ov);
    ov.event = CreateEventW(0, 1, 0, 0);
    if (!DeviceIoControl(h, CODE(0x801), in, 3, out, sizeof out, 0, &ov)) ExitProcess(7);
    if (WaitForSingleObject(ov.event, 1000) != 0 || ov.internal != 0 || ov.internal_high != 3) ExitProcess(7);
    if (!GetOverlappedResult(h, &ov, &n, 0) || n != 3 || out[0] != 3) ExitProcess(7);

    // 8. Port de complétion : un paquet par requête terminée.
    void *port = CreateIoCompletionPort(h, 0, 0x1533, 0);
    if (!port) ExitProcess(8);
    memset(&ov, 0, sizeof ov);
    if (!DeviceIoControl(h, CODE(0x800), in, 2, out, sizeof out, 0, &ov)) ExitProcess(8);
    u64 key = 0;
    void *done = 0;
    if (!GetQueuedCompletionStatus(port, &n, &key, &done, 1000) || key != 0x1533 || done != &ov || n != 8)
        ExitProcess(8);
    CloseHandle(h);

    // 9. Ouvertures répétées : chaque handle garde son appareil, le
    //    compteur de l'appareil avance (même module derrière tous).
    u32 last = 0;
    for (int i = 0; i < 500; i++) {
        h = CreateFileW(essai, 0xC0000000, 3, 0, 3, 0, 0);
        if (h == INVALID) ExitProcess(9);
        if (!DeviceIoControl(h, CODE(0x802), 0, 0, out, 4, &n, 0) || n != 4 || le32(out) <= last) ExitProcess(9);
        last = le32(out);
        CloseHandle(h);
    }
    ExitProcess(0);
}

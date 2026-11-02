// Ports série, suite (#271), au format MSVC (clang en mode
// x86_64-pc-windows-msvc, sans CRT) : trouver le port puis lui parler, comme
// l'IDE Arduino, Cura ou un logiciel radio.
//   QueryDosDeviceW, HKLM\HARDWARE\DEVICEMAP\SERIALCOMM, BuildCommDCBW,
//   SetCommState, SetCommMask + WaitCommEvent, TransmitCommChar,
//   GetCommProperties.
// Le port COM7 est un pseudo-terminal (scripts/test-serial.sh) dont l'autre
// bout renvoie en majuscules ce qu'il reçoit.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall CreateFileW(const u16 *, u32, u32, void *, u32, u32, void *);
__declspec(dllimport) int __stdcall ReadFile(void *, void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall WriteFile(void *, const void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) u32 __stdcall QueryDosDeviceW(const u16 *, u16 *, u32);
__declspec(dllimport) int __stdcall BuildCommDCBW(const u16 *, void *);
__declspec(dllimport) int __stdcall GetCommState(void *, void *);
__declspec(dllimport) int __stdcall SetCommState(void *, void *);
__declspec(dllimport) int __stdcall SetCommTimeouts(void *, void *);
__declspec(dllimport) int __stdcall SetCommMask(void *, u32);
__declspec(dllimport) int __stdcall WaitCommEvent(void *, u32 *, void *);
__declspec(dllimport) int __stdcall TransmitCommChar(void *, char);
__declspec(dllimport) int __stdcall GetCommProperties(void *, void *);
__declspec(dllimport) long __stdcall RegOpenKeyExW(void *, const u16 *, u32, u32, void **);
__declspec(dllimport) long __stdcall RegQueryValueExW(void *, const u16 *, u32 *, u32 *, void *, u32 *);

void *memset(void *d, int c, u64 n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}

static int weq(const u16 *a, const char *b) {
    while (*b)
        if (*a++ != (u16)*b++) return 0;
    return *a == 0;
}
static int wstarts(const u16 *a, const char *b) {
    while (*b)
        if (*a++ != (u16)*b++) return 0;
    return 1;
}

static void *INVALID = (void *)~0ull;
static u16 list[4096];
static const u16 com7[] = {'C', 'O', 'M', '7', 0};
static const u16 path[] = {'\\', '\\', '.', '\\', 'C', 'O', 'M', '7', 0};
static const u16 key[] = {'H', 'A', 'R', 'D', 'W', 'A', 'R', 'E', '\\', 'D', 'E', 'V', 'I', 'C', 'E', 'M', 'A', 'P',
                          '\\', 'S', 'E', 'R', 'I', 'A', 'L', 'C', 'O', 'M', 'M', 0};
static const u16 serial6[] = {'\\', 'D', 'e', 'v', 'i', 'c', 'e', '\\', 'S', 'e', 'r', 'i', 'a', 'l', '6', 0};
static const u16 def[] = {'b', 'a', 'u', 'd', '=', '1', '1', '5', '2', '0', '0', ' ', 'p', 'a', 'r', 'i', 't',
                          'y', '=', 'N', ' ', 'd', 'a', 't', 'a', '=', '8', ' ', 's', 't', 'o', 'p', '=', '1', 0};

static int read_exact(void *h, u8 *buf, u32 n) {
    u32 got = 0, r = 0;
    for (int tries = 0; got < n && tries < 100; tries++) {
        if (!ReadFile(h, buf + got, n - got, &r, 0)) return 0;
        got += r;
    }
    return got == n;
}

int entry() {
    // 1. QueryDosDeviceW(NULL) liste COM7 parmi les noms DOS.
    u32 n = QueryDosDeviceW(0, list, 4096);
    if (n < 4) ExitProcess(1);
    int found = 0;
    for (u16 *p = list; *p; p++) {
        if (weq(p, "COM7")) found = 1;
        while (*p) p++;
    }
    if (!found) ExitProcess(1);
    // 2. Sa cible NT.
    if (!QueryDosDeviceW(com7, list, 4096) || !wstarts(list, "\\Device\\Serial6")) ExitProcess(2);
    // 3. Le registre SERIALCOMM, que lisent la plupart des logiciels.
    void *hk = 0;
    if (RegOpenKeyExW((void *)0xFFFFFFFF80000002ull, key, 0, 0x20019, &hk) != 0) ExitProcess(3);
    u32 type = 0, size = sizeof list;
    if (RegQueryValueExW(hk, serial6, 0, &type, list, &size) != 0 || type != 1 || !weq(list, "COM7")) ExitProcess(3);
    // 4. Ouverture du port et réglage par texte.
    void *h = CreateFileW(path, 0xC0000000, 0, 0, 3, 0, 0);
    if (h == INVALID) ExitProcess(4);
    u8 dcb[28];
    memset(dcb, 0, sizeof dcb);
    *(u32 *)dcb = 28;
    if (!GetCommState(h, dcb) || !BuildCommDCBW(def, dcb) || *(u32 *)(dcb + 4) != 115200) ExitProcess(4);
    if (!SetCommState(h, dcb)) ExitProcess(4);
    u32 timeouts[5] = {0, 0, 2000, 0, 0};
    SetCommTimeouts(h, timeouts);
    // 5. Propriétés : COMMPROP de 64 octets, RS-232.
    u8 prop[64];
    if (!GetCommProperties(h, prop) || *(u16 *)prop != 64 || *(u32 *)(prop + 24) != 1) ExitProcess(5);
    // 6. SetCommMask(EV_RXCHAR), envoi, WaitCommEvent, réponse en majuscules.
    if (!SetCommMask(h, 1)) ExitProcess(6);
    u32 done = 0;
    if (!WriteFile(h, "ping\n", 5, &done, 0) || done != 5) ExitProcess(6);
    u32 ev = 0;
    if (!WaitCommEvent(h, &ev, 0) || ev != 1) ExitProcess(6);
    u8 in[8];
    if (!read_exact(h, in, 5) || in[0] != 'P' || in[3] != 'G' || in[4] != '\n') ExitProcess(6);
    // 7. TransmitCommChar : un octet, renvoyé en majuscule.
    if (!TransmitCommChar(h, 'x') || !read_exact(h, in, 1) || in[0] != 'X') ExitProcess(7);
    CloseHandle(h);
    ExitProcess(0);
    return 0;
}

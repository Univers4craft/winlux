// DeviceIoControl sur volumes, disques et fichiers (#271), au format MSVC
// (clang en mode x86_64-pc-windows-msvc, sans CRT) : ce que demandent les
// lanceurs de jeux, outils de licence et de sauvegarde.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef long long i64;
typedef unsigned int u32;
typedef unsigned char u8;
typedef unsigned short u16;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall CreateFileW(const u16 *, u32, u32, void *, u32, u32, void *);
__declspec(dllimport) int __stdcall DeviceIoControl(void *, u32, void *, u32, void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall WriteFile(void *, const void *, u32, u32 *, void *);
__declspec(dllimport) int __stdcall ReadFile(void *, void *, u32, u32 *, void *);
__declspec(dllimport) u32 __stdcall SetFilePointer(void *, int, int *, u32);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) u32 __stdcall GetLastError(void);

void *memset(void *d, int c, u64 n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}

static void *INVALID = (void *)~0ull;
static u8 buf[8192];
static const u16 volume[] = {'\\', '\\', '.', '\\', 'C', ':', 0};
static const u16 drive0[] = {'\\', '\\', '.', '\\', 'P', 'h', 'y', 's', 'i', 'c', 'a', 'l',
                             'D', 'r', 'i', 'v', 'e', '0', 0};
static const u16 sparse[] = {'c', 'r', 'e', 'u', 'x', '.', 'b', 'i', 'n', 0};

static u32 at32(int off) { return *(u32 *)(buf + off); }
static i64 at64(int off) { return *(i64 *)(buf + off); }

int entry() {
    u32 n = 0;
    // 1. Le volume C: s'ouvre sans droits d'accès (comme les outils de disque).
    void *v = CreateFileW(volume, 0, 3, 0, 3, 0, 0);
    if (v == INVALID) ExitProcess(1);
    // 2. Taille du volume.
    if (!DeviceIoControl(v, 0x7405C, 0, 0, buf, 8, &n, 0) || n != 8 || at64(0) <= 0) ExitProcess(2);
    // 3. Géométrie : secteurs de 512 ou 4096 octets, disque fixe ou amovible.
    if (!DeviceIoControl(v, 0x70000, 0, 0, buf, 24, &n, 0) || n != 24) ExitProcess(3);
    if ((at32(20) != 512 && at32(20) != 4096) || (at32(8) != 11 && at32(8) != 12)) ExitProcess(3);
    // 4. Fiche du disque en deux appels : en-tête de 8 octets, puis le tout.
    u32 query[3] = {0, 0, 0}; // StorageDeviceProperty, PropertyStandardQuery
    if (!DeviceIoControl(v, 0x2D1400, query, 12, buf, 8, &n, 0) || n != 8 || at32(4) < 40) ExitProcess(4);
    u32 size = at32(4);
    memset(buf, 0, sizeof buf);
    if (!DeviceIoControl(v, 0x2D1400, query, 12, buf, sizeof buf, &n, 0) || n != size || at32(0) != 40)
        ExitProcess(4);
    if (at32(28) == 0 || at32(28) > 0x14) ExitProcess(4); // BusType connu
    // 5. SSD ou disque à plateaux (DEVICE_SEEK_PENALTY_DESCRIPTOR).
    query[0] = 7;
    if (!DeviceIoControl(v, 0x2D1400, query, 12, buf, 12, &n, 0) || n != 12 || buf[8] > 1) ExitProcess(5);
    // 6. Numéro de disque : FILE_DEVICE_DISK.
    if (!DeviceIoControl(v, 0x2D1080, 0, 0, buf, 12, &n, 0) || n != 12 || at32(0) != 7) ExitProcess(6);
    // 7. Verrouiller un volume monté : refusé (accès refusé).
    if (DeviceIoControl(v, 0x90018, 0, 0, 0, 0, &n, 0) || GetLastError() != 5) ExitProcess(7);
    // 8. Code inconnu : ERROR_NOT_SUPPORTED, sans plantage.
    if (DeviceIoControl(v, 0x00123457, 0, 0, buf, 16, &n, 0) || GetLastError() != 50) ExitProcess(8);
    // 9. Tampon trop petit : ERROR_INSUFFICIENT_BUFFER.
    if (DeviceIoControl(v, 0x70000, 0, 0, buf, 8, &n, 0) || GetLastError() != 122) ExitProcess(9);
    CloseHandle(v);

    // 10. Fichier creux : FSCTL_SET_SPARSE, deux blocs écrits à 8 Mo d'écart,
    //     FSCTL_QUERY_ALLOCATED_RANGES les retrouve.
    void *f = CreateFileW(sparse, 0xC0000000, 0, 0, 2, 0, 0);
    if (f == INVALID) ExitProcess(10);
    if (!DeviceIoControl(f, 0x900C4, 0, 0, 0, 0, &n, 0)) ExitProcess(10);
    memset(buf, 1, 4096);
    if (!WriteFile(f, buf, 4096, &n, 0) || n != 4096) ExitProcess(10);
    SetFilePointer(f, 8 << 20, 0, 0);
    if (!WriteFile(f, buf, 4096, &n, 0) || n != 4096) ExitProcess(10);
    i64 range[2] = {0, 16 << 20};
    memset(buf, 0, sizeof buf);
    if (!DeviceIoControl(f, 0x940CF, range, 16, buf, sizeof buf, &n, 0) || n < 16 || n % 16) ExitProcess(10);
    if (at64(0) != 0 || at64(n - 16) + at64(n - 8) != (8 << 20) + 4096) ExitProcess(10);
    // 11. FSCTL_SET_ZERO_DATA remet le premier bloc à zéro.
    i64 zero[2] = {0, 4096};
    if (!DeviceIoControl(f, 0x980C8, zero, 16, 0, 0, &n, 0)) ExitProcess(11);
    SetFilePointer(f, 0, 0, 0);
    memset(buf, 9, 4096);
    if (!ReadFile(f, buf, 4096, &n, 0) || n != 4096) ExitProcess(11);
    for (int i = 0; i < 4096; i++)
        if (buf[i]) ExitProcess(11);
    // 12. Compression NTFS : aucune.
    if (!DeviceIoControl(f, 0x9003C, 0, 0, buf, 2, &n, 0) || n != 2 || *(u16 *)buf != 0) ExitProcess(12);
    CloseHandle(f);

    // 13. PhysicalDrive0, quand la machine a un disque : sa taille.
    void *d = CreateFileW(drive0, 0, 3, 0, 3, 0, 0);
    if (d != INVALID) {
        if (!DeviceIoControl(d, 0x7405C, 0, 0, buf, 8, &n, 0) || at64(0) <= 0) ExitProcess(13);
        CloseHandle(d);
    }
    ExitProcess(0);
    return 0;
}

// Définition d'écran et rampe gamma des jeux (#271), au format MSVC (clang en
// mode x86_64-pc-windows-msvc, sans CRT) : EnumDisplaySettingsW,
// ChangeDisplaySettingsExW (CDS_TEST, mode refusé, vrai changement),
// GetDeviceGammaRamp. xrandr est remplacé par un faux (scripts/test-display-mode.sh)
// qui note ses appels ; le script vérifie ensuite que l'écran est remis
// comme avant après la fin du programme.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) int __stdcall EnumDisplaySettingsW(const u16 *, u32, void *);
__declspec(dllimport) int __stdcall ChangeDisplaySettingsExW(const u16 *, void *, void *, u32, void *);
__declspec(dllimport) int __stdcall GetDeviceGammaRamp(void *, u16 *);

void *memset(void *d, int c, u64 n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}
u64 wcslen(const u16 *s) {
    u64 n = 0;
    while (s[n]) n++;
    return n;
}
void *memcpy(void *d, const void *s, u64 n) {
    u8 *o = d;
    const u8 *i = s;
    while (n--) *o++ = *i++;
    return d;
}

static u8 dm[220];
static u16 ramp[768];
#define FIELDS(d) (*(u32 *)((d) + 72))
#define BPP(d) (*(u32 *)((d) + 168))
#define WIDTH(d) (*(u32 *)((d) + 172))
#define HEIGHT(d) (*(u32 *)((d) + 176))
#define HZ(d) (*(u32 *)((d) + 184))

static void reset(void) {
    memset(dm, 0, sizeof dm);
    *(u16 *)(dm + 68) = sizeof dm; // dmSize
}

int entry() {
    // 1. Mode actuel : 2560x1440 à 144 Hz, 32 bits.
    reset();
    if (!EnumDisplaySettingsW(0, 0xFFFFFFFF, dm)) ExitProcess(1);
    if (WIDTH(dm) != 2560 || HEIGHT(dm) != 1440 || HZ(dm) != 144 || BPP(dm) != 32) ExitProcess(1);
    // 2. Liste : 6 couples définition/fréquence, en 32 et 16 bits.
    int count = 0, found = 0;
    for (u32 i = 0; i < 100; i++) {
        reset();
        if (!EnumDisplaySettingsW(0, i, dm)) break;
        count++;
        if (WIDTH(dm) == 1280 && HEIGHT(dm) == 720 && HZ(dm) == 60 && BPP(dm) == 32) found = 1;
    }
    if (count != 12 || !found) ExitProcess(2);
    // 3. CDS_TEST d'un mode existant : accepté, rien n'est changé.
    reset();
    FIELDS(dm) = 0x80000 | 0x100000;
    WIDTH(dm) = 1280;
    HEIGHT(dm) = 720;
    if (ChangeDisplaySettingsExW(0, dm, 0, 2, 0) != 0) ExitProcess(3);
    // 4. Mode inexistant : DISP_CHANGE_BADMODE.
    WIDTH(dm) = 1234;
    HEIGHT(dm) = 567;
    if (ChangeDisplaySettingsExW(0, dm, 0, 4, 0) != -2) ExitProcess(4);
    // 5. Plein écran en 1280x720 à 60 Hz.
    reset();
    FIELDS(dm) = 0x80000 | 0x100000 | 0x400000;
    WIDTH(dm) = 1280;
    HEIGHT(dm) = 720;
    HZ(dm) = 60;
    if (ChangeDisplaySettingsExW(0, dm, 0, 4, 0) != 0) ExitProcess(5);
    // 6. Rampe gamma lisible (neutre sans RandR).
    if (!GetDeviceGammaRamp(0, ramp) || ramp[255] < ramp[0]) ExitProcess(6);
    // Fin sans rien remettre : le gardien doit s'en charger.
    ExitProcess(0);
    return 0;
}

/* Anciens jeux sous Winlux (#237, #777) : DirectDraw, MIDI et minuteurs
 * multimédia, construit deux fois par clang sans CRT : en x86
 * (i686-pc-windows-msvc, jeux32.exe) et en x64 (jeux64.exe), pour vérifier
 * les deux dispositions des structures (DDSURFACEDESC, DDBLTFX, MIDIHDR,
 * WAVEHDR) et les rappels x86.
 * Direct3D 8 (vérifications 80 à 105) passe par la traduction vers
 * Direct3D 9.
 * Rend 0 et écrit « Jeux anciens : tout est correct » si tout va bien,
 * sinon le numéro de la vérification qui a échoué. */
typedef unsigned int DWORD, UINT;
typedef int BOOL, LONG, HRESULT;
typedef unsigned short WORD;
typedef unsigned char BYTE;
typedef void *HANDLE, *HWND, *HDC, *HINSTANCE, *HBRUSH, *HGDIOBJ;
#ifdef _WIN64
typedef unsigned long long UINT_PTR, DWORD_PTR;
typedef long long LONG_PTR;
#else
typedef unsigned int UINT_PTR, DWORD_PTR;
typedef long LONG_PTR;
#endif
typedef UINT_PTR WPARAM;
typedef LONG_PTR LPARAM, LRESULT;

#define WINAPI __stdcall
#define CALLBACK __stdcall
typedef LRESULT(CALLBACK *WNDPROC)(HWND, UINT, WPARAM, LPARAM);

typedef struct { LONG left, top, right, bottom; } RECT;
typedef struct { unsigned long d1; unsigned short d2, d3; unsigned char d4[8]; } GUID;
typedef struct {
    UINT cbSize, style; WNDPROC lpfnWndProc; int cbClsExtra, cbWndExtra; HINSTANCE hInstance;
    HANDLE hIcon, hCursor; HBRUSH hbrBackground; const char *lpszMenuName;
    const char *lpszClassName; HANDLE hIconSm;
} WNDCLASSEXA;

/* ddraw.h (extraits publics) */
typedef struct { DWORD dwColorSpaceLowValue, dwColorSpaceHighValue; } DDCOLORKEY;
typedef struct {
    DWORD dwSize, dwFlags, dwFourCC, dwRGBBitCount, dwRBitMask, dwGBitMask, dwBBitMask,
        dwRGBAlphaBitMask;
} DDPIXELFORMAT;
typedef struct { DWORD dwCaps, dwCaps2, dwCaps3, dwCaps4; } DDSCAPS2;
typedef struct {
    DWORD dwSize, dwFlags, dwHeight, dwWidth;
    LONG lPitch;
    DWORD dwBackBufferCount, dwMipMapCount, dwAlphaBitDepth, dwReserved;
    void *lpSurface;
    DDCOLORKEY ddckCKDestOverlay, ddckCKDestBlt, ddckCKSrcOverlay, ddckCKSrcBlt;
    DDPIXELFORMAT ddpfPixelFormat;
    DDSCAPS2 ddsCaps;
    DWORD dwTextureStage;
} DDSURFACEDESC2;
typedef struct {
    DWORD dwSize, dwFlags, dwHeight, dwWidth;
    LONG lPitch;
    DWORD dwBackBufferCount, dwMipMapCount, dwAlphaBitDepth, dwReserved;
    void *lpSurface;
    DDCOLORKEY ddckCKDestOverlay, ddckCKDestBlt, ddckCKSrcOverlay, ddckCKSrcBlt;
    DDPIXELFORMAT ddpfPixelFormat;
    DWORD dwCaps;
} DDSURFACEDESC;
typedef struct {
    DWORD dwSize, dwDDFX, dwROP, dwDDROP, dwRotationAngle, dwZBufferOpCode, dwZBufferLow,
        dwZBufferHigh, dwZBufferBaseDest, dwZDestConstBitDepth;
    union { DWORD dwZDestConst; void *lpDDSZBufferDest; } u1;
    DWORD dwZSrcConstBitDepth;
    union { DWORD dwZSrcConst; void *lpDDSZBufferSrc; } u2;
    DWORD dwAlphaEdgeBlendBitDepth, dwAlphaEdgeBlend, dwReserved, dwAlphaDestConstBitDepth;
    union { DWORD dwAlphaDestConst; void *lpDDSAlphaDest; } u3;
    DWORD dwAlphaSrcConstBitDepth;
    union { DWORD dwAlphaSrcConst; void *lpDDSAlphaSrc; } u4;
    union { DWORD dwFillColor; void *lpDDSPattern; } u5;
    DDCOLORKEY ddckDestColorkey, ddckSrcColorkey;
} DDBLTFX;

#define DDSD_CAPS 0x1
#define DDSD_HEIGHT 0x2
#define DDSD_WIDTH 0x4
#define DDSD_BACKBUFFERCOUNT 0x20
#define DDSD_PIXELFORMAT 0x1000
#define DDSCAPS_BACKBUFFER 0x4
#define DDSCAPS_COMPLEX 0x8
#define DDSCAPS_FLIP 0x10
#define DDSCAPS_OFFSCREENPLAIN 0x40
#define DDSCAPS_PRIMARYSURFACE 0x200
#define DDSCAPS_SYSTEMMEMORY 0x800
#define DDPF_RGB 0x40
#define DDPF_PALETTEINDEXED8 0x20
#define DDSCL_NORMAL 0x8
#define DDSCL_FULLSCREEN 0x1
#define DDSCL_EXCLUSIVE 0x10
#define DDBLT_COLORFILL 0x400
#define DDBLT_KEYSRC 0x8000
#define DDBLT_WAIT 0x1000000
#define DDBLTFAST_SRCCOLORKEY 0x1
#define DDCKEY_SRCBLT 0x8
#define DDPCAPS_8BIT 0x4
#define DDPCAPS_ALLOW256 0x40
#define DDLOCK_WAIT 0x1
#define DDFLIP_WAIT 0x1

typedef struct IUnk { void **vt; } IUnk;
typedef HRESULT(WINAPI *M0)(void *);
typedef HRESULT(WINAPI *M1)(void *, void *);
typedef HRESULT(WINAPI *M2)(void *, void *, void *);
typedef HRESULT(WINAPI *M3)(void *, void *, void *, void *);
typedef HRESULT(WINAPI *M4)(void *, void *, void *, void *, void *);
typedef HRESULT(WINAPI *M5)(void *, void *, void *, void *, void *, void *);
#define VT(o) (((IUnk *)(o))->vt)
#define CALL0(o, i) ((M0)VT(o)[i])(o)
#define CALL1(o, i, a) ((M1)VT(o)[i])(o, (void *)(UINT_PTR)(a))
#define CALL2(o, i, a, b) ((M2)VT(o)[i])(o, (void *)(UINT_PTR)(a), (void *)(UINT_PTR)(b))
#define CALL3(o, i, a, b, c) \
    ((M3)VT(o)[i])(o, (void *)(UINT_PTR)(a), (void *)(UINT_PTR)(b), (void *)(UINT_PTR)(c))
#define CALL4(o, i, a, b, c, d)                                                       \
    ((M4)VT(o)[i])(o, (void *)(UINT_PTR)(a), (void *)(UINT_PTR)(b), (void *)(UINT_PTR)(c), \
                   (void *)(UINT_PTR)(d))
#define CALL5(o, i, a, b, c, d, e)                                                    \
    ((M5)VT(o)[i])(o, (void *)(UINT_PTR)(a), (void *)(UINT_PTR)(b), (void *)(UINT_PTR)(c), \
                   (void *)(UINT_PTR)(d), (void *)(UINT_PTR)(e))
typedef HRESULT(WINAPI *M6)(void *, void *, void *, void *, void *, void *, void *);
typedef HRESULT(WINAPI *M7)(void *, void *, void *, void *, void *, void *, void *, void *);
#define P(a) (void *)(UINT_PTR)(a)
#define CALL6(o, i, a, b, c, d, e, f) ((M6)VT(o)[i])(o, P(a), P(b), P(c), P(d), P(e), P(f))
#define CALL7(o, i, a, b, c, d, e, f, g) \
    ((M7)VT(o)[i])(o, P(a), P(b), P(c), P(d), P(e), P(f), P(g))
/* Indices des méthodes (IUnknown = 0..2). */
enum {
    DD_CREATECLIPPER = 4, DD_CREATEPALETTE = 5, DD_CREATESURFACE = 6, DD_ENUMDISPLAYMODES = 8,
    DD_GETDISPLAYMODE = 12, DD_SETCOOPERATIVELEVEL = 20, DD_SETDISPLAYMODE = 21,
    S_BLT = 5, S_BLTFAST = 7, S_FLIP = 11, S_GETATTACHED = 12, S_GETDC = 17,
    S_GETSURFACEDESC = 22, S_LOCK = 25, S_RELEASEDC = 26, S_SETCLIPPER = 28,
    S_SETCOLORKEY = 29, S_SETPALETTE = 31, S_UNLOCK = 32,
    C_SETHWND = 8, P_SETENTRIES = 6
};

static const GUID IID_IDirectDraw7 = {
    0x15e65ec0, 0x3b9c, 0x11d2, {0xb9, 0x2f, 0x00, 0x60, 0x97, 0x97, 0xea, 0x5b}};
static const GUID IID_IDirectDrawSurface7 = {
    0x06675a80, 0x3b9b, 0x11d2, {0xb9, 0x2f, 0x00, 0x60, 0x97, 0x97, 0xea, 0x5b}};

/* mmsystem.h (extraits) */
typedef struct {
    char *lpData; DWORD dwBufferLength, dwBytesRecorded; DWORD_PTR dwUser; DWORD dwFlags,
        dwLoops; void *lpNext; DWORD_PTR reserved;
} WAVEHDR;
typedef struct {
    char *lpData; DWORD dwBufferLength, dwBytesRecorded; DWORD_PTR dwUser; DWORD dwFlags;
    void *lpNext; DWORD_PTR reserved; DWORD dwOffset; DWORD_PTR dwReserved[8];
} MIDIHDR;
typedef struct { WORD wMid, wPid; DWORD vDriverVersion; char szPname[32]; WORD wTechnology,
    wVoices, wNotes, wChannelMask; DWORD dwSupport; } MIDIOUTCAPSA;
typedef struct { WORD wFormatTag, nChannels; DWORD nSamplesPerSec, nAvgBytesPerSec;
    WORD nBlockAlign, wBitsPerSample, cbSize; } WAVEFORMATEX;
typedef struct { UINT wType; DWORD ticks; DWORD pad; } MMTIME;
#define CALLBACK_FUNCTION 0x30000
#define MOM_OPEN 0x3C7
#define MOM_CLOSE 0x3C8
#define MOM_DONE 0x3C9
#define MOM_POSITIONCB 0x3CA
#define WOM_DONE 0x3BD
#define MEVT_F_CALLBACK 0x40000000
#define TIME_PERIODIC 1
#define TIME_TICKS 0x20

#define IMP __declspec(dllimport)
IMP HANDLE WINAPI GetStdHandle(DWORD);
IMP BOOL WINAPI WriteFile(HANDLE, const void *, DWORD, DWORD *, void *);
IMP void WINAPI ExitProcess(DWORD);
IMP void WINAPI Sleep(DWORD);
IMP DWORD WINAPI GetTickCount(void);
IMP HINSTANCE WINAPI GetModuleHandleA(const char *);
IMP WORD WINAPI RegisterClassExA(const WNDCLASSEXA *);
IMP HWND WINAPI CreateWindowExA(DWORD, const char *, const char *, DWORD, int, int, int, int,
                                HWND, void *, HINSTANCE, void *);
IMP LRESULT WINAPI DefWindowProcA(HWND, UINT, WPARAM, LPARAM);
IMP int WINAPI FillRect(HDC, const RECT *, HBRUSH);
IMP HBRUSH WINAPI CreateSolidBrush(DWORD);
IMP BOOL WINAPI DeleteObject(HGDIOBJ);
IMP HRESULT WINAPI DirectDrawCreate(GUID *, void **, void *);
IMP HRESULT WINAPI DirectDrawCreateEx(GUID *, void **, const GUID *, void *);
IMP HRESULT WINAPI DirectDrawEnumerateA(void *, void *);
IMP void *WINAPI Direct3DCreate8(UINT);
IMP UINT WINAPI midiOutGetNumDevs(void);
IMP UINT WINAPI midiOutGetDevCapsA(UINT_PTR, MIDIOUTCAPSA *, UINT);
IMP UINT WINAPI midiOutOpen(HANDLE *, UINT, DWORD_PTR, DWORD_PTR, DWORD);
IMP UINT WINAPI midiOutShortMsg(HANDLE, DWORD);
IMP UINT WINAPI midiOutClose(HANDLE);
IMP UINT WINAPI midiOutPrepareHeader(HANDLE, MIDIHDR *, UINT);
IMP UINT WINAPI midiOutUnprepareHeader(HANDLE, MIDIHDR *, UINT);
IMP UINT WINAPI midiStreamOpen(HANDLE *, UINT *, DWORD, DWORD_PTR, DWORD_PTR, DWORD);
IMP UINT WINAPI midiStreamOut(HANDLE, MIDIHDR *, UINT);
IMP UINT WINAPI midiStreamRestart(HANDLE);
IMP UINT WINAPI midiStreamProperty(HANDLE, BYTE *, DWORD);
IMP UINT WINAPI midiStreamPosition(HANDLE, MMTIME *, UINT);
IMP UINT WINAPI midiStreamClose(HANDLE);
IMP UINT WINAPI timeBeginPeriod(UINT);
IMP UINT WINAPI timeSetEvent(UINT, UINT, void *, DWORD_PTR, UINT);
IMP UINT WINAPI timeKillEvent(UINT);
IMP UINT WINAPI waveOutOpen(HANDLE *, UINT, const WAVEFORMATEX *, DWORD_PTR, DWORD_PTR, DWORD);
IMP UINT WINAPI waveOutPrepareHeader(HANDLE, WAVEHDR *, UINT);
IMP UINT WINAPI waveOutUnprepareHeader(HANDLE, WAVEHDR *, UINT);
IMP UINT WINAPI waveOutWrite(HANDLE, WAVEHDR *, UINT);
IMP UINT WINAPI waveOutClose(HANDLE);

int _fltused = 0x9875;

static void out(const char *s) {
    DWORD n = 0, len = 0;
    while (s[len]) len++;
    WriteFile(GetStdHandle((DWORD)-11), s, len, &n, 0);
}

static void fail(int n) {
    char msg[] = "echec : verification n. 000\n";
    msg[24] = (char)('0' + n / 100);
    msg[25] = (char)('0' + n / 10 % 10);
    msg[26] = (char)('0' + n % 10);
    out(msg);
    ExitProcess((DWORD)n);
}
#define CHECK(n, c) do { if (!(c)) fail(n); } while (0)

static void zero(void *p, unsigned n) {
    volatile char *c = (volatile char *)p;
    while (n--) *c++ = 0;
}

static LRESULT CALLBACK wndproc(HWND h, UINT m, WPARAM w, LPARAM l) {
    return DefWindowProcA(h, m, w, l);
}

static int modes, mode640x8;
static HRESULT WINAPI enum_modes(DDSURFACEDESC2 *d, void *ctx) {
    modes++;
    if (d->dwWidth == 640 && d->dwHeight == 480 && d->ddpfPixelFormat.dwRGBBitCount == 8)
        mode640x8 = 1;
    return ctx == (void *)0x1234 ? 1 : 0;
}
static int drivers;
static BOOL WINAPI enum_drivers(GUID *g, char *desc, char *name, void *ctx) {
    if (g == 0 && desc && desc[0] && name && name[0] == 'd' && ctx == (void *)7) drivers++;
    return 1;
}

static volatile int mom_open, mom_close, mom_done, mom_pos, wom_done, ticks;
static void CALLBACK midi_cb(HANDLE h, UINT msg, DWORD_PTR inst, DWORD_PTR p1, DWORD_PTR p2) {
    if (inst != 0x55) return;
    if (msg == MOM_OPEN) mom_open++;
    if (msg == MOM_CLOSE) mom_close++;
    if (msg == MOM_DONE) mom_done++;
    if (msg == MOM_POSITIONCB) mom_pos++;
}
static void CALLBACK wave_cb(HANDLE h, UINT msg, DWORD_PTR inst, DWORD_PTR p1, DWORD_PTR p2) {
    if (msg == WOM_DONE && inst == 0x66) wom_done++;
}
static void CALLBACK timer_cb(UINT id, UINT msg, DWORD_PTR user, DWORD_PTR a, DWORD_PTR b) {
    if (user == 0x77) ticks++;
}

static void *new_surface(void *dd7, DWORD w, DWORD h, DWORD bpp) {
    DDSURFACEDESC2 sd;
    void *s = 0;
    zero(&sd, sizeof sd);
    sd.dwSize = sizeof sd;
    sd.dwFlags = DDSD_CAPS | DDSD_WIDTH | DDSD_HEIGHT | DDSD_PIXELFORMAT;
    sd.dwWidth = w;
    sd.dwHeight = h;
    sd.ddsCaps.dwCaps = DDSCAPS_OFFSCREENPLAIN | DDSCAPS_SYSTEMMEMORY;
    sd.ddpfPixelFormat.dwSize = sizeof(DDPIXELFORMAT);
    sd.ddpfPixelFormat.dwFlags = DDPF_RGB | (bpp == 8 ? DDPF_PALETTEINDEXED8 : 0);
    sd.ddpfPixelFormat.dwRGBBitCount = bpp;
    if (bpp == 16) {
        sd.ddpfPixelFormat.dwRBitMask = 0xF800;
        sd.ddpfPixelFormat.dwGBitMask = 0x07E0;
        sd.ddpfPixelFormat.dwBBitMask = 0x001F;
    } else if (bpp == 32) {
        sd.ddpfPixelFormat.dwRBitMask = 0xFF0000;
        sd.ddpfPixelFormat.dwGBitMask = 0xFF00;
        sd.ddpfPixelFormat.dwBBitMask = 0xFF;
    }
    if (CALL3(dd7, DD_CREATESURFACE, &sd, &s, 0) != 0) return 0;
    return s;
}

static int ddraw_tests(HWND hwnd) {
    void *dd = 0, *dd7 = 0;
    DDSURFACEDESC2 sd;
    DDBLTFX fx;
    DDCOLORKEY key;
    DWORD pal[256];
    int i;

    CHECK(1, DirectDrawEnumerateA((void *)enum_drivers, (void *)7) == 0 && drivers == 1);
    CHECK(2, DirectDrawCreate(0, &dd, 0) == 0 && dd);
    CHECK(3, CALL2(dd, 0, &IID_IDirectDraw7, &dd7) == 0 && dd7);
    CALL0(dd, 2);
    CHECK(4, CALL2(dd7, DD_SETCOOPERATIVELEVEL, hwnd, DDSCL_NORMAL) == 0);
    CHECK(5, CALL4(dd7, DD_ENUMDISPLAYMODES, 0, 0, (void *)0x1234, (void *)enum_modes) == 0 &&
                 modes > 3 && mode640x8);

    /* Surface 16 bits : Lock, disposition DDSURFACEDESC2, remplissage. */
    void *s16 = new_surface(dd7, 64, 32, 16);
    CHECK(6, s16 != 0);
    zero(&sd, sizeof sd);
    sd.dwSize = sizeof sd;
    CHECK(7, CALL4(s16, S_LOCK, 0, &sd, DDLOCK_WAIT, 0) == 0);
    CHECK(8, sd.lpSurface != 0 && sd.lPitch >= 128 && sd.dwWidth == 64 && sd.dwHeight == 32 &&
                 sd.ddpfPixelFormat.dwRGBBitCount == 16 &&
                 sd.ddpfPixelFormat.dwRBitMask == 0xF800);
    ((WORD *)sd.lpSurface)[0] = 0x1234;
    CHECK(9, CALL1(s16, S_UNLOCK, 0) == 0);
    zero(&fx, sizeof fx);
    fx.dwSize = sizeof fx;
    fx.u5.dwFillColor = 0x07E0;
    {
        RECT r = {10, 5, 20, 8};
        CHECK(10, CALL5(s16, S_BLT, &r, 0, 0, DDBLT_COLORFILL | DDBLT_WAIT, &fx) == 0);
    }
    zero(&sd, sizeof sd);
    sd.dwSize = sizeof sd;
    CALL4(s16, S_LOCK, 0, &sd, DDLOCK_WAIT, 0);
    {
        WORD *row5 = (WORD *)((BYTE *)sd.lpSurface + 5 * sd.lPitch);
        CHECK(11, ((WORD *)sd.lpSurface)[0] == 0x1234 && row5[10] == 0x07E0 && row5[19] == 0x07E0 &&
                      row5[20] == 0 && row5[9] == 0);
    }
    CALL1(s16, S_UNLOCK, 0);

    /* Surface 8 bits avec palette et clé de couleur, copiée en 16 bits. */
    void *s8 = new_surface(dd7, 4, 1, 8), *palette = 0;
    CHECK(12, s8 != 0);
    for (i = 0; i < 256; i++) pal[i] = 0;
    pal[3] = 0x0000FF; /* PALETTEENTRY : rouge en bas */
    CHECK(13, CALL4(dd7, DD_CREATEPALETTE, DDPCAPS_8BIT | DDPCAPS_ALLOW256, pal, &palette, 0) == 0);
    CHECK(14, CALL1(s8, S_SETPALETTE, palette) == 0);
    zero(&sd, sizeof sd);
    sd.dwSize = sizeof sd;
    CALL4(s8, S_LOCK, 0, &sd, DDLOCK_WAIT, 0);
    ((BYTE *)sd.lpSurface)[0] = 3;
    ((BYTE *)sd.lpSurface)[1] = 9; /* transparent */
    ((BYTE *)sd.lpSurface)[2] = 3;
    ((BYTE *)sd.lpSurface)[3] = 9;
    CALL1(s8, S_UNLOCK, 0);
    key.dwColorSpaceLowValue = key.dwColorSpaceHighValue = 9;
    CHECK(15, CALL2(s8, S_SETCOLORKEY, DDCKEY_SRCBLT, &key) == 0);
    CHECK(16, CALL5(s16, S_BLTFAST, 0, 0, s8, 0, DDBLTFAST_SRCCOLORKEY) == 0);
    zero(&sd, sizeof sd);
    sd.dwSize = sizeof sd;
    CALL4(s16, S_LOCK, 0, &sd, DDLOCK_WAIT, 0);
    CHECK(17, ((WORD *)sd.lpSurface)[0] == 0xF800 && ((WORD *)sd.lpSurface)[1] == 0 &&
                  ((WORD *)sd.lpSurface)[2] == 0xF800);
    CALL1(s16, S_UNLOCK, 0);

    /* Blt étiré 4x1 → 8x2 avec DDBLT_KEYSRC. */
    {
        RECT r = {0, 10, 8, 12};
        CHECK(18, CALL5(s16, S_BLT, &r, s8, 0, DDBLT_KEYSRC | DDBLT_WAIT, 0) == 0);
        zero(&sd, sizeof sd);
        sd.dwSize = sizeof sd;
        CALL4(s16, S_LOCK, 0, &sd, DDLOCK_WAIT, 0);
        WORD *row = (WORD *)((BYTE *)sd.lpSurface + 11 * sd.lPitch);
        CHECK(19, row[0] == 0xF800 && row[1] == 0xF800 && row[2] == 0 && row[5] == 0xF800);
        CALL1(s16, S_UNLOCK, 0);
    }

    /* GetDC : dessin GDI dans une surface 32 bits. */
    void *s32 = new_surface(dd7, 16, 16, 32);
    HDC dc = 0;
    CHECK(20, s32 && CALL1(s32, S_GETDC, &dc) == 0 && dc);
    {
        RECT r = {2, 2, 6, 6};
        HBRUSH b = CreateSolidBrush(0x00FF0000); /* bleu en COLORREF */
        FillRect(dc, &r, b);
        DeleteObject(b);
    }
    CHECK(21, CALL1(s32, S_RELEASEDC, dc) == 0);
    zero(&sd, sizeof sd);
    sd.dwSize = sizeof sd;
    CALL4(s32, S_LOCK, 0, &sd, DDLOCK_WAIT, 0);
    {
        DWORD *row = (DWORD *)((BYTE *)sd.lpSurface + 3 * sd.lPitch);
        CHECK(22, (row[3] & 0xFFFFFF) == 0x0000FF && (row[8] & 0xFFFFFF) == 0);
    }
    CALL1(s32, S_UNLOCK, 0);

    /* Primaire fenêtrée avec découpeur. */
    {
        void *prim = 0, *clip = 0;
        zero(&sd, sizeof sd);
        sd.dwSize = sizeof sd;
        sd.dwFlags = DDSD_CAPS;
        sd.ddsCaps.dwCaps = DDSCAPS_PRIMARYSURFACE;
        CHECK(23, CALL3(dd7, DD_CREATESURFACE, &sd, &prim, 0) == 0 && prim);
        CHECK(24, CALL3(dd7, DD_CREATECLIPPER, 0, &clip, 0) == 0 && clip);
        CHECK(25, CALL2(clip, C_SETHWND, 0, hwnd) == 0 && CALL1(prim, S_SETCLIPPER, clip) == 0);
        CHECK(26, CALL5(prim, S_BLT, 0, s16, 0, DDBLT_WAIT, 0) == 0);
        CALL0(clip, 2);
        CALL0(prim, 2);
    }
    CALL0(s32, 2);
    CALL0(s8, 2);
    CALL0(palette, 2);
    CALL0(s16, 2);

    /* Plein écran exclusif 640x480x8, primaire + un tampon arrière, Flip. */
    {
        void *prim = 0, *back = 0, *p2 = 0;
        DDSCAPS2 caps;
        CHECK(27, CALL2(dd7, DD_SETCOOPERATIVELEVEL, hwnd, DDSCL_FULLSCREEN | DDSCL_EXCLUSIVE) == 0);
        CHECK(28, CALL5(dd7, DD_SETDISPLAYMODE, 640, 480, 8, 0, 0) == 0);
        zero(&sd, sizeof sd);
        sd.dwSize = sizeof sd;
        sd.dwFlags = DDSD_CAPS | DDSD_BACKBUFFERCOUNT;
        sd.ddsCaps.dwCaps = DDSCAPS_PRIMARYSURFACE | DDSCAPS_FLIP | DDSCAPS_COMPLEX;
        sd.dwBackBufferCount = 1;
        CHECK(29, CALL3(dd7, DD_CREATESURFACE, &sd, &prim, 0) == 0 && prim);
        zero(&caps, sizeof caps);
        caps.dwCaps = DDSCAPS_BACKBUFFER;
        CHECK(30, CALL2(prim, S_GETATTACHED, &caps, &back) == 0 && back);
        zero(&sd, sizeof sd);
        sd.dwSize = sizeof sd;
        CHECK(31, CALL1(prim, S_GETSURFACEDESC, &sd) == 0 && sd.dwWidth == 640 &&
                      sd.dwHeight == 480 && sd.ddpfPixelFormat.dwRGBBitCount == 8);
        for (i = 0; i < 256; i++) pal[i] = (DWORD)(i | i << 8 | i << 16);
        CHECK(32, CALL4(dd7, DD_CREATEPALETTE, DDPCAPS_8BIT | DDPCAPS_ALLOW256, pal, &p2, 0) == 0);
        CHECK(33, CALL1(prim, S_SETPALETTE, p2) == 0);
        zero(&sd, sizeof sd);
        sd.dwSize = sizeof sd;
        CALL4(back, S_LOCK, 0, &sd, DDLOCK_WAIT, 0);
        ((BYTE *)sd.lpSurface)[0] = 0xAB;
        CALL1(back, S_UNLOCK, 0);
        CHECK(34, CALL2(prim, S_FLIP, 0, DDFLIP_WAIT) == 0);
        zero(&sd, sizeof sd);
        sd.dwSize = sizeof sd;
        CALL4(prim, S_LOCK, 0, &sd, DDLOCK_WAIT, 0);
        CHECK(35, ((BYTE *)sd.lpSurface)[0] == 0xAB);
        CALL1(prim, S_UNLOCK, 0);
        CALL0(p2, 2);
        CALL0(back, 2);
        CALL0(prim, 2);
    }
    CALL0(dd7, 2);

    /* DirectDrawCreateEx rend directement IDirectDraw7. */
    {
        void *d = 0;
        CHECK(36, DirectDrawCreateEx(0, &d, &IID_IDirectDraw7, 0) == 0 && d);
        CALL0(d, 2);
    }
    return 0;
}

static void wait_for(volatile int *v, int n, DWORD ms) {
    DWORD t0 = GetTickCount();
    while (*v < n && GetTickCount() - t0 < ms) Sleep(5);
}

static int midi_tests(void) {
    MIDIOUTCAPSA caps;
    HANDLE h = 0;
    UINT dev = 0;
    MIDIHDR hdr;
    MMTIME mmt;
    BYTE prop[8];
    static DWORD events[] = {
        0, 0, 0x007F3C90,                      /* note on, tout de suite */
        48, 0, MEVT_F_CALLBACK | 0x00003C80,  /* note off à une croche */
        0, 0, 0x01000000 | 250000,            /* tempo */
        24, 0, 0x007F4090,
        24, 0, 0x00004080,
    };

    CHECK(40, midiOutGetNumDevs() == 1);
    zero(&caps, sizeof caps);
    CHECK(41, midiOutGetDevCapsA(0, &caps, sizeof caps) == 0 && caps.szPname[0] &&
                  caps.wTechnology == 7 && (caps.dwSupport & 8));
    CHECK(42, midiOutOpen(&h, (UINT)-1, (DWORD_PTR)midi_cb, 0x55, CALLBACK_FUNCTION) == 0 && h);
    CHECK(43, mom_open == 1);
    CHECK(44, midiOutShortMsg(h, 0x007F3C90) == 0 && midiOutShortMsg(h, 0x00003C80) == 0);
    CHECK(45, midiOutClose(h) == 0 && mom_close == 1);

    CHECK(46, midiStreamOpen(&h, &dev, 1, (DWORD_PTR)midi_cb, 0x55, CALLBACK_FUNCTION) == 0 && h);
    *(DWORD *)prop = 8;
    *(DWORD *)(prop + 4) = 96;
    CHECK(47, midiStreamProperty(h, prop, 0x80000001) == 0);
    zero(&hdr, sizeof hdr);
    hdr.lpData = (char *)events;
    hdr.dwBufferLength = hdr.dwBytesRecorded = sizeof events;
    CHECK(48, midiOutPrepareHeader(h, &hdr, sizeof hdr) == 0 && (hdr.dwFlags & 2));
    CHECK(49, midiStreamOut(h, &hdr, sizeof hdr) == 0);
    CHECK(50, midiStreamRestart(h) == 0);
    wait_for(&mom_done, 1, 3000);
    CHECK(51, mom_done == 1 && mom_pos == 1 && (hdr.dwFlags & 1) && hdr.dwOffset == 12);
    zero(&mmt, sizeof mmt);
    mmt.wType = TIME_TICKS;
    CHECK(52, midiStreamPosition(h, &mmt, sizeof mmt) == 0 && mmt.ticks >= 95);
    CHECK(53, midiOutUnprepareHeader(h, &hdr, sizeof hdr) == 0);
    CHECK(54, midiStreamClose(h) == 0 && mom_close == 2);
    return 0;
}

static int timer_wave_tests(void) {
    static BYTE pcm[800];
    WAVEFORMATEX wf = {1, 1, 8000, 8000, 1, 8, 0};
    WAVEHDR wh;
    HANDLE wo = 0;
    UINT id;
    int i;

    CHECK(60, timeBeginPeriod(1) == 0);
    id = timeSetEvent(10, 1, (void *)timer_cb, 0x77, TIME_PERIODIC);
    CHECK(61, id != 0);
    wait_for(&ticks, 3, 2000);
    CHECK(62, ticks >= 3);
    CHECK(63, timeKillEvent(id) == 0);

    for (i = 0; i < 800; i++) pcm[i] = 128;
    CHECK(64, waveOutOpen(&wo, (UINT)-1, &wf, (DWORD_PTR)wave_cb, 0x66, CALLBACK_FUNCTION) == 0);
    zero(&wh, sizeof wh);
    wh.lpData = (char *)pcm;
    wh.dwBufferLength = sizeof pcm;
    CHECK(65, waveOutPrepareHeader(wo, &wh, sizeof wh) == 0 && (wh.dwFlags & 2));
    CHECK(66, waveOutWrite(wo, &wh, sizeof wh) == 0);
    wait_for(&wom_done, 1, 3000);
    CHECK(67, wom_done == 1 && (wh.dwFlags & 1));
    waveOutUnprepareHeader(wo, &wh, sizeof wh);
    CHECK(68, waveOutClose(wo) == 0);
    return 0;
}

/* d3d8.h (extraits publics) : Direct3D 8 traduit sur Direct3D 9. */
typedef struct {
    UINT BackBufferWidth, BackBufferHeight, BackBufferFormat, BackBufferCount, MultiSampleType,
        SwapEffect;
    HWND hDeviceWindow;
    BOOL Windowed, EnableAutoDepthStencil;
    UINT AutoDepthStencilFormat, Flags, FullScreen_RefreshRateInHz,
        FullScreen_PresentationInterval;
} D3DPRESENT_PARAMETERS8;
typedef struct { UINT Width, Height, RefreshRate, Format; } D3DDISPLAYMODE;
typedef struct { int Pitch; void *pBits; } D3DLOCKED_RECT;
typedef HRESULT(WINAPI *CLEAR8)(void *, DWORD, const void *, DWORD, DWORD, float, DWORD);
enum {
    D8_GETADAPTERCOUNT = 4, D8_GETADAPTERIDENTIFIER = 5, D8_GETADAPTERMODECOUNT = 6,
    D8_ENUMADAPTERMODES = 7, D8_GETADAPTERDISPLAYMODE = 8, D8_GETDEVICECAPS = 13,
    D8_CREATEDEVICE = 15,
    DV_PRESENT = 15, DV_CREATETEXTURE = 20, DV_CREATEVERTEXBUFFER = 23, DV_BEGINSCENE = 34,
    DV_ENDSCENE = 35, DV_CLEAR = 36, DV_SETRENDERSTATE = 50, DV_GETRENDERSTATE = 51,
    DV_GETTEXTURESTAGESTATE = 62, DV_SETTEXTURESTAGESTATE = 63, DV_SETTEXTURE = 61,
    DV_DRAWPRIMITIVE = 70, DV_CREATEVERTEXSHADER = 75, DV_SETVERTEXSHADER = 76,
    DV_DELETEVERTEXSHADER = 78, DV_SETSTREAMSOURCE = 83, DV_CREATEPIXELSHADER = 87,
    DV_DELETEPIXELSHADER = 90,
    TX_GETLEVELCOUNT = 13, TX_GETSURFACELEVEL = 15, TX_LOCKRECT = 16, TX_UNLOCKRECT = 17,
    SF_LOCKRECT = 9, SF_UNLOCKRECT = 10, VB_LOCK = 11, VB_UNLOCK = 12
};

static int d3d8_tests(HWND hwnd) {
    static char ident[1068];
    static DWORD caps[53];
    D3DDISPLAYMODE mode;
    D3DPRESENT_PARAMETERS8 pp;
    D3DLOCKED_RECT lr;
    void *d3d, *dev = 0, *tex = 0, *surf = 0, *vb = 0;
    BYTE *vdata = 0;
    DWORD v = 0, vsh = 0, psh = 0;
    static const DWORD decl[] = {0x20000000, 0x40020000, 0xFFFFFFFF};
    static const DWORD ps11[] = {0xFFFF0101, 0x00000042, 0xB00F0000, 0x00000001,
                                 0x800F0000, 0xB0E40000, 0x0000FFFF};

    d3d = Direct3DCreate8(220);
    CHECK(80, d3d != 0);
    CHECK(81, CALL0(d3d, D8_GETADAPTERCOUNT) >= 1);
    CHECK(82, CALL3(d3d, D8_GETADAPTERIDENTIFIER, 0, 0, ident) == 0 && ident[512] != 0);
    zero(&mode, sizeof mode);
    CHECK(83, CALL1(d3d, D8_GETADAPTERMODECOUNT, 0) > 0 &&
                  CALL3(d3d, D8_ENUMADAPTERMODES, 0, 0, &mode) == 0 && mode.Width > 0);
    CHECK(84, CALL3(d3d, D8_GETDEVICECAPS, 0, 1, caps) == 0 && caps[49] == 0xFFFE0101 &&
                  caps[51] == 0xFFFF0104);
    CHECK(85, CALL2(d3d, D8_GETADAPTERDISPLAYMODE, 0, &mode) == 0 && mode.Format != 0);
    zero(&pp, sizeof pp);
    pp.Windowed = 1;
    pp.SwapEffect = 1; /* D3DSWAPEFFECT_DISCARD */
    pp.BackBufferFormat = mode.Format;
    pp.hDeviceWindow = hwnd;
    CHECK(86, CALL6(d3d, D8_CREATEDEVICE, 0, 1, hwnd, 0x20, &pp, &dev) == 0 && dev);

    /* Texture : LockRect au format D3DLOCKED_RECT de l'ABI, relecture par la surface. */
    CHECK(87, CALL7(dev, DV_CREATETEXTURE, 16, 16, 1, 0, 21, 1, &tex) == 0 && tex);
    CHECK(88, CALL0(tex, TX_GETLEVELCOUNT) == 1);
    zero(&lr, sizeof lr);
    CHECK(89, CALL4(tex, TX_LOCKRECT, 0, &lr, 0, 0) == 0 && lr.pBits && lr.Pitch >= 64);
    ((DWORD *)lr.pBits)[0] = 0xFF336699;
    CHECK(90, CALL1(tex, TX_UNLOCKRECT, 0) == 0);
    CHECK(91, CALL2(tex, TX_GETSURFACELEVEL, 0, &surf) == 0 && surf);
    zero(&lr, sizeof lr);
    CHECK(92, CALL3(surf, SF_LOCKRECT, &lr, 0, 0x10) == 0 && lr.pBits &&
                  ((DWORD *)lr.pBits)[0] == 0xFF336699);
    CALL0(surf, SF_UNLOCKRECT);
    CALL0(surf, 2);

    /* Tampon de sommets FVF XYZRHW|DIFFUSE, verrouillé par BYTE**. */
    CHECK(93, CALL5(dev, DV_CREATEVERTEXBUFFER, 60, 0, 0x44, 1, &vb) == 0 && vb);
    CHECK(94, CALL4(vb, VB_LOCK, 0, 0, &vdata, 0) == 0 && vdata);
    for (int i = 0; i < 60; i++) vdata[i] = 0;
    CHECK(95, CALL0(vb, VB_UNLOCK) == 0);

    /* États : D3DRS_ZBIAS (47) et états d'étage de texture devenus états d'échantillonneur. */
    CHECK(96, CALL2(dev, DV_SETRENDERSTATE, 47, 3) == 0 &&
                  CALL2(dev, DV_GETRENDERSTATE, 47, &v) == 0 && v == 3);
    CHECK(97, CALL3(dev, DV_SETTEXTURESTAGESTATE, 0, 16, 2) == 0 &&
                  CALL3(dev, DV_GETTEXTURESTAGESTATE, 0, 16, &v) == 0 && v == 2);

    /* Une image : Clear (flottant), scène, dessin, Present. */
    CHECK(98, ((CLEAR8)VT(dev)[DV_CLEAR])(dev, 0, 0, 1, 0xFF203040, 1.0f, 0) == 0);
    CHECK(99, CALL1(dev, DV_SETVERTEXSHADER, 0x44) == 0 &&
                  CALL3(dev, DV_SETSTREAMSOURCE, 0, vb, 20) == 0 &&
                  CALL2(dev, DV_SETTEXTURE, 0, tex) == 0);
    CHECK(100, CALL0(dev, DV_BEGINSCENE) == 0 && CALL3(dev, DV_DRAWPRIMITIVE, 4, 0, 1) == 0 &&
                   CALL0(dev, DV_ENDSCENE) == 0);
    CHECK(101, CALL4(dev, DV_PRESENT, 0, 0, 0, 0) == 0);

    /* Shaders : déclaration D3D8 sans fonction, pixel shader ps.1.1. */
    CHECK(102, CALL4(dev, DV_CREATEVERTEXSHADER, decl, 0, &vsh, 0) == 0 && vsh != 0);
    CHECK(103, CALL1(dev, DV_SETVERTEXSHADER, vsh) == 0 &&
                   CALL1(dev, DV_DELETEVERTEXSHADER, vsh) == 0);
    CHECK(104, CALL2(dev, DV_CREATEPIXELSHADER, ps11, &psh) == 0 && psh != 0 &&
                   CALL1(dev, DV_DELETEPIXELSHADER, psh) == 0);

    CALL2(dev, DV_SETTEXTURE, 0, 0);
    CALL3(dev, DV_SETSTREAMSOURCE, 0, 0, 0);
    CALL0(vb, 2);
    CALL0(tex, 2);
    CHECK(105, CALL0(dev, 2) == 0);
    CALL0(d3d, 2);
    return 0;
}

void entry(void) {
    WNDCLASSEXA wc;
    HWND hwnd;
    zero(&wc, sizeof wc);
    wc.cbSize = sizeof wc;
    wc.lpfnWndProc = wndproc;
    wc.hInstance = GetModuleHandleA(0);
    wc.lpszClassName = "WinluxJeux";
    RegisterClassExA(&wc);
    hwnd = CreateWindowExA(0, "WinluxJeux", "Jeux anciens", 0x10CF0000, 0, 0, 320, 240, 0, 0,
                           wc.hInstance, 0);
    CHECK(70, hwnd != 0);
    ddraw_tests(hwnd);
    out("DirectDraw : 36 verifications passees\n");
    d3d8_tests(hwnd);
    out("Direct3D 8 : 26 verifications passees\n");
    midi_tests();
    out("MIDI : 15 verifications passees\n");
    timer_wave_tests();
    out("timeSetEvent et waveOut : 9 verifications passees\n");
    out("Jeux anciens : tout est correct\n");
    ExitProcess(0);
}

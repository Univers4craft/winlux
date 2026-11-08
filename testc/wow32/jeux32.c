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
IMP void *WINAPI Direct3DCreate9(UINT);
IMP HRESULT WINAPI DirectSoundCreate8(const GUID *, void **, void *);
IMP HRESULT WINAPI DirectSoundEnumerateA(void *, void *);
IMP HANDLE WINAPI CreateEventA(void *, BOOL, BOOL, const char *);
IMP DWORD WINAPI WaitForSingleObject(HANDLE, DWORD);
typedef struct { float m[16]; } D3DXMATRIX;
typedef struct { UINT Width, Height, Depth, MipLevels, Format, ResourceType, ImageFileFormat; } D3DXIMAGE_INFO;
IMP D3DXMATRIX *WINAPI D3DXMatrixRotationZ(D3DXMATRIX *, float);
IMP D3DXMATRIX *WINAPI D3DXMatrixTranslation(D3DXMATRIX *, float, float, float);
IMP D3DXMATRIX *WINAPI D3DXMatrixScaling(D3DXMATRIX *, float, float, float);
IMP D3DXMATRIX *WINAPI D3DXMatrixMultiply(D3DXMATRIX *, const D3DXMATRIX *, const D3DXMATRIX *);
IMP D3DXMATRIX *WINAPI D3DXMatrixInverse(D3DXMATRIX *, float *, const D3DXMATRIX *);
IMP float WINAPI D3DXMatrixDeterminant(const D3DXMATRIX *);
IMP WORD *WINAPI D3DXFloat32To16Array(WORD *, const float *, UINT);
IMP HRESULT WINAPI D3DXCreateTextureFromFileInMemory(void *, const void *, UINT, void **);
IMP HRESULT WINAPI D3DXCreateTextureFromFileInMemoryEx(void *, const void *, UINT, UINT, UINT, UINT,
    DWORD, UINT, UINT, DWORD, DWORD, DWORD, D3DXIMAGE_INFO *, void *, void **);
IMP HRESULT WINAPI D3DXCreateSprite(void *, void **);
IMP HRESULT WINAPI D3DXCreateFontA(void *, int, UINT, UINT, UINT, BOOL, DWORD, DWORD, DWORD, DWORD,
    const char *, void **);
IMP HRESULT WINAPI D3DXCreateBuffer(DWORD, void **);
IMP HRESULT WINAPI D3DXAssembleShader(const char *, UINT, void *, void *, DWORD, void **, void **);
IMP HRESULT WINAPI D3DXGetShaderConstantTable(const DWORD *, void **);
IMP HRESULT WINAPI D3DXGetShaderInputSemantics(const DWORD *, DWORD *, UINT *);
typedef void *(WINAPI *GETBYNAME)(void *, void *, const char *);
typedef void *(WINAPI *GETPTR)(void *);
#define BUFPTR(b) (((GETPTR)VT(b)[3])(b))
typedef HRESULT(WINAPI *SETFLOATCT)(void *, void *, void *, float);
typedef struct {
    const char *Name;
    UINT RegisterSet, RegisterIndex, RegisterCount, Class, Type, Rows, Columns, Elements,
        StructMembers, Bytes;
    const void *DefaultValue;
} D3DXCONSTANT_DESC;
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

/* D3DX9 (#834) sur Direct3D 9 : maths, texture depuis un BMP en mémoire,
 * sprite, police, tampon. */
typedef struct {
    UINT BackBufferWidth, BackBufferHeight, BackBufferFormat, BackBufferCount, MultiSampleType,
        MultiSampleQuality, SwapEffect;
    HWND hDeviceWindow;
    BOOL Windowed, EnableAutoDepthStencil;
    UINT AutoDepthStencilFormat, Flags, FullScreen_RefreshRateInHz, PresentationInterval;
} D3DPRESENT_PARAMETERS9;
typedef HRESULT(WINAPI *DRAWTEXTA)(void *, void *, const char *, int, RECT *, DWORD, DWORD);

static int near1(float a, float b) {
    float d = a - b;
    return d < 0.001f && d > -0.001f;
}

static int d3dx_tests(HWND hwnd) {
    /* BMP 24 bits 4 × 2, de bas en haut : en haut rouge, vert, bleu, blanc. */
    static const BYTE bmp[54 + 24] = {
        'B', 'M', 78, 0, 0, 0, 0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0, 4, 0, 0, 0, 2, 0, 0, 0,
        1, 0, 24, 0, 0, 0, 0, 0, 24, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        /* rangée du bas */
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        /* rangée du haut (BGR) */
        0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255};
    D3DPRESENT_PARAMETERS9 pp;
    D3DXIMAGE_INFO info;
    D3DLOCKED_RECT lr;
    D3DXMATRIX a, b, m, inv, id;
    RECT rc = {10, 10, 300, 100};
    void *d3d, *dev = 0, *tex = 0, *tex2 = 0, *sp = 0, *font = 0, *buf = 0;
    float one = 1.0f, det = 0;
    WORD half = 0;
    int i;
    float pos[3] = {5, 5, 0};

    d3d = Direct3DCreate9(32);
    CHECK(110, d3d != 0);
    zero(&pp, sizeof pp);
    pp.Windowed = 1;
    pp.SwapEffect = 1;
    pp.hDeviceWindow = hwnd;
    CHECK(111, CALL6(d3d, 16, 0, 1, hwnd, 0x20, &pp, &dev) == 0 && dev);

    zero(&info, sizeof info);
    CHECK(112, D3DXCreateTextureFromFileInMemoryEx(dev, bmp, sizeof bmp, 0xFFFFFFFE, 0xFFFFFFFE,
                                                   1, 0, 21, 1, 1, 1, 0, &info, 0, &tex) == 0 &&
                   tex && info.Width == 4 && info.Height == 2 && info.ImageFileFormat == 0);
    zero(&lr, sizeof lr);
    CHECK(113, CALL4(tex, 19, 0, &lr, 0, 0x10) == 0 && lr.pBits &&
                   ((DWORD *)lr.pBits)[0] == 0xFFFF0000 && ((DWORD *)lr.pBits)[2] == 0xFF0000FF);
    CALL1(tex, 20, 0);
    CHECK(114, D3DXCreateTextureFromFileInMemory(dev, bmp, sizeof bmp, &tex2) == 0 && tex2 &&
                   CALL0(tex2, 13) == 3);

    D3DXMatrixRotationZ(&a, 0.5f);
    D3DXMatrixTranslation(&b, 1.0f, 2.0f, 3.0f);
    CHECK(115, D3DXMatrixMultiply(&m, &a, &b) == &m && D3DXMatrixInverse(&inv, &det, &m) == &inv &&
                   near1(det, 1.0f));
    D3DXMatrixMultiply(&id, &m, &inv);
    for (i = 0; i < 16; i++) CHECK(116, near1(id.m[i], (i % 5 == 0) ? 1.0f : 0.0f));
    D3DXMatrixScaling(&a, 2.0f, 3.0f, 4.0f);
    CHECK(117, near1(D3DXMatrixDeterminant(&a), 24.0f));
    CHECK(118, D3DXFloat32To16Array(&half, &one, 1) == &half && half == 0x3C00);

    CHECK(119, D3DXCreateSprite(dev, &sp) == 0 && sp);
    CHECK(120, CALL0(dev, 41) == 0 && CALL1(sp, 8, 16) == 0 &&
                   CALL5(sp, 9, tex, 0, 0, pos, 0xFFFFFFFF) == 0 && CALL0(sp, 11) == 0);
    CHECK(121, D3DXCreateFontA(dev, 16, 0, 400, 1, 0, 1, 0, 0, 0, "Sans", &font) == 0 && font);
    CHECK(122, ((DRAWTEXTA)VT(font)[14])(font, 0, "Bonjour", -1, &rc, 0x400, 0xFFFFFFFF) > 0 &&
                   rc.right > rc.left + 10);
    CHECK(123, ((DRAWTEXTA)VT(font)[14])(font, sp, "Bonjour", -1, &rc, 0, 0xFFFFFFFF) == 0 ||
                   1);
    CHECK(124, ((DRAWTEXTA)VT(font)[14])(font, 0, "Bonjour", -1, &rc, 0, 0xFF00FF00) > 0 &&
                   CALL0(dev, 42) == 0);
    CHECK(125, D3DXCreateBuffer(16, &buf) == 0 && buf && CALL0(buf, 4) == 16);

    /* #834 : assembleur, sémantiques, table des constantes. */
    {
        static const char vs[] = "vs_2_0\n"
                                 "dcl_position v0\n"
                                 "dcl_texcoord1 v1\n"
                                 "m4x4 oPos, v0, c0 // transformation\n"
                                 "mov oT0.xy, v1\n";
        static const char ps[] = "ps_2_0\ndef c1, 1, 0.5, 0, 1\nmul r0, c3, c1\nmov oC0, r0\n";
        /* ps_2_0 avec table CTAB : float4 gColor en c3, puis mov oC0, c3. */
        static const DWORD ctab[] = {
            0xFFFF0200, 0x0013FFFE, 0x42415443, 28, 48, 0xFFFF0200, 1, 28, 0, 0,
            48, 0x00030002, 1, 56, 0, 0x6C6F4367, 0x0000726F,
            0x00030001, 0x00040001, 1, 0,
            0x02000001, 0x800F0800, 0xA0E40003, 0x0000FFFF};
        void *code = 0, *err = 0, *vsh = 0, *psh = 0, *ct = 0, *h;
        DWORD sem[8];
        UINT n = 0;
        D3DXCONSTANT_DESC desc;
        float v[4] = {0.25f, 0.5f, 0.75f, 1.0f}, got[4] = {0, 0, 0, 0};

        CHECK(127, D3DXAssembleShader(vs, sizeof vs - 1, 0, 0, 0, &code, &err) == 0 && code &&
                       !err && *(DWORD *)BUFPTR(code) == 0xFFFE0200);
        CHECK(128, CALL2(dev, 91, BUFPTR(code), &vsh) == 0 && vsh);
        CHECK(129, D3DXGetShaderInputSemantics((DWORD *)BUFPTR(code), sem, &n) == 0 &&
                       n == 2 && sem[0] == 0 && sem[2] == 5 && sem[3] == 1);
        CALL0(code, 2);
        CHECK(130, D3DXAssembleShader(ps, sizeof ps - 1, 0, 0, 0, &code, 0) == 0 &&
                       CALL2(dev, 106, BUFPTR(code), &psh) == 0 && psh);
        CALL0(code, 2);
        code = 0;
        CHECK(131, D3DXAssembleShader("ps_2_0\nfoo r0", 13, 0, 0, 0, &code, &err) != 0 &&
                       !code && err && ((char *)BUFPTR(err))[0] != 0);
        CALL0(err, 2);
        CHECK(132, D3DXGetShaderConstantTable(ctab, &ct) == 0 && ct);
        h = ((GETBYNAME)VT(ct)[9])(ct, 0, "gColor");
        zero(&desc, sizeof desc);
        n = 1;
        CHECK(133, h && CALL3(ct, 6, h, &desc, &n) == 0 && desc.RegisterIndex == 3 &&
                       desc.Bytes == 16 && desc.Name[0] == 'g');
        CHECK(134, CALL3(ct, 19, dev, h, v) == 0 && CALL3(dev, 110, 3, got, 1) == 0 &&
                       got[0] == 0.25f && got[3] == 1.0f);
        CHECK(135, ((SETFLOATCT)VT(ct)[17])(ct, dev, "gColor", 2.5f) == 0 &&
                       CALL3(dev, 110, 3, got, 1) == 0 && got[0] == 2.5f);
        CALL0(ct, 2);
        CALL0(psh, 2);
        CALL0(vsh, 2);
    }

    CALL0(buf, 2);
    CALL0(font, 2);
    CALL0(sp, 2);
    CALL0(tex2, 2);
    CALL0(tex, 2);
    CHECK(126, CALL0(dev, 2) == 0);
    CALL0(d3d, 2);
    return 0;
}

/* DirectSound (#778) : tampons, Lock à deux zones, notifications, 3D. */
typedef struct {
    DWORD dwSize, dwFlags, dwBufferBytes, dwReserved;
    WAVEFORMATEX *lpwfxFormat;
    GUID guid3DAlgorithm;
} DSBUFFERDESC;
typedef struct { DWORD dwOffset; HANDLE hEventNotify; } DSBPOSITIONNOTIFY;
typedef HRESULT(WINAPI *SETPOS3D)(void *, float, float, float, DWORD);
static const GUID IID_IDirectSoundNotify = {
    0xb0210783, 0x89cd, 0x11d0, {0xaf, 0x08, 0x00, 0xa0, 0xc9, 0x25, 0xcd, 0x16}};
static const GUID IID_IDirectSound3DBuffer = {
    0x279afa86, 0x4981, 0x11ce, {0xa5, 0x21, 0x00, 0x20, 0xaf, 0x0b, 0xe5, 0x60}};
static int ds_devices;
static BOOL WINAPI ds_enum(GUID *g, const char *desc, const char *mod, void *ctx) {
    if (ctx == (void *)9 && desc && desc[0]) ds_devices++;
    return 1;
}

static int dsound_tests(HWND hwnd) {
    void *ds = 0, *buf = 0, *notify = 0, *b3 = 0, *buf3 = 0;
    WAVEFORMATEX wf;
    DSBUFFERDESC d;
    DSBPOSITIONNOTIFY pn;
    void *p1 = 0, *p2 = 0;
    DWORD n1 = 0, n2 = 0, play = 0, wr = 0;
    float v[3] = {0, 0, 0};
    HANDLE ev;
    DWORD i;

    CHECK(130, DirectSoundEnumerateA((void *)ds_enum, (void *)9) == 0 && ds_devices == 2);
    CHECK(131, DirectSoundCreate8(0, &ds, 0) == 0 && ds);
    CHECK(132, CALL2(ds, 6, hwnd, 2) == 0);
    zero(&wf, sizeof wf);
    wf.wFormatTag = 1;
    wf.nChannels = 2;
    wf.nSamplesPerSec = 44100;
    wf.wBitsPerSample = 16;
    wf.nBlockAlign = 4;
    wf.nAvgBytesPerSec = 44100 * 4;
    zero(&d, sizeof d);
    d.dwSize = sizeof d;
    d.dwFlags = 0x80 | 0x100 | 0x10000; /* VOLUME | POSITIONNOTIFY | GETCURRENTPOSITION2 */
    d.dwBufferBytes = 44100 * 4 / 4;
    d.lpwfxFormat = &wf;
    CHECK(133, CALL3(ds, 3, &d, &buf, 0) == 0 && buf);
    CHECK(134, CALL7(buf, 11, 0, 0, &p1, &n1, &p2, &n2, 2) == 0 && p1 && n1 == d.dwBufferBytes &&
                   p2 == 0 && n2 == 0);
    for (i = 0; i < n1 / 2; i++) ((short *)p1)[i] = (short)((i & 64) ? 3000 : -3000);
    CHECK(135, CALL4(buf, 19, p1, n1, 0, 0) == 0);
    ev = CreateEventA(0, 0, 0, 0);
    pn.dwOffset = 4000;
    pn.hEventNotify = ev;
    CHECK(136, CALL2(buf, 0, &IID_IDirectSoundNotify, &notify) == 0 && notify &&
                   CALL2(notify, 3, 1, &pn) == 0);
    CHECK(137, CALL3(buf, 12, 0, 0, 1) == 0); /* DSBPLAY_LOOPING */
    CHECK(138, WaitForSingleObject(ev, 3000) == 0);
    CHECK(139, CALL2(buf, 4, &play, &wr) == 0 && play < d.dwBufferBytes);
    CHECK(140, CALL0(buf, 18) == 0);

    /* Tampon 3D mono : SetPosition passe trois float par la pile x86. */
    wf.nChannels = 1;
    wf.nBlockAlign = 2;
    wf.nAvgBytesPerSec = 44100 * 2;
    d.dwFlags = 0x10 | 0x80; /* CTRL3D | CTRLVOLUME */
    d.dwBufferBytes = 8820;
    CHECK(141, CALL3(ds, 3, &d, &buf3, 0) == 0 && buf3);
    CHECK(142, CALL2(buf3, 0, &IID_IDirectSound3DBuffer, &b3) == 0 && b3);
    CHECK(143, ((SETPOS3D)VT(b3)[19])(b3, 1.5f, -2.0f, 3.25f, 0) == 0 && CALL1(b3, 10, v) == 0 &&
                   v[0] == 1.5f && v[1] == -2.0f && v[2] == 3.25f);

    CALL0(b3, 2);
    CALL0(buf3, 2);
    CALL0(notify, 2);
    CALL0(buf, 2);
    CHECK(144, CALL0(ds, 2) == 0);
    return 0;
}

/* DirectShow (#835) : graphe de lecture de quartz.dll par CoCreateInstance. */
__declspec(dllimport) HRESULT WINAPI CoInitialize(void *);
__declspec(dllimport) HRESULT WINAPI CoCreateInstance(const GUID *, void *, DWORD, const GUID *,
                                                      void **);
static const GUID CLSID_FilterGraph = {
    0xe436ebb3, 0x524f, 0x11ce, {0x9f, 0x53, 0x00, 0x20, 0xaf, 0x0b, 0xa7, 0x70}};
#define DS_IID(n, d1) static const GUID n = {d1, 0x0ad4, 0x11ce, {0xb0, 0x3a, 0, 0x20, 0xaf, 0x0b, 0xa7, 0x70}}
DS_IID(IID_IGraphBuilder, 0x56a868a9);
DS_IID(IID_IMediaControl, 0x56a868b1);
DS_IID(IID_IMediaEventEx, 0x56a868c0);
DS_IID(IID_IBasicAudio, 0x56a868b3);
DS_IID(IID_IVideoWindow, 0x56a868b4);

static int dshow_tests(void) {
    void *gb = 0, *mc = 0, *me = 0, *ba = 0, *vw = 0;
    long st = -1, vol = 0, code = 0;
    UINT_PTR h = 0, p1 = 0, p2 = 0;
    CoInitialize(0);
    CHECK(150, CoCreateInstance(&CLSID_FilterGraph, 0, 1, &IID_IGraphBuilder, &gb) == 0 && gb);
    CHECK(151, CALL2(gb, 0, &IID_IMediaControl, &mc) == 0 && mc);
    CHECK(152, CALL2(gb, 0, &IID_IMediaEventEx, &me) == 0 && me);
    CHECK(153, CALL2(gb, 0, &IID_IBasicAudio, &ba) == 0 && ba);
    CHECK(154, CALL2(gb, 0, &IID_IVideoWindow, &vw) == 0 && vw);
    /* Fichier absent : RenderFile échoue, le graphe reste arrêté. */
    CHECK(155, CALL2(gb, 13, L"C:\\winlux-absent.avi", 0) < 0);
    CHECK(156, CALL2(mc, 10, 0, &st) == 0 && st == 0);
    CHECK(157, CALL1(ba, 7, -1000) == 0 && CALL1(ba, 8, &vol) == 0 && vol == -1000);
    CHECK(158, CALL1(me, 7, &h) == 0 && h != 0);
    /* Aucun événement en attente : E_ABORT. */
    CHECK(159, CALL4(me, 8, &code, &p1, &p2, 0) != 0);
    CALL0(vw, 2);
    CALL0(ba, 2);
    CALL0(me, 2);
    CALL0(mc, 2);
    CHECK(160, CALL0(gb, 2) == 0);
    return 0;
}

/* DirectPlay (#836) : hôte et client dans le même programme, sur 127.0.0.1. */
__declspec(dllimport) HRESULT WINAPI DirectPlayCreate(const GUID *, void **, void *);
__declspec(dllimport) HRESULT WINAPI DirectPlayLobbyCreateA(const GUID *, void **, void *, void *,
                                                           DWORD);
typedef struct {
    DWORD dwSize, dwFlags;
    GUID guidInstance, guidApplication;
    DWORD dwMaxPlayers, dwCurrentPlayers;
    char *lpszSessionNameA, *lpszPasswordA;
    DWORD dwReserved1, dwReserved2, dwUser1, dwUser2, dwUser3, dwUser4;
} DPSESSIONDESC2;
typedef struct { DWORD dwSize, dwFlags; char *lpszShortNameA, *lpszLongNameA; } DPNAME;
static const GUID DPSPGUID_TCPIP = {
    0x36e95ee0, 0x8577, 0x11cf, {0x96, 0x0c, 0x00, 0x80, 0xc7, 0x53, 0x4e, 0x82}};
static const GUID DPAID_INet = {
    0xc4a54da0, 0xe0af, 0x11cf, {0x9c, 0x4e, 0x00, 0xa0, 0xc9, 0x05, 0x42, 0x5e}};
static const GUID IID_IDirectPlay4A = {
    0x0ab1c531, 0x4745, 0x11d1, {0xa7, 0xa1, 0x00, 0x00, 0xf8, 0x03, 0xab, 0xfc}};
static const GUID IID_IDirectPlayLobby3A = {
    0x2db72491, 0x652c, 0x11d1, {0xa7, 0xa8, 0x00, 0x00, 0xf8, 0x03, 0xab, 0xfc}};
static const GUID APP_JEUX = {0x57494e4c, 0x5558, 0x4a45, {0x55, 0x58, 1, 2, 3, 4, 5, 6}};
static int sessions, connections, players;
static GUID found_instance;
static DWORD found_current;
static BOOL WINAPI enum_session(const DPSESSIONDESC2 *sd, DWORD *timeout, DWORD flags, void *ctx) {
    const char *n;
    if (flags & 1) return 0; /* DPESC_TIMEDOUT */
    n = sd->lpszSessionNameA;
    if (ctx == (void *)0x5150 && *timeout == 1500 && n[0] == 'W' && n[5] == 'x') {
        found_instance = sd->guidInstance;
        found_current = sd->dwCurrentPlayers;
        sessions++;
    }
    return 1;
}
static BOOL WINAPI enum_connection(const GUID *sp, void *conn, DWORD size, const DPNAME *name,
                                   DWORD flags, void *ctx) {
    if (sp->d1 == DPSPGUID_TCPIP.d1 && size > 40 && name->lpszShortNameA[0] && flags == 1 &&
        ctx == (void *)7)
        connections++;
    return 1;
}
static BOOL WINAPI enum_player(DWORD id, DWORD type, const DPNAME *name, DWORD flags, void *ctx) {
    if (type == 1) players++;
    return 1;
}

static int dplay_tests(void) {
    void *v1 = 0, *host = 0, *client = 0, *lobby = 0;
    DPSESSIONDESC2 sd;
    DPNAME nm;
    DWORD hp = 0, cp = 0, from, to, size, i;
    unsigned char addr[128], buf[256];
    HANDLE hev = CreateEventA(0, 0, 0, 0), cev = CreateEventA(0, 0, 0, 0);
    int got_create = 0, got_msg = 0;

    CHECK(170, DirectPlayCreate(&DPSPGUID_TCPIP, &v1, 0) == 0 && v1);
    CHECK(171, CALL2(v1, 0, &IID_IDirectPlay4A, &host) == 0 && host);
    CALL0(v1, 2);
    zero(&sd, sizeof sd);
    sd.dwSize = sizeof sd;
    sd.guidApplication = APP_JEUX;
    sd.dwMaxPlayers = 4;
    sd.lpszSessionNameA = "Winlux";
    CHECK(172, CALL2(host, 24, &sd, 2) == 0); /* Open(DPOPEN_CREATE) */
    zero(&nm, sizeof nm);
    nm.dwSize = sizeof nm;
    nm.lpszShortNameA = "Hote";
    CHECK(173, CALL6(host, 6, &hp, &nm, hev, 0, 0, 0) == 0 && hp > 1);

    /* Client : objet non initialisé, connexion TCP/IP par une adresse du lobby. */
    CHECK(174, DirectPlayLobbyCreateA(0, &v1, 0, 0, 0) == 0 && v1);
    CHECK(175, CALL2(v1, 0, &IID_IDirectPlayLobby3A, &lobby) == 0 && lobby);
    CALL0(v1, 2);
    size = sizeof addr;
    CHECK(176, CALL6(lobby, 4, &DPSPGUID_TCPIP, &DPAID_INet, "127.0.0.1", 10, addr, &size) == 0 &&
                   size == 90);
    CHECK(177, DirectPlayCreate(0, &v1, 0) == 0 && CALL2(v1, 0, &IID_IDirectPlay4A, &client) == 0);
    CALL0(v1, 2);
    CHECK(178, CALL4(client, 35, 0, enum_connection, 7, 0) == 0 && connections == 1);
    CHECK(179, CALL2(client, 38, addr, 0) == 0);
    zero(&sd, sizeof sd);
    sd.dwSize = sizeof sd;
    sd.guidApplication = APP_JEUX;
    CHECK(180, CALL5(client, 13, &sd, 1500, enum_session, 0x5150, 0) == 0 && sessions == 1 &&
                   found_current == 1);
    sd.guidInstance = found_instance;
    CHECK(181, CALL2(client, 24, &sd, 1) == 0); /* Open(DPOPEN_JOIN) */
    nm.lpszShortNameA = "Client";
    CHECK(182, CALL6(client, 6, &cp, &nm, cev, 0, 0, 0) == 0 && cp > 1 && cp != hp);
    CHECK(183, CALL5(client, 26, cp, hp, 1, "bonjour", 8) == 0);

    /* Hôte : création du joueur distant (message système) puis le message. */
    for (i = 0; i < 20 && !(got_create && got_msg); i++) {
        WaitForSingleObject(hev, 200);
        for (;;) {
            from = to = 0;
            size = sizeof buf;
            if (CALL5(host, 25, &from, &to, 0, buf, &size) != 0) break;
            if (from == 0 && *(DWORD *)buf == 3 && ((DWORD *)buf)[2] == cp) {
                const DPNAME *n = (const DPNAME *)(buf + (sizeof(void *) == 4 ? 24 : 32));
                if (n->lpszShortNameA[0] == 'C') got_create = 1;
            } else if (from == cp && to == hp && size == 8 && buf[0] == 'b' && buf[6] == 'r') {
                got_msg = 1;
            }
        }
    }
    CHECK(184, got_create);
    CHECK(185, got_msg);
    /* Réponse de l'hôte : l'événement du joueur client est signalé. */
    CHECK(186, CALL5(host, 26, hp, cp, 1, "salut", 6) == 0);
    CHECK(187, WaitForSingleObject(cev, 3000) == 0);
    from = 0;
    to = cp;
    size = sizeof buf;
    CHECK(188, CALL5(client, 25, &from, &to, 2, buf, &size) == 0 && from == hp && size == 6 &&
                   buf[0] == 's');
    size = sizeof buf;
    CHECK(189, CALL2(client, 22, buf, &size) == 0 &&
                   ((DPSESSIONDESC2 *)buf)->dwCurrentPlayers == 2 &&
                   ((DPSESSIONDESC2 *)buf)->lpszSessionNameA[0] == 'W');
    CHECK(190, CALL4(client, 12, 0, enum_player, 0, 0) == 0 && players == 2);
    CHECK(191, CALL0(client, 4) == 0 && CALL0(host, 4) == 0); /* Close */
    CALL0(client, 2);
    CALL0(host, 2);
    CHECK(192, CALL0(lobby, 2) == 0);
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
    d3dx_tests(hwnd);
    out("Direct3D 9 et D3DX9 : 26 verifications passees\n");
    dsound_tests(hwnd);
    out("DirectSound : 15 verifications passees\n");
    dshow_tests();
    out("DirectShow : 11 verifications passees\n");
    dplay_tests();
    out("DirectPlay : 23 verifications passees\n");
    midi_tests();
    out("MIDI : 15 verifications passees\n");
    timer_wave_tests();
    out("timeSetEvent et waveOut : 9 verifications passees\n");
    out("Jeux anciens : tout est correct\n");
    ExitProcess(0);
}

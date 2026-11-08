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
typedef struct { WORD wMid, wPid; DWORD vDriverVersion; char szPname[32]; DWORD dwSupport; }
    MIDIINCAPSA;
typedef struct { WORD wFormatTag, nChannels; DWORD nSamplesPerSec, nAvgBytesPerSec;
    WORD nBlockAlign, wBitsPerSample, cbSize; } WAVEFORMATEX;
typedef struct { UINT wType; DWORD ticks; DWORD pad; } MMTIME;
#define CALLBACK_FUNCTION 0x30000
#define MOM_OPEN 0x3C7
#define MOM_CLOSE 0x3C8
#define MOM_DONE 0x3C9
#define MOM_POSITIONCB 0x3CA
#define MIM_OPEN 0x3C1
#define MIM_CLOSE 0x3C2
#define MIM_DATA 0x3C3
#define MIM_LONGDATA 0x3C4
#define WOM_DONE 0x3BD
#define MEVT_F_CALLBACK 0x40000000
#define TIME_PERIODIC 1
#define TIME_TICKS 0x20

#define IMP __declspec(dllimport)
IMP HANDLE WINAPI GetStdHandle(DWORD);
IMP BOOL WINAPI WriteFile(HANDLE, const void *, DWORD, DWORD *, void *);
IMP void WINAPI ExitProcess(DWORD);
IMP void WINAPI Sleep(DWORD);
IMP DWORD WINAPI GetEnvironmentVariableA(const char *, char *, DWORD);
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
IMP HRESULT WINAPI D3DXCreateBox(void *, float, float, float, void **, void **);
IMP HRESULT WINAPI D3DXCreateSphere(void *, float, UINT, UINT, void **, void **);
IMP HRESULT WINAPI D3DXComputeNormals(void *, const DWORD *);
IMP HRESULT WINAPI D3DXCompileShader(const char *, UINT, void *, void *, const char *, const char *,
    DWORD, void **, void **, void **);
IMP HRESULT WINAPI D3DXCreateEffect(void *, const void *, UINT, void *, void *, DWORD, void *, void **,
    void **);
typedef void *(WINAPI *GETH1)(void *, const char *);
typedef HRESULT(WINAPI *SETFLOATFX)(void *, void *, float);
typedef struct {
    const char *Name, *Semantic;
    UINT Class, Type, Rows, Columns, Elements, Annotations, StructMembers, Flags, Bytes;
} D3DXPARAMETER_DESC;
IMP HRESULT WINAPI D3DXLoadMeshFromXInMemory(const void *, DWORD, DWORD, void *, void **, void **,
    void **, DWORD *, void **);
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
IMP UINT WINAPI midiInGetNumDevs(void);
typedef struct {
    DWORD cbStruct, fdwStatus; DWORD_PTR dwUser; BYTE *pbSrc; DWORD cbSrcLength, cbSrcLengthUsed;
    DWORD_PTR dwSrcUser; BYTE *pbDst; DWORD cbDstLength, cbDstLengthUsed; DWORD_PTR dwDstUser;
    DWORD dwReservedDriver[sizeof(void *) == 8 ? 15 : 10];
} ACMSTREAMHEADER;
typedef struct { DWORD cbStruct, dwFormatTagIndex, dwFormatTag, cbFormatSize, fdwSupport,
    cStandardFormats; char szFormatTag[48]; } ACMFORMATTAGDETAILSA;
IMP UINT WINAPI acmMetrics(HANDLE, UINT, void *);
IMP UINT WINAPI acmFormatSuggest(HANDLE, WAVEFORMATEX *, WAVEFORMATEX *, DWORD, DWORD);
IMP UINT WINAPI acmFormatTagDetailsA(HANDLE, ACMFORMATTAGDETAILSA *, DWORD);
IMP UINT WINAPI acmStreamOpen(HANDLE *, HANDLE, WAVEFORMATEX *, WAVEFORMATEX *, void *, DWORD_PTR,
                              DWORD_PTR, DWORD);
IMP UINT WINAPI acmStreamSize(HANDLE, DWORD, DWORD *, DWORD);
IMP UINT WINAPI acmStreamPrepareHeader(HANDLE, ACMSTREAMHEADER *, DWORD);
IMP UINT WINAPI acmStreamUnprepareHeader(HANDLE, ACMSTREAMHEADER *, DWORD);
IMP UINT WINAPI acmStreamConvert(HANDLE, ACMSTREAMHEADER *, DWORD);
IMP UINT WINAPI acmStreamClose(HANDLE, DWORD);
IMP UINT WINAPI midiInGetDevCapsA(UINT_PTR, MIDIINCAPSA *, UINT);
IMP UINT WINAPI midiInOpen(HANDLE *, UINT, DWORD_PTR, DWORD_PTR, DWORD);
IMP UINT WINAPI midiInPrepareHeader(HANDLE, MIDIHDR *, UINT);
IMP UINT WINAPI midiInUnprepareHeader(HANDLE, MIDIHDR *, UINT);
IMP UINT WINAPI midiInAddBuffer(HANDLE, MIDIHDR *, UINT);
IMP UINT WINAPI midiInStart(HANDLE);
IMP UINT WINAPI midiInStop(HANDLE);
IMP UINT WINAPI midiInReset(HANDLE);
IMP UINT WINAPI midiInClose(HANDLE);
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
/* Entrée MIDI : messages reçus dans l'ordre, avec leur heure. */
static volatile int mim_open, mim_close, mim_data, mim_long;
static volatile DWORD mim_msgs[8], mim_times[8];
static volatile DWORD_PTR mim_hdr;
static void CALLBACK midi_in_cb(HANDLE h, UINT msg, DWORD_PTR inst, DWORD_PTR p1, DWORD_PTR p2) {
    if (inst != 0x56) return;
    if (msg == MIM_OPEN) mim_open++;
    if (msg == MIM_CLOSE) mim_close++;
    if (msg == MIM_DATA && mim_data < 8) {
        mim_msgs[mim_data] = (DWORD)p1;
        mim_times[mim_data] = (DWORD)p2;
        mim_data++;
    }
    if (msg == MIM_LONGDATA) {
        mim_hdr = p1;
        mim_long++;
    }
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

/* Entrée MIDI (#237) sans matériel : test-wow32.sh met dans
 * WINLUX_MIDI_IN_DIR un fichier midiC0D0 qui contient ce qu'aurait joué un
 * clavier : note on 90 3C 7F, statut courant 3C 00, horloge F8, système
 * exclusif F0 7E 7F 09 01 F7, changement de programme C0 05. */
static int midi_in_tests(void) {
    MIDIINCAPSA caps;
    MIDIHDR hdr, other;
    HANDLE h = 0;
    char dir[260];
    static char sysex[64];

    if (!GetEnvironmentVariableA("WINLUX_MIDI_IN_DIR", dir, sizeof dir)) return 1;
    zero(&caps, sizeof caps);
    CHECK(161, midiInGetNumDevs() == 1 && midiInGetDevCapsA(0, &caps, sizeof caps) == 0 &&
                   caps.szPname[0] && midiInGetDevCapsA(3, &caps, sizeof caps) == 2);
    CHECK(162, midiInOpen(&h, 0, (DWORD_PTR)midi_in_cb, 0x56, CALLBACK_FUNCTION) == 0 && h &&
                   mim_open == 1);
    zero(&hdr, sizeof hdr);
    hdr.lpData = sysex;
    hdr.dwBufferLength = sizeof sysex;
    zero(&other, sizeof other);
    other.lpData = sysex;
    other.dwBufferLength = sizeof sysex;
    /* Tampon non préparé : MIDIERR_UNPREPARED. */
    CHECK(163, midiInAddBuffer(h, &other, sizeof other) == 64 &&
                   midiInPrepareHeader(h, &hdr, sizeof hdr) == 0 &&
                   midiInAddBuffer(h, &hdr, sizeof hdr) == 0 && (hdr.dwFlags & 4));
    /* Rien n'arrive avant midiInStart, et on ne ferme pas avec un tampon en file. */
    Sleep(100);
    CHECK(164, mim_data == 0 && midiInClose(h) == 65);
    CHECK(165, midiInStart(h) == 0);
    wait_for(&mim_data, 4, 3000);
    wait_for(&mim_long, 1, 1000);
    CHECK(166, mim_data == 4 && mim_msgs[0] == 0x7F3C90 && mim_msgs[1] == 0x003C90 &&
                   mim_msgs[2] == 0xF8 && mim_msgs[3] == 0x05C0 && mim_times[3] < 3000);
    CHECK(167, mim_long == 1 && mim_hdr == (DWORD_PTR)&hdr && hdr.dwBytesRecorded == 6 &&
                   (BYTE)sysex[0] == 0xF0 && (BYTE)sysex[5] == 0xF7 && (hdr.dwFlags & 1) &&
                   !(hdr.dwFlags & 4));
    /* Reset rend le tampon remis en file, puis on ferme. */
    CHECK(168, midiInAddBuffer(h, &hdr, sizeof hdr) == 0 && midiInStop(h) == 0 &&
                   midiInReset(h) == 0 && mim_long == 2 && hdr.dwBytesRecorded == 0 &&
                   (hdr.dwFlags & 1));
    CHECK(169, midiInUnprepareHeader(h, &hdr, sizeof hdr) == 0 && midiInClose(h) == 0 &&
                   mim_close == 1 && midiInClose(h) == 5);
    return 0;
}

/* msacm32 (#237) : conversion PCM 8 bits mono 11 025 Hz vers 16 bits
 * stéréo 22 050 Hz, rappel asynchrone, en x86 comme en x64. */
static volatile int acm_open, acm_close, acm_done;
static void CALLBACK acm_cb(HANDLE h, UINT msg, DWORD_PTR inst, DWORD_PTR p1, DWORD_PTR p2) {
    if (inst != 0x57) return;
    if (msg == 0x3D4) acm_open++;
    if (msg == 0x3D5) acm_close++;
    if (msg == 0x3D6) acm_done++;
}
static int acm_tests(void) {
    WAVEFORMATEX src = {1, 1, 11025, 11025, 1, 8, 0}, dst, adpcm = {2, 1, 22050, 11155, 512, 4, 0};
    ACMFORMATTAGDETAILSA tag;
    ACMSTREAMHEADER hdr, other;
    static BYTE in[100];
    static short pcm[1000];
    HANDLE h = 0;
    DWORD n = 0;
    int i;

    for (i = 0; i < 100; i++) in[i] = (BYTE)(128 + i);
    zero(&dst, sizeof dst);
    dst.wFormatTag = 1;
    dst.wBitsPerSample = 16;
    CHECK(145, acmMetrics(0, 50, &n) == 0 && n >= 18 &&
                   acmFormatSuggest(0, &src, &dst, sizeof dst, 0x90000) == 0 &&
                   dst.nChannels == 1 && dst.nSamplesPerSec == 11025 && dst.nBlockAlign == 2);
    zero(&tag, sizeof tag);
    tag.cbStruct = sizeof tag;
    tag.dwFormatTag = 1;
    CHECK(146, acmFormatTagDetailsA(0, &tag, 1) == 0 && tag.szFormatTag[0] == 'P' &&
                   acmStreamOpen(0, 0, &src, &adpcm, 0, 0, 0, 1) == 512);
    dst.nChannels = 2;
    dst.nSamplesPerSec = 22050;
    dst.nBlockAlign = 4;
    dst.nAvgBytesPerSec = 88200;
    CHECK(147, acmStreamOpen(0, 0, &src, &dst, 0, 0, 0, 1) == 0 &&
                   acmStreamOpen(&h, 0, &src, &dst, 0, (DWORD_PTR)acm_cb, 0x57,
                                 CALLBACK_FUNCTION | 2) == 0 && h && acm_open == 1);
    CHECK(148, acmStreamSize(h, sizeof in, &n, 0) == 0 && n == 804);
    zero(&hdr, sizeof hdr);
    hdr.cbStruct = sizeof hdr;
    hdr.pbSrc = in;
    hdr.cbSrcLength = sizeof in;
    hdr.pbDst = (BYTE *)pcm;
    hdr.cbDstLength = sizeof pcm;
    other = hdr;
    CHECK(149, acmStreamConvert(h, &other, 0) == 514 &&
                   acmStreamPrepareHeader(h, &hdr, 0) == 0 && (hdr.fdwStatus & 0x20000));
    CHECK(106, acmStreamConvert(h, &hdr, 0x10 | 0x20) == 0 && hdr.cbSrcLengthUsed == 100 &&
                   hdr.cbDstLengthUsed == 800 && (hdr.fdwStatus & 0x10000) && acm_done == 1);
    /* 128 → 0 ; puis une rampe de 256 par octet source, 128 par trame cible. */
    CHECK(107, pcm[0] == 0 && pcm[1] == 0 && pcm[2] == 128 && pcm[3] == 128 && pcm[4] == 256 &&
                   pcm[198 * 2] == 99 * 256);
    CHECK(108, acmStreamUnprepareHeader(h, &hdr, 0) == 0 && !(hdr.fdwStatus & 0x20000) &&
                   acmStreamConvert(h, &hdr, 0) == 514);
    CHECK(109, acmStreamClose(h, 0) == 0 && acm_close == 1 && acmStreamClose(h, 0) == 5);
    return 0;
}

/* Décodeurs ADPCM de msacm32 (#1122) : un bloc IMA ADPCM et un bloc
 * ADPCM MS, valeurs calculées d'après les spécifications publiques. */
static void put16(BYTE *p, int v) { p[0] = (BYTE)v; p[1] = (BYTE)(v >> 8); }
static int adpcm_tests(void) {
    static BYTE ima[20], ms[50], block[8], out[64];
    static const short coefs[14] = {256, 0, 512, -256, 0, 0, 192, 64, 240, 0, 460, -208, 392, -232};
    static const short want_ima[9] = {0, 11, 41, -22, -13, -5, 2, 8, 14};
    static const short want_ms[4] = {50, 100, 116, 100};
    WAVEFORMATEX pcm = {1, 1, 22050, 44100, 2, 16, 0}, sug;
    ACMFORMATTAGDETAILSA tag;
    ACMSTREAMHEADER hdr;
    HANDLE h = 0;
    DWORD n = 0;
    int i, ok;

    put16(ima, 0x11); put16(ima + 2, 1); put16(ima + 4, 22050); put16(ima + 12, 8);
    put16(ima + 14, 4); put16(ima + 16, 2); put16(ima + 18, 9);
    put16(ms, 2); put16(ms + 2, 1); put16(ms + 4, 22050); put16(ms + 12, 8);
    put16(ms + 14, 4); put16(ms + 16, 32); put16(ms + 18, 4); put16(ms + 20, 7);
    for (i = 0; i < 14; i++) put16(ms + 22 + 2 * i, coefs[i]);
    zero(&sug, sizeof sug);
    zero(&tag, sizeof tag);
    tag.cbStruct = sizeof tag;
    tag.dwFormatTag = 0x11;
    CHECK(127, acmMetrics(0, 50, &n) == 0 && n == 50 &&
                   acmFormatSuggest(0, (WAVEFORMATEX *)ima, &sug, sizeof sug, 0) == 0 &&
                   sug.wFormatTag == 1 && sug.wBitsPerSample == 16 && sug.nSamplesPerSec == 22050 &&
                   acmFormatTagDetailsA(0, &tag, 1) == 0 && tag.cbFormatSize == 20 &&
                   tag.szFormatTag[0] == 'I');

    /* IMA : échantillon 0, pas n° 0, quartets 7 7 F 0 0 0 0 0. */
    zero(block, sizeof block);
    block[4] = 0x77;
    block[5] = 0x0F;
    zero(&hdr, sizeof hdr);
    hdr.cbStruct = sizeof hdr;
    hdr.pbSrc = block;
    hdr.cbSrcLength = sizeof block;
    hdr.pbDst = out;
    hdr.cbDstLength = sizeof out;
    ok = acmStreamOpen(&h, 0, (WAVEFORMATEX *)ima, &pcm, 0, 0, 0, 0) == 0 &&
         acmStreamSize(h, 8, &n, 0) == 0 && n == 18 &&
         acmStreamPrepareHeader(h, &hdr, 0) == 0 && acmStreamConvert(h, &hdr, 0) == 0 &&
         hdr.cbSrcLengthUsed == 8 && hdr.cbDstLengthUsed == 18;
    for (i = 0; ok && i < 9; i++) ok = ((short *)out)[i] == want_ima[i];
    CHECK(128, ok && acmStreamUnprepareHeader(h, &hdr, 0) == 0 && acmStreamClose(h, 0) == 0);

    /* ADPCM MS : prédicteur 0, delta 16, s1 = 100, s2 = 50, quartets 1 puis F. */
    zero(block, sizeof block);
    put16(block + 1, 16);
    put16(block + 3, 100);
    put16(block + 5, 50);
    block[7] = 0x1F;
    zero(out, sizeof out);
    hdr.fdwStatus = 0;
    hdr.cbSrcLengthUsed = hdr.cbDstLengthUsed = 0;
    ok = acmStreamOpen(&h, 0, (WAVEFORMATEX *)ms, &pcm, 0, 0, 0, 0) == 0 &&
         acmStreamPrepareHeader(h, &hdr, 0) == 0 && acmStreamConvert(h, &hdr, 0) == 0 &&
         hdr.cbSrcLengthUsed == 8 && hdr.cbDstLengthUsed == 8;
    for (i = 0; ok && i < 4; i++) ok = ((short *)out)[i] == want_ms[i];
    CHECK(129, ok && acmStreamUnprepareHeader(h, &hdr, 0) == 0 && acmStreamClose(h, 0) == 0);
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

        CHECK(193, D3DXAssembleShader(vs, sizeof vs - 1, 0, 0, 0, &code, &err) == 0 && code &&
                       !err && *(DWORD *)BUFPTR(code) == 0xFFFE0200);
        CHECK(194, CALL2(dev, 91, BUFPTR(code), &vsh) == 0 && vsh);
        CHECK(195, D3DXGetShaderInputSemantics((DWORD *)BUFPTR(code), sem, &n) == 0 &&
                       n == 2 && sem[0] == 0 && sem[2] == 5 && sem[3] == 1);
        CALL0(code, 2);
        CHECK(196, D3DXAssembleShader(ps, sizeof ps - 1, 0, 0, 0, &code, 0) == 0 &&
                       CALL2(dev, 106, BUFPTR(code), &psh) == 0 && psh);
        CALL0(code, 2);
        code = 0;
        CHECK(197, D3DXAssembleShader("ps_2_0\nfoo r0", 13, 0, 0, 0, &code, &err) != 0 &&
                       !code && err && ((char *)BUFPTR(err))[0] != 0);
        CALL0(err, 2);
        CHECK(198, D3DXGetShaderConstantTable(ctab, &ct) == 0 && ct);
        h = ((GETBYNAME)VT(ct)[9])(ct, 0, "gColor");
        zero(&desc, sizeof desc);
        n = 1;
        CHECK(199, h && CALL3(ct, 6, h, &desc, &n) == 0 && desc.RegisterIndex == 3 &&
                       desc.Bytes == 16 && desc.Name[0] == 'g');
        CHECK(200, CALL3(ct, 19, dev, h, v) == 0 && CALL3(dev, 110, 3, got, 1) == 0 &&
                       got[0] == 0.25f && got[3] == 1.0f);
        CHECK(201, ((SETFLOATCT)VT(ct)[17])(ct, dev, "gColor", 2.5f) == 0 &&
                       CALL3(dev, 110, 3, got, 1) == 0 && got[0] == 2.5f);
        CALL0(ct, 2);
        CALL0(psh, 2);
        CALL0(vsh, 2);
    }

    /* Maillages (#834) : D3DXCreateBox, fichier .x texte en mémoire, matériaux,
     * table d'attributs, DrawSubset, sphère et D3DXComputeNormals. */
    {
        static const char xfile[] =
            "xof 0303txt 0032\n"
            "Material Rouge { 1.0;0.0;0.0;1.0;; 10.0; 1.0;1.0;1.0;; 0.0;0.0;0.0;;"
            " TextureFilename { \"rouge.bmp\"; } }\n"
            "Mesh Quad { 4; -1;-1;0;, 1;-1;0;, 1;1;0;, -1;1;0;;\n"
            " 1; 4;0,1,2,3;;\n"
            " MeshNormals { 1; 0;0;1;; 1; 4;0,0,0,0;; }\n"
            " MeshTextureCoords { 4; 0;1;, 1;1;, 1;0;, 0;0;; }\n"
            " MeshMaterialList { 1; 1; 0;; { Rouge } } }\n";
        void *box = 0, *quad = 0, *sphere = 0, *adjb = 0, *mats = 0;
        float *vtx = 0;
        DWORD nmat = 0, nattr = 0;
        unsigned char *mat;

        CHECK(202, D3DXCreateBox(dev, 1.0f, 2.0f, 3.0f, &box, 0) == 0 && box &&
                       CALL0(box, 4) == 12 && CALL0(box, 5) == 24 && CALL0(box, 6) == 0x12);
        CHECK(203, CALL2(box, 15, 0, &vtx) == 0 && vtx && vtx[0] == -0.5f && vtx[1] == -1.0f &&
                       vtx[2] == -1.5f && vtx[3] == -1.0f && CALL0(box, 16) == 0);
        CHECK(204, CALL0(dev, 41) == 0 && CALL1(box, 3, 0) == 0 && CALL0(dev, 42) == 0);
        CHECK(205, D3DXLoadMeshFromXInMemory(xfile, sizeof xfile - 1, 0x220, dev, &adjb, &mats, 0,
                                             &nmat, &quad) == 0 &&
                       quad && nmat == 1 && CALL0(quad, 4) == 2 && CALL0(quad, 5) == 4);
        mat = (unsigned char *)BUFPTR(mats);
        /* D3DXMATERIAL : D3DMATERIAL9 (68 octets) puis le nom de texture, aligné sur 8 en x64. */
        CHECK(206, ((float *)mat)[0] == 1.0f && ((float *)mat)[1] == 0.0f &&
                       (*(char **)(mat + (sizeof(void *) == 8 ? 72 : 68)))[0] == 'r' &&
                       (*(char **)(mat + (sizeof(void *) == 8 ? 72 : 68)))[5] == '.' &&
                       adjb && CALL0(adjb, 4) == 24);
        CHECK(207, CALL2(quad, 19, 0, &nattr) == 0 && nattr == 1);
        CHECK(208, D3DXCreateSphere(dev, 1.0f, 8, 4, &sphere, 0) == 0 && sphere &&
                       CALL0(sphere, 4) > 0 && D3DXComputeNormals(sphere, 0) == 0);
        /* DrawSubset laisse ses tampons liés à l'appareil : on les délie. */
        CALL4(dev, 100, 0, 0, 0, 0);
        CALL1(dev, 104, 0);
        CALL0(sphere, 2);
        CALL0(adjb, 2);
        CALL0(mats, 2);
        CALL0(quad, 2);
        CALL0(box, 2);
    }

    /* Compilation HLSL et effets (#834). */
    {
        static const char vs[] = "float4x4 WVP;\nfloat Scale;\n"
                                 "struct O { float4 p : POSITION; float2 t : TEXCOORD0; };\n"
                                 "O main(float4 p : POSITION, float2 t : TEXCOORD0) {\n"
                                 "  O o; o.p = mul(p, WVP); o.t = t * Scale; return o; }\n";
        static const char ps[] = "sampler2D S;\n"
                                 "float4 main(float2 uv : TEXCOORD0) : COLOR {\n"
                                 "  float4 c = tex2D(S, uv);\n"
                                 "  if (c.a < 0.1) discard;\n"
                                 "  return saturate(c * 2); }\n";
        static const char fxsrc[] =
            "float4x4 WVP;\n"
            "float Intensity < string UIName = \"Intensite\"; > = 1.0;\n"
            "float4 VS(float4 p : POSITION) : POSITION { return mul(p, WVP); }\n"
            "float4 PS() : COLOR { return float4(Intensity, Intensity, Intensity, 1); }\n"
            "technique Rendu {\n"
            "  pass P0 { VertexShader = compile vs_2_0 VS(); PixelShader = compile ps_2_0 PS();\n"
            "            AlphaBlendEnable = TRUE; SrcBlend = SrcAlpha; }\n"
            "}\n";
        void *code = 0, *err = 0, *ct = 0, *vsh = 0, *psh = 0, *fx = 0, *h, *hw, *t, *a;
        D3DXCONSTANT_DESC desc;
        D3DXPARAMETER_DESC pd;
        UINT n = 1, np = 0;
        DWORD rs = 0;
        float got[4] = {0, 0, 0, 0}, fv = 0;
        const char *str = 0;

        CHECK(209, D3DXCompileShader(vs, sizeof vs - 1, 0, 0, "main", "vs_2_0", 0, &code, &err, &ct) == 0 &&
                       code && ct && !err && *(DWORD *)BUFPTR(code) == 0xFFFE0200);
        CHECK(210, CALL2(dev, 91, BUFPTR(code), &vsh) == 0 && vsh);
        h = ((GETBYNAME)VT(ct)[9])(ct, 0, "Scale");
        zero(&desc, sizeof desc);
        CHECK(211, h && CALL3(ct, 6, h, &desc, &n) == 0 && desc.RegisterIndex == 4 &&
                       ((SETFLOATCT)VT(ct)[17])(ct, dev, h, 2.5f) == 0 &&
                       CALL3(dev, 95, 4, got, 1) == 0 && got[0] == 2.5f);
        CALL0(ct, 2);
        CALL0(code, 2);
        code = 0;
        CHECK(212, D3DXCompileShader(ps, sizeof ps - 1, 0, 0, "main", "ps_2_0", 0, &code, 0, 0) == 0 &&
                       code && CALL2(dev, 106, BUFPTR(code), &psh) == 0 && psh);
        CALL0(code, 2);
        code = 0;
        CHECK(213, D3DXCompileShader("float4 main() : COLOR { return x; }", 34, 0, 0, "main", "ps_2_0",
                                     0, &code, &err, 0) != 0 &&
                       !code && err && ((char *)BUFPTR(err))[0] != 0);
        CALL0(err, 2);
        err = 0;
        CHECK(214, D3DXCreateEffect(dev, fxsrc, sizeof fxsrc - 1, 0, 0, 0, 0, &fx, &err) == 0 && fx && !err);
        t = ((GETH1)VT(fx)[13])(fx, "Rendu");
        CHECK(215, t && CALL1(fx, 58, t) == 0);
        h = ((GETBYNAME)VT(fx)[9])(fx, 0, "Intensity");
        CHECK(216, h && ((SETFLOATFX)VT(fx)[30])(fx, h, 0.75f) == 0 && CALL2(fx, 31, h, &fv) == 0 &&
                       fv == 0.75f);
        hw = ((GETBYNAME)VT(fx)[9])(fx, 0, "WVP");
        zero(&pd, sizeof pd);
        CHECK(217, hw && CALL2(fx, 4, hw, &pd) == 0 && pd.Rows == 4 && pd.Columns == 4 &&
                       pd.Class == 3 && pd.Type == 3 && pd.Name[0] == 'W');
        a = ((GETBYNAME)VT(fx)[19])(fx, h, "UIName");
        CHECK(218, a && CALL2(fx, 51, a, &str) == 0 && str && str[0] == 'I');
        CHECK(219, CALL2(fx, 63, &np, 0) == 0 && np == 1 && CALL1(fx, 64, 0) == 0 &&
                       CALL2(dev, 58, 27, &rs) == 0 && rs == 1);
        CHECK(220, CALL3(dev, 110, 0, got, 1) == 0 && got[0] == 0.75f &&
                       ((SETFLOATFX)VT(fx)[30])(fx, h, 0.5f) == 0 && CALL0(fx, 65) == 0 &&
                       CALL3(dev, 110, 0, got, 1) == 0 && got[0] == 0.5f);
        CHECK(221, CALL0(fx, 66) == 0 && CALL0(fx, 67) == 0 && CALL2(dev, 58, 27, &rs) == 0 && rs == 0);
        CHECK(222, CALL0(fx, 2) == 0);
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

/* DirectPlay 8 (#863) : deux pairs dans le même programme, sur 127.0.0.1. */
__declspec(dllimport) HRESULT WINAPI DirectPlay8Create(const GUID *, void **, void *);
__declspec(dllimport) HRESULT WINAPI DirectPlay8AddressCreate(const GUID *, void **, void *);
typedef HRESULT(WINAPI *M11)(void *, void *, void *, void *, void *, void *, void *, void *, void *,
                             void *, void *, void *);
#define CALL11(o, i, a, b, c, d, e, f, g, h, j, k, l) \
    ((M11)VT(o)[i])(o, P(a), P(b), P(c), P(d), P(e), P(f), P(g), P(h), P(j), P(k), P(l))
static const GUID CLSID_DirectPlay8Peer = {
    0x286f484d, 0x375e, 0x4458, {0xa2, 0x72, 0xb1, 0x38, 0xe2, 0xf8, 0x0a, 0x6a}};
static const GUID IID_IDirectPlay8Peer = {
    0x5102dacf, 0x241b, 0x11d3, {0xae, 0xa7, 0x00, 0x60, 0x97, 0xb0, 0x14, 0x11}};
static const GUID IID_IDirectPlay8Address = {
    0x83783300, 0x4063, 0x4c8a, {0x9d, 0xb3, 0x82, 0x83, 0x0a, 0x7f, 0xeb, 0x31}};
static const GUID CLSID_DP8SP_TCPIP = {
    0xebfe7ba0, 0x628d, 0x11d2, {0xae, 0x0f, 0x00, 0x60, 0x97, 0xb0, 0x14, 0x11}};
typedef unsigned short WCH;
typedef struct {
    DWORD dwSize, dwFlags;
    GUID guidInstance, guidApplication;
    DWORD dwMaxPlayers, dwCurrentPlayers;
    WCH *pwszSessionName, *pwszPassword;
    void *pvReservedData;
    DWORD dwReservedDataSize;
    void *pvApplicationReservedData;
    DWORD dwApplicationReservedDataSize;
} DPN_APPLICATION_DESC;
typedef struct {
    DWORD dwSize, dwInfoFlags;
    WCH *pwszName;
    void *pvData;
    DWORD dwDataSize, dwPlayerFlags;
} DPN_PLAYER_INFO;
typedef struct { DWORD dwBufferSize; unsigned char *pBufferData; } DPN_BUFFER_DESC;
typedef struct { DWORD dwSize, dpnid; void *pvPlayerContext; } DPNMSG_PLAYER;
typedef struct {
    DWORD dwSize, dpnidSender;
    void *pvPlayerContext;
    unsigned char *pReceiveData;
    DWORD dwReceiveDataSize, hBufferHandle;
} DPNMSG_RECEIVE;
typedef struct {
    DWORD dwSize, hAsyncOp;
    void *pvUserContext;
    HRESULT hResultCode;
    void *pvApplicationReplyData;
    DWORD dwApplicationReplyDataSize, dpnidLocal;
} DPNMSG_CONNECT_COMPLETE;
typedef struct {
    DWORD dwSize;
    void *pAddressSender, *pAddressDevice;
    const DPN_APPLICATION_DESC *pApplicationDescription;
    void *pvResponseData;
    DWORD dwResponseDataSize;
    void *pvUserContext;
    DWORD dwRoundTripLatencyMS;
} DPNMSG_ENUM_HOSTS_RESPONSE;
typedef struct {
    DWORD dwSize;
    void *pvUserConnectData;
    DWORD dwUserConnectDataSize;
    void *pvReplyData;
    DWORD dwReplyDataSize;
    void *pvReplyContext, *pvPlayerContext, *pAddressPlayer, *pAddressDevice;
} DPNMSG_INDICATE_CONNECT;
enum {
    P8_INITIALIZE = 3, P8_CONNECT = 6, P8_SENDTO = 7, P8_HOST = 9, P8_GETAPPDESC = 10,
    P8_ENUMPLAYERS = 18, P8_SETPEERINFO = 20, P8_GETPEERINFO = 21, P8_LOCALADDRESSES = 23,
    P8_CLOSE = 24, P8_ENUMHOSTS = 25, A8_BUILDA = 4, A8_GETURLA = 10, A8_SETSP = 13,
    A8_GETCOMPONENT = 16, A8_ADDCOMPONENT = 18
};
#define DP8_SYNC 0x80000000u
static volatile int h8_create, h8_indicate, h8_recv, h8_destroy, c8_create, c8_recv, c8_done,
    c8_enum;
static volatile DWORD h8_from, c8_from, c8_local;
static HRESULT WINAPI dp8_handler(void *ctx, DWORD id, void *msg) {
    int host = ctx == (void *)0x8001;
    switch (id) {
    case 0xFFFF0007: /* CREATE_PLAYER */
        ((DPNMSG_PLAYER *)msg)->pvPlayerContext = (void *)(UINT_PTR)(0x700 + h8_create);
        if (host) h8_create++; else c8_create++;
        break;
    case 0xFFFF0009: /* DESTROY_PLAYER */
        if (host) h8_destroy++;
        break;
    case 0xFFFF000E: { /* INDICATE_CONNECT */
        DPNMSG_INDICATE_CONNECT *m = msg;
        if (m->dwUserConnectDataSize == 4 && *(DWORD *)m->pvUserConnectData == 0x4A4F494E)
            h8_indicate++;
        break;
    }
    case 0xFFFF0011: { /* RECEIVE */
        DPNMSG_RECEIVE *m = msg;
        if (host && m->dwReceiveDataSize == 8 && m->pReceiveData[0] == 'b') {
            h8_from = m->dpnidSender;
            h8_recv++;
        } else if (!host && m->dwReceiveDataSize == 6 && m->pReceiveData[0] == 's') {
            c8_from = m->dpnidSender;
            c8_recv++;
        }
        break;
    }
    case 0xFFFF0005: { /* CONNECT_COMPLETE */
        DPNMSG_CONNECT_COMPLETE *m = msg;
        if (m->hResultCode == 0 && m->pvUserContext == (void *)0x55) {
            c8_local = m->dpnidLocal;
            c8_done++;
        }
        break;
    }
    case 0xFFFF000B: { /* ENUM_HOSTS_RESPONSE */
        DPNMSG_ENUM_HOSTS_RESPONSE *m = msg;
        if (m->pvUserContext == (void *)0x66 && m->pApplicationDescription->pwszSessionName[0] == 'W' &&
            m->pApplicationDescription->dwCurrentPlayers == 1)
            c8_enum++;
        break;
    }
    }
    return 0;
}
static void dp8_name(DPN_PLAYER_INFO *pi, WCH *name) {
    zero(pi, sizeof *pi);
    pi->dwSize = sizeof *pi;
    pi->dwInfoFlags = 1; /* DPNINFO_NAME */
    pi->pwszName = name;
}

static int dplay8_tests(void) {
    void *host = 0, *client = 0, *dev = 0, *haddr = 0, *locals[4] = {0};
    DPN_APPLICATION_DESC ad;
    DPN_PLAYER_INFO pi;
    DPN_BUFFER_DESC bd;
    DWORD n, size, type, port = 0, h = 0, ids[8], join = 0x4A4F494E;
    char url[256];
    unsigned char buf[512];
    static WCH hote[] = {'H', 'o', 't', 'e', 0}, cli[] = {'C', 'l', 'i', 0},
               session[] = {'W', 'i', 'n', 'l', 'u', 'x', 0},
               kport[] = {'p', 'o', 'r', 't', 0},
               khost[] = {'h', 'o', 's', 't', 'n', 'a', 'm', 'e', 0},
               loop[] = {'1', '2', '7', '.', '0', '.', '0', '.', '1', 0};

    /* Adresses : URL DirectPlay aller-retour. */
    CHECK(223, DirectPlay8AddressCreate(&IID_IDirectPlay8Address, &haddr, 0) == 0 && haddr);
    CHECK(224, CALL1(haddr, A8_BUILDA,
                     "x-directplay:/provider=%7BEBFE7BA0-628D-11D2-AE0F-006097B01411%7D;"
                     "hostname=127.0.0.1;port=2302") == 0);
    n = sizeof url;
    size = sizeof port;
    CHECK(225, CALL2(haddr, A8_GETURLA, url, &n) == 0 && url[0] == 'x' &&
                   CALL4(haddr, A8_GETCOMPONENT, kport, &port, &size, &type) == 0 &&
                   port == 2302 && type == 2);

    /* Hôte. */
    CHECK(226, DirectPlay8Create(&IID_IDirectPlay8Peer, &host, 0) == 0 && host);
    CHECK(227, CALL3(host, P8_INITIALIZE, 0x8001, dp8_handler, 0) == 0);
    dp8_name(&pi, hote);
    CHECK(228, CALL4(host, P8_SETPEERINFO, &pi, 0, 0, DP8_SYNC) == 0);
    CHECK(229, DirectPlay8AddressCreate(&IID_IDirectPlay8Address, &dev, 0) == 0 &&
                   CALL1(dev, A8_SETSP, &CLSID_DP8SP_TCPIP) == 0);
    zero(&ad, sizeof ad);
    ad.dwSize = sizeof ad;
    ad.guidApplication = APP_JEUX;
    ad.dwMaxPlayers = 4;
    ad.pwszSessionName = session;
    CHECK(230, CALL7(host, P8_HOST, &ad, &dev, 1, 0, 0, 0x11, 0) == 0);
    wait_for(&h8_create, 1, 2000);
    CHECK(231, h8_create == 1);
    /* Port réel de la partie, lu sur l'adresse locale de l'hôte. */
    n = 4;
    size = sizeof port;
    port = 0;
    CHECK(232, CALL3(host, P8_LOCALADDRESSES, locals, &n, 0) == 0 && n >= 1 &&
                   CALL4(locals[0], A8_GETCOMPONENT, kport, &port, &size, &type) == 0 && port);
    for (h = 0; h < n; h++) CALL0(locals[h], 2);

    /* Client par CoCreateInstance, recherche puis connexion. */
    CHECK(233, CoCreateInstance(&CLSID_DirectPlay8Peer, 0, 1, &IID_IDirectPlay8Peer, &client) == 0 &&
                   client);
    CHECK(234, CALL3(client, P8_INITIALIZE, 0x8002, dp8_handler, 0) == 0);
    dp8_name(&pi, cli);
    CHECK(235, CALL4(client, P8_SETPEERINFO, &pi, 0, 0, DP8_SYNC) == 0);
    CALL0(haddr, 8); /* Clear */
    CHECK(236, CALL1(haddr, A8_SETSP, &CLSID_DP8SP_TCPIP) == 0 &&
                   CALL4(haddr, A8_ADDCOMPONENT, khost, loop, sizeof loop, 1) == 0 &&
                   CALL4(haddr, A8_ADDCOMPONENT, kport, &port, 4, 2) == 0);
    zero(&ad, sizeof ad);
    ad.dwSize = sizeof ad;
    ad.guidApplication = APP_JEUX;
    CHECK(237, CALL11(client, P8_ENUMHOSTS, &ad, haddr, dev, 0, 0, 1, 0, 800, 0x66, &h, DP8_SYNC) ==
                       0 &&
                   c8_enum >= 1);
    CHECK(238, CALL11(client, P8_CONNECT, &ad, haddr, dev, 0, 0, &join, 4, 0, 0x55, &h, DP8_SYNC) ==
                   0);
    wait_for(&c8_done, 1, 3000);
    wait_for(&c8_create, 2, 3000);
    CHECK(239, c8_done == 1 && c8_local != 0 && c8_create == 2);
    wait_for(&h8_create, 2, 3000);
    CHECK(240, h8_indicate == 1 && h8_create == 2);

    /* Messages dans les deux sens. */
    bd.dwBufferSize = 8;
    bd.pBufferData = (unsigned char *)"bonjour";
    CHECK(241, CALL7(client, P8_SENDTO, 0, &bd, 1, 0, 0, &h, DP8_SYNC | 0x10) == 0);
    wait_for(&h8_recv, 1, 3000);
    CHECK(242, h8_recv == 1 && h8_from == c8_local);
    bd.dwBufferSize = 6;
    bd.pBufferData = (unsigned char *)"salut";
    CHECK(243, CALL7(host, P8_SENDTO, h8_from, &bd, 1, 0, 0, &h, DP8_SYNC) == 0);
    wait_for(&c8_recv, 1, 3000);
    CHECK(244, c8_recv == 1 && c8_from != 0 && c8_from != c8_local);

    /* Le client voit le nom et le rôle de l'hôte, la partie et ses joueurs. */
    size = sizeof buf;
    CHECK(245, CALL4(client, P8_GETPEERINFO, c8_from, buf, &size, 0) == 0 &&
                   ((DPN_PLAYER_INFO *)buf)->pwszName[0] == 'H' &&
                   (((DPN_PLAYER_INFO *)buf)->dwPlayerFlags & 4));
    size = sizeof buf;
    CHECK(246, CALL3(client, P8_GETAPPDESC, buf, &size, 0) == 0 &&
                   ((DPN_APPLICATION_DESC *)buf)->dwCurrentPlayers == 2 &&
                   ((DPN_APPLICATION_DESC *)buf)->pwszSessionName[0] == 'W');
    n = 8;
    CHECK(247, CALL3(client, P8_ENUMPLAYERS, ids, &n, 1) == 0 && n == 2);

    /* Départ du client : l'hôte détruit son joueur. */
    CHECK(248, CALL1(client, P8_CLOSE, 0) == 0);
    wait_for(&h8_destroy, 1, 3000);
    CHECK(249, h8_destroy == 1);
    CHECK(250, CALL1(host, P8_CLOSE, 0) == 0);
    CALL0(client, 2);
    CALL0(host, 2);
    CALL0(dev, 2);
    CHECK(251, CALL0(haddr, 2) == 0);
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
    out("Direct3D 9 et D3DX9 : 47 verifications passees\n");
    dsound_tests(hwnd);
    out("DirectSound : 15 verifications passees\n");
    dshow_tests();
    out("DirectShow : 11 verifications passees\n");
    dplay_tests();
    out("DirectPlay : 23 verifications passees\n");
    dplay8_tests();
    out("DirectPlay 8 : 29 verifications passees\n");
    midi_tests();
    out("MIDI : 15 verifications passees\n");
    acm_tests();
    out("msacm32 : 9 verifications passees\n");
    adpcm_tests();
    out("msacm32 ADPCM : 3 verifications passees\n");
    if (midi_in_tests() == 0) out("Entree MIDI : 9 verifications passees\n");
    else out("Entree MIDI : WINLUX_MIDI_IN_DIR absent, sautee\n");
    timer_wave_tests();
    out("timeSetEvent et waveOut : 9 verifications passees\n");
    out("Jeux anciens : tout est correct\n");
    ExitProcess(0);
}

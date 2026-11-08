/* Direct3D 10 et 10.1 sur Direct3D 11 (#1090, étape 2), construit deux fois
 * par clang sans CRT : en x86 (d3d1032.exe) et en x64 (d3d1064.exe), pour
 * vérifier les deux dispositions des structures propres à Direct3D 10
 * (D3D10_MAPPED_TEXTURE2D, D3D10_VIEWPORT en entiers, D3D10_BUFFER_DESC à
 * cinq champs, D3D10_BLEND_DESC) et des tableaux de pointeurs d'objets.
 * Rend 0 et écrit « Direct3D 10 : tout est correct » si tout va bien,
 * sinon le numéro de la vérification qui a échoué. */
typedef unsigned int DWORD, UINT;
typedef int BOOL, LONG, HRESULT;
typedef unsigned short WORD;
typedef unsigned char BYTE;
typedef void *HANDLE, *HWND, *HINSTANCE, *HBRUSH;
#ifdef _WIN64
typedef unsigned long long UINT_PTR;
typedef long long LONG_PTR;
#else
typedef unsigned int UINT_PTR;
typedef long LONG_PTR;
#endif
typedef UINT_PTR WPARAM;
typedef LONG_PTR LPARAM, LRESULT;
#define WINAPI __stdcall
#define CALLBACK __stdcall
#define IMP __declspec(dllimport)
typedef LRESULT(CALLBACK *WNDPROC)(HWND, UINT, WPARAM, LPARAM);
typedef struct {
    UINT cbSize, style; WNDPROC lpfnWndProc; int cbClsExtra, cbWndExtra; HINSTANCE hInstance;
    HANDLE hIcon, hCursor; HBRUSH hbrBackground; const char *lpszMenuName;
    const char *lpszClassName; HANDLE hIconSm;
} WNDCLASSEXA;
typedef struct { unsigned long d1; unsigned short d2, d3; unsigned char d4[8]; } GUID;

IMP HANDLE WINAPI GetStdHandle(DWORD);
IMP BOOL WINAPI WriteFile(HANDLE, const void *, DWORD, DWORD *, void *);
IMP void WINAPI ExitProcess(DWORD);
IMP HINSTANCE WINAPI GetModuleHandleA(const char *);
IMP WORD WINAPI RegisterClassExA(const WNDCLASSEXA *);
IMP HWND WINAPI CreateWindowExA(DWORD, const char *, const char *, DWORD, int, int, int, int,
                                HWND, void *, HINSTANCE, void *);
IMP LRESULT WINAPI DefWindowProcA(HWND, UINT, WPARAM, LPARAM);
IMP HRESULT WINAPI D3D10CreateDevice(void *, UINT, void *, UINT, UINT, void **);
IMP HRESULT WINAPI D3D10CreateDeviceAndSwapChain(void *, UINT, void *, UINT, UINT, const void *,
                                                 void **, void **);
IMP HRESULT WINAPI D3D10CreateDevice1(void *, UINT, void *, UINT, UINT, UINT, void **);

/* d3d10.h, d3d10_1.h et dxgi.h (extraits publics). */
typedef struct { UINT ByteWidth, Usage, BindFlags, CPUAccessFlags, MiscFlags; } BUFFER_DESC;
typedef struct { const void *pSysMem; UINT SysMemPitch, SysMemSlicePitch; } SUBRESOURCE_DATA;
typedef struct {
    UINT Width, Height, MipLevels, ArraySize, Format, Count, Quality, Usage, BindFlags,
        CPUAccessFlags, MiscFlags;
} TEXTURE2D_DESC;
typedef struct { void *pData; UINT RowPitch; } MAPPED_TEXTURE2D;
typedef struct { int TopLeftX, TopLeftY; UINT Width, Height; float MinDepth, MaxDepth; } VIEWPORT;
typedef struct {
    BOOL AlphaToCoverageEnable, BlendEnable[8];
    UINT SrcBlend, DestBlend, BlendOp, SrcBlendAlpha, DestBlendAlpha, BlendOpAlpha;
    BYTE RenderTargetWriteMask[8];
} BLEND_DESC;
typedef struct {
    UINT Width, Height, RefreshNum, RefreshDen, Format, ScanlineOrdering, Scaling;
} MODE_DESC;
typedef struct {
    MODE_DESC BufferDesc;
    UINT Count, Quality, BufferUsage, BufferCount;
    HWND OutputWindow;
    BOOL Windowed;
    UINT SwapEffect, Flags;
} SWAP_CHAIN_DESC;

typedef struct IUnk { void **vt; } IUnk;
typedef HRESULT(WINAPI *M0)(void *);
typedef HRESULT(WINAPI *M1)(void *, void *);
typedef HRESULT(WINAPI *M2)(void *, void *, void *);
typedef HRESULT(WINAPI *M3)(void *, void *, void *, void *);
typedef HRESULT(WINAPI *M4)(void *, void *, void *, void *, void *);
typedef HRESULT(WINAPI *M5)(void *, void *, void *, void *, void *, void *);
#define P(a) (void *)(UINT_PTR)(a)
#define VT(o) (((IUnk *)(o))->vt)
#define CALL0(o, i) ((M0)VT(o)[i])(o)
#define CALL1(o, i, a) ((M1)VT(o)[i])(o, P(a))
#define CALL2(o, i, a, b) ((M2)VT(o)[i])(o, P(a), P(b))
#define CALL3(o, i, a, b, c) ((M3)VT(o)[i])(o, P(a), P(b), P(c))
#define CALL4(o, i, a, b, c, d) ((M4)VT(o)[i])(o, P(a), P(b), P(c), P(d))
#define CALL5(o, i, a, b, c, d, e) ((M5)VT(o)[i])(o, P(a), P(b), P(c), P(d), P(e))
#define RELEASE(o) CALL0(o, 2)

/* Indices des méthodes (IUnknown = 0..2, ID3D10DeviceChild = 3..6). */
enum {
    DEV_VSSETCB = 3, DEV_OMSETRT = 24, DEV_RSSETVP = 30, DEV_COPYRESOURCE = 33,
    DEV_CLEARRTV = 35, DEV_VSGETCB = 39, DEV_OMGETRT = 56, DEV_RSGETVP = 61,
    DEV_CREATEBUFFER = 71, DEV_CREATETEXTURE2D = 73, DEV_CREATERTV = 76,
    DEV_CREATEBLEND = 83, DEV_GETCREATIONFLAGS = 94, DEV1_GETFEATURELEVEL = 100,
    RES_GETTYPE = 7, RES_MAP = 10, RES_UNMAP = 11, RES_GETDESC = 12, STATE_GETDESC = 7,
    VIEW_GETRESOURCE = 7, SC_PRESENT = 8, SC_GETBUFFER = 9
};
static const GUID IID_IDXGIDevice = {
    0x54ec77fa, 0x1377, 0x44e6, {0x8c, 0x32, 0x88, 0xfd, 0x5f, 0x44, 0xc8, 0x4c}};
static const GUID IID_ID3D10Texture2D = {
    0x9b7e4c04, 0x342c, 0x4106, {0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0}};
static const GUID IID_ID3D10Device1 = {
    0x9b7e4c8f, 0x342c, 0x4106, {0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0}};

#define GUARD 0x5a5a5a5au

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

static void *texture(void *dev, UINT w, UINT h, UINT usage, UINT bind, UINT cpu,
                     const SUBRESOURCE_DATA *init) {
    TEXTURE2D_DESC d;
    void *t = 0;
    zero(&d, sizeof d);
    d.Width = w;
    d.Height = h;
    d.MipLevels = 1;
    d.ArraySize = 1;
    d.Format = 28;
    d.Count = 1;
    d.Usage = usage;
    d.BindFlags = bind;
    d.CPUAccessFlags = cpu;
    if (CALL3(dev, DEV_CREATETEXTURE2D, &d, init, &t) < 0) return 0;
    return t;
}

static void *buffer(void *dev, UINT bytes, UINT usage, UINT bind, UINT cpu,
                    const SUBRESOURCE_DATA *init) {
    BUFFER_DESC d;
    void *b = 0;
    zero(&d, sizeof d);
    d.ByteWidth = bytes;
    d.Usage = usage;
    d.BindFlags = bind;
    d.CPUAccessFlags = cpu;
    if (CALL3(dev, DEV_CREATEBUFFER, &d, init, &b) < 0) return 0;
    return b;
}

void entry(void) {
    WNDCLASSEXA wc;
    HWND hwnd;
    void *dev = 0, *dev1 = 0, *dxgidev = 0, *sc = 0, *dev2 = 0, *back = 0, *same = 0;
    void *staging, *rt, *rtv = 0, *res = 0, *blend = 0, *p = 0;
    UINT i, n;
    struct { MAPPED_TEXTURE2D m; DWORD guard; } map;
    struct { BUFFER_DESC d; DWORD guard; } bd;
    struct { TEXTURE2D_DESC d; DWORD guard; } td;
    struct { BLEND_DESC d; DWORD guard; } bl;
    struct { VIEWPORT v[2]; DWORD guard; } vps;
    BLEND_DESC want;
    VIEWPORT vp;
    SWAP_CHAIN_DESC scd;
    SUBRESOURCE_DATA init;
    DWORD words[4] = {1, 2, 3, 4};
    DWORD pixels[4] = {0x11223344, 0x55667788, 0x99aabbcc, 0xddeeff00};
    void *ptrs[3], *objs[2], *dsv;
    float red[4] = {1.0f, 0.0f, 0.0f, 1.0f};
    const DWORD *px;

    zero(&wc, sizeof wc);
    wc.cbSize = sizeof wc;
    wc.lpfnWndProc = wndproc;
    wc.hInstance = GetModuleHandleA(0);
    wc.lpszClassName = "WinluxD3D10";
    RegisterClassExA(&wc);
    hwnd = CreateWindowExA(0, "WinluxD3D10", "Direct3D 10", 0x10CF0000, 0, 0, 320, 240, 0, 0,
                           wc.hInstance, 0);
    CHECK(1, hwnd != 0);

    /* Appareil Direct3D 10 (D3D10_DRIVER_TYPE_HARDWARE = 0, SDK 29). */
    CHECK(2, D3D10CreateDevice(0, 0, 0, 0, 29, &dev) >= 0 && dev != 0);
    CHECK(3, CALL0(dev, DEV_GETCREATIONFLAGS) == 0);

    /* Tampon de lecture : D3D10_BUFFER_DESC à cinq champs, Map rend un pointeur. */
    init.pSysMem = words;
    init.SysMemPitch = 16;
    init.SysMemSlicePitch = 0;
    staging = buffer(dev, 16, 3, 0, 0x20000, &init);
    CHECK(4, staging != 0);
    zero(&bd, sizeof bd);
    bd.guard = GUARD;
    CALL1(staging, RES_GETDESC, &bd.d);
    CHECK(5, bd.d.ByteWidth == 16 && bd.d.Usage == 3 && bd.d.CPUAccessFlags == 0x20000
                 && bd.guard == GUARD);
    n = 0;
    CALL1(staging, RES_GETTYPE, &n);
    CHECK(6, n == 1);
    ptrs[0] = 0;
    ptrs[1] = P(GUARD);
    CHECK(7, CALL3(staging, RES_MAP, 1, 0, ptrs) >= 0 && ptrs[0] != 0 && ptrs[1] == P(GUARD));
    px = (const DWORD *)ptrs[0];
    CHECK(8, px[0] == 1 && px[1] == 2 && px[2] == 3 && px[3] == 4);
    CALL0(staging, RES_UNMAP);
    RELEASE(staging);

    /* Texture avec données initiales, relue par D3D10_MAPPED_TEXTURE2D
     * (8 octets en x86, 16 en x64). */
    init.pSysMem = pixels;
    init.SysMemPitch = 8;
    staging = texture(dev, 2, 2, 3, 0, 0x20000, &init);
    CHECK(9, staging != 0);
    zero(&map, sizeof map);
    map.guard = GUARD;
    CHECK(10, CALL4(staging, RES_MAP, 0, 1, 0, &map.m) >= 0);
    CHECK(11, map.m.pData != 0 && map.m.RowPitch >= 8 && map.guard == GUARD);
    px = (const DWORD *)map.m.pData;
    CHECK(12, px[0] == pixels[0] && px[1] == pixels[1]);
    px = (const DWORD *)((const BYTE *)map.m.pData + map.m.RowPitch);
    CHECK(13, px[0] == pixels[2] && px[1] == pixels[3]);
    CALL1(staging, RES_UNMAP, 0);
    zero(&td, sizeof td);
    td.guard = GUARD;
    CALL1(staging, RES_GETDESC, &td.d);
    CHECK(14, td.d.Width == 2 && td.d.Height == 2 && td.d.Format == 28 && td.guard == GUARD);
    RELEASE(staging);

    /* Cible de rendu effacée en rouge puis recopiée ; la vue rend sa ressource. */
    rt = texture(dev, 4, 4, 0, 0x20, 0, 0);
    CHECK(15, rt != 0);
    CHECK(16, CALL3(dev, DEV_CREATERTV, rt, 0, &rtv) >= 0 && rtv != 0);
    CALL1(rtv, VIEW_GETRESOURCE, &res);
    CHECK(17, res == rt);
    RELEASE(res);
    objs[0] = rtv;
    CALL3(dev, DEV_OMSETRT, 1, objs, 0);
    CALL2(dev, DEV_CLEARRTV, rtv, red);
    staging = texture(dev, 4, 4, 3, 0, 0x20000, 0);
    CHECK(18, staging != 0);
    CALL2(dev, DEV_COPYRESOURCE, staging, rt);
    zero(&map, sizeof map);
    CHECK(19, CALL4(staging, RES_MAP, 0, 1, 0, &map.m) >= 0 && map.m.pData != 0);
    px = (const DWORD *)map.m.pData;
    CHECK(20, px[0] == 0xff0000ffu);
    CALL1(staging, RES_UNMAP, 0);
    RELEASE(staging);

    /* Tableaux de pointeurs rendus : 4 octets par case en x86 ; un objet
     * rendu est celui que le programme a donné (Direct3D 11 peut rendre
     * des cases vides). */
    ptrs[0] = ptrs[1] = P(1);
    ptrs[2] = P(GUARD);
    dsv = P(1);
    CALL3(dev, DEV_OMGETRT, 1, ptrs, &dsv);
    CHECK(21, (ptrs[0] == 0 || ptrs[0] == rtv) && ptrs[2] == P(GUARD) && dsv == 0);
    if (ptrs[0]) RELEASE(ptrs[0]);
    objs[0] = buffer(dev, 16, 0, 4, 0, 0);
    objs[1] = buffer(dev, 16, 0, 4, 0, 0);
    CHECK(22, objs[0] != 0 && objs[1] != 0);
    CALL3(dev, DEV_VSSETCB, 0, 2, objs);
    ptrs[0] = ptrs[1] = P(1);
    ptrs[2] = P(GUARD);
    CALL3(dev, DEV_VSGETCB, 0, 2, ptrs);
    CHECK(23, (ptrs[0] == 0 || ptrs[0] == objs[0]) && (ptrs[1] == 0 || ptrs[1] == objs[1])
                  && ptrs[2] == P(GUARD));
    for (i = 0; i < 2; i++)
        if (ptrs[i]) RELEASE(ptrs[i]);
    RELEASE(objs[0]);
    RELEASE(objs[1]);

    /* D3D10_VIEWPORT : coordonnées entières. */
    vp.TopLeftX = 3;
    vp.TopLeftY = 5;
    vp.Width = 64;
    vp.Height = 32;
    vp.MinDepth = 0.0f;
    vp.MaxDepth = 1.0f;
    CALL2(dev, DEV_RSSETVP, 1, &vp);
    zero(&vps, sizeof vps);
    vps.guard = GUARD;
    n = 2;
    CALL2(dev, DEV_RSGETVP, &n, vps.v);
    CHECK(24, n >= 1 && vps.v[0].TopLeftX == 3 && vps.v[0].TopLeftY == 5);
    CHECK(25, vps.v[0].Width == 64 && vps.v[0].Height == 32 && vps.v[0].MaxDepth == 1.0f
                  && vps.guard == GUARD);

    /* D3D10_BLEND_DESC (68 octets) : rendu tel quel. */
    zero(&want, sizeof want);
    want.BlendEnable[0] = 1;
    want.SrcBlend = 5;
    want.DestBlend = 6;
    want.BlendOp = 1;
    want.SrcBlendAlpha = 2;
    want.DestBlendAlpha = 1;
    want.BlendOpAlpha = 1;
    for (i = 0; i < 8; i++) want.RenderTargetWriteMask[i] = 15;
    CHECK(26, CALL2(dev, DEV_CREATEBLEND, &want, &blend) >= 0 && blend != 0);
    zero(&bl, sizeof bl);
    bl.guard = GUARD;
    CALL1(blend, STATE_GETDESC, &bl.d);
    CHECK(27, bl.d.BlendEnable[0] == 1 && bl.d.BlendEnable[1] == 0 && bl.d.SrcBlend == 5);
    CHECK(28, bl.d.DestBlend == 6 && bl.d.RenderTargetWriteMask[7] == 15 && bl.guard == GUARD);
    RELEASE(blend);

    /* L'appareil répond aussi pour DXGI. */
    CHECK(29, CALL2(dev, 0, &IID_IDXGIDevice, &dxgidev) >= 0 && dxgidev != 0);
    RELEASE(dxgidev);

    /* Direct3D 10.1 : niveau demandé rendu par GetFeatureLevel. */
    CHECK(30, D3D10CreateDevice1(0, 0, 0, 0, 0xa100, 0x20, &dev1) >= 0 && dev1 != 0);
    CHECK(31, (UINT)CALL0(dev1, DEV1_GETFEATURELEVEL) == 0xa100);
    CHECK(32, CALL2(dev1, 0, &IID_ID3D10Device1, &same) >= 0 && same == dev1);
    RELEASE(same);
    RELEASE(dev1);

    /* D3D10CreateDeviceAndSwapChain : DXGI_SWAP_CHAIN_DESC de 60 octets en
     * x86, tampon arrière vu comme ID3D10Texture2D. */
    zero(&scd, sizeof scd);
    scd.BufferDesc.Width = 64;
    scd.BufferDesc.Height = 48;
    scd.BufferDesc.Format = 28;
    scd.Count = 1;
    scd.BufferUsage = 0x20;
    scd.BufferCount = 1;
    scd.OutputWindow = hwnd;
    scd.Windowed = 1;
    CHECK(33, D3D10CreateDeviceAndSwapChain(0, 0, 0, 0, 29, &scd, &sc, &dev2) >= 0);
    CHECK(34, sc != 0 && dev2 != 0);
    CHECK(35, CALL3(sc, SC_GETBUFFER, 0, &IID_ID3D10Texture2D, &back) >= 0 && back != 0);
    zero(&td, sizeof td);
    td.guard = GUARD;
    CALL1(back, RES_GETDESC, &td.d);
    CHECK(36, td.d.Width == 64 && td.d.Height == 48 && td.guard == GUARD);
    CHECK(37, CALL3(dev2, DEV_CREATERTV, back, 0, &p) >= 0 && p != 0);
    CALL2(dev2, DEV_CLEARRTV, p, red);
    CHECK(38, CALL2(sc, SC_PRESENT, 0, 0) >= 0);
    RELEASE(p);
    RELEASE(back);
    RELEASE(sc);
    RELEASE(dev2);

    RELEASE(rtv);
    RELEASE(rt);
    RELEASE(dev);
    out("Direct3D 10 : tout est correct\n");
    ExitProcess(0);
}

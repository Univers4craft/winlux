/* Direct3D 11 et DXGI depuis un programme 32 bits (#1090), construit deux
 * fois par clang sans CRT : en x86 (d3d1132.exe) et en x64 (d3d1164.exe),
 * pour vérifier les deux dispositions des structures et des tableaux de
 * pointeurs d'objets.
 * Rend 0 et écrit « Direct3D 11 : tout est correct » si tout va bien,
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
IMP HRESULT WINAPI D3D11CreateDevice(void *, UINT, void *, UINT, const UINT *, UINT, UINT,
                                     void **, UINT *, void **);
IMP HRESULT WINAPI D3D11CreateDeviceAndSwapChain(void *, UINT, void *, UINT, const UINT *, UINT,
                                                 UINT, const void *, void **, void **, UINT *,
                                                 void **);
IMP HRESULT WINAPI CreateDXGIFactory(const GUID *, void **);

/* d3d11.h et dxgi.h (extraits publics). */
typedef struct { UINT ByteWidth, Usage, BindFlags, CPUAccessFlags, MiscFlags, Stride; } BUFFER_DESC;
typedef struct { const void *pSysMem; UINT SysMemPitch, SysMemSlicePitch; } SUBRESOURCE_DATA;
typedef struct {
    UINT Width, Height, MipLevels, ArraySize, Format, Count, Quality, Usage, BindFlags,
        CPUAccessFlags, MiscFlags;
} TEXTURE2D_DESC;
typedef struct { void *pData; UINT RowPitch, DepthPitch; } MAPPED_SUBRESOURCE;
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
typedef struct {
    WORD Description[128];
    UINT VendorId, DeviceId, SubSysId, Revision;
    UINT_PTR DedicatedVideoMemory, DedicatedSystemMemory, SharedSystemMemory;
    DWORD LuidLow;
    LONG LuidHigh;
} ADAPTER_DESC;

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

/* Indices des méthodes (IUnknown = 0..2). */
enum {
    DEV_CREATEBUFFER = 3, DEV_CREATETEXTURE2D = 5, DEV_CREATERTV = 9,
    CTX_VSSETCB = 7, CTX_MAP = 14, CTX_UNMAP = 15, CTX_IASETVB = 18, CTX_OMSETRT = 33,
    CTX_COPYRESOURCE = 47, CTX_CLEARRTV = 50, CTX_VSGETCB = 72, CTX_IAGETVB = 79,
    CTX_OMGETRT = 89,
    DXGIDEV_GETADAPTER = 7, ADAPTER_GETDESC = 8, FACTORY_CREATESWAPCHAIN = 10,
    SC_PRESENT = 8, SC_GETBUFFER = 9, SC_GETDESC = 12
};
static const GUID IID_IDXGIDevice = {
    0x54ec77fa, 0x1377, 0x44e6, {0x8c, 0x32, 0x88, 0xfd, 0x5f, 0x44, 0xc8, 0x4c}};
static const GUID IID_ID3D11Texture2D = {
    0x6f15aaf2, 0xd208, 0x4e89, {0x9a, 0xb4, 0x48, 0x95, 0x35, 0xd3, 0x4f, 0x9c}};
static const GUID IID_IDXGIFactory = {
    0x7b7166ec, 0x21c7, 0x44ae, {0xb2, 0x1a, 0xc9, 0xae, 0x32, 0x1a, 0xe3, 0x69}};

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

/* Texture 2D RGBA 8 bits. */
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
    void *dev = 0, *ctx = 0, *dxgidev = 0, *adapter = 0, *factory = 0, *sc = 0, *back = 0;
    void *dev2 = 0, *ctx2 = 0, *sc2 = 0;
    UINT fl = 0, i;
    struct { MAPPED_SUBRESOURCE m; DWORD guard; } map;
    struct { ADAPTER_DESC d; DWORD guard; } ad;
    struct { SWAP_CHAIN_DESC d; DWORD guard; } scd;
    SWAP_CHAIN_DESC want;
    SUBRESOURCE_DATA init;
    DWORD words[4] = {1, 2, 3, 4};
    DWORD pixels[4] = {0x11223344, 0x55667788, 0x99aabbcc, 0xddeeff00};
    void *ptrs[3], *objs[2], *dsv;
    UINT strides[2] = {12, 16}, offsets[2] = {0, 4};
    float red[4] = {1.0f, 0.0f, 0.0f, 1.0f};
    void *staging, *rt, *rtv = 0;
    const DWORD *px;

    zero(&wc, sizeof wc);
    wc.cbSize = sizeof wc;
    wc.lpfnWndProc = wndproc;
    wc.hInstance = GetModuleHandleA(0);
    wc.lpszClassName = "WinluxD3D11";
    RegisterClassExA(&wc);
    hwnd = CreateWindowExA(0, "WinluxD3D11", "Direct3D 11", 0x10CF0000, 0, 0, 320, 240, 0, 0,
                           wc.hInstance, 0);
    CHECK(1, hwnd != 0);

    /* Appareil et contexte immédiat. */
    CHECK(2, D3D11CreateDevice(0, 1, 0, 0, 0, 0, 7, &dev, &fl, &ctx) >= 0);
    CHECK(3, dev != 0 && ctx != 0 && fl >= 0x9100);

    /* D3D11_SUBRESOURCE_DATA puis D3D11_MAPPED_SUBRESOURCE (12 octets en x86). */
    init.pSysMem = words;
    init.SysMemPitch = 16;
    init.SysMemSlicePitch = 0;
    staging = buffer(dev, 16, 3, 0, 0x20000, &init);
    CHECK(4, staging != 0);
    zero(&map, sizeof map);
    map.guard = GUARD;
    CHECK(5, CALL5(ctx, CTX_MAP, staging, 0, 1, 0, &map.m) >= 0);
    CHECK(6, map.m.pData != 0 && map.guard == GUARD);
    px = (const DWORD *)map.m.pData;
    CHECK(7, px[0] == 1 && px[1] == 2 && px[2] == 3 && px[3] == 4);
    CALL2(ctx, CTX_UNMAP, staging, 0);
    RELEASE(staging);

    /* Texture avec données initiales, relue par Map. */
    init.pSysMem = pixels;
    init.SysMemPitch = 8;
    staging = texture(dev, 2, 2, 3, 0, 0x20000, &init);
    CHECK(8, staging != 0);
    zero(&map, sizeof map);
    map.guard = GUARD;
    CHECK(9, CALL5(ctx, CTX_MAP, staging, 0, 1, 0, &map.m) >= 0);
    CHECK(10, map.m.pData != 0 && map.m.RowPitch >= 8 && map.guard == GUARD);
    px = (const DWORD *)map.m.pData;
    CHECK(11, px[0] == pixels[0] && px[1] == pixels[1]);
    px = (const DWORD *)((const BYTE *)map.m.pData + map.m.RowPitch);
    CHECK(12, px[0] == pixels[2] && px[1] == pixels[3]);
    CALL2(ctx, CTX_UNMAP, staging, 0);
    RELEASE(staging);

    /* Cible de rendu : tableau de vues passé à OMSetRenderTargets, effacée
     * en rouge et recopiée dans une texture de lecture. */
    rt = texture(dev, 4, 4, 0, 0x20, 0, 0);
    CHECK(13, rt != 0);
    CHECK(14, CALL3(dev, DEV_CREATERTV, rt, 0, &rtv) >= 0 && rtv != 0);
    objs[0] = rtv;
    CALL3(ctx, CTX_OMSETRT, 1, objs, 0);
    CALL2(ctx, CTX_CLEARRTV, rtv, red);
    staging = texture(dev, 4, 4, 3, 0, 0x20000, 0);
    CHECK(15, staging != 0);
    CALL2(ctx, CTX_COPYRESOURCE, staging, rt);
    zero(&map, sizeof map);
    CHECK(16, CALL5(ctx, CTX_MAP, staging, 0, 1, 0, &map.m) >= 0 && map.m.pData != 0);
    px = (const DWORD *)map.m.pData;
    CHECK(17, px[0] == 0xff0000ffu);
    CALL2(ctx, CTX_UNMAP, staging, 0);
    RELEASE(staging);

    /* Tableaux de pointeurs rendus : 4 octets par case en x86. */
    ptrs[0] = ptrs[1] = P(1);
    ptrs[2] = P(GUARD);
    dsv = P(1);
    CALL3(ctx, CTX_OMGETRT, 1, ptrs, &dsv);
    CHECK(18, ptrs[2] == P(GUARD) && dsv != P(1));
    if (ptrs[0]) RELEASE(ptrs[0]);
    if (dsv) RELEASE(dsv);
    objs[0] = buffer(dev, 16, 0, 4, 0, 0);
    objs[1] = buffer(dev, 16, 0, 4, 0, 0);
    CHECK(19, objs[0] != 0 && objs[1] != 0);
    CALL3(ctx, CTX_VSSETCB, 0, 2, objs);
    ptrs[0] = ptrs[1] = P(1);
    ptrs[2] = P(GUARD);
    CALL3(ctx, CTX_VSGETCB, 0, 2, ptrs);
    CHECK(20, ptrs[0] != P(1) && ptrs[1] != P(1) && ptrs[2] == P(GUARD));
    for (i = 0; i < 2; i++)
        if (ptrs[i]) RELEASE(ptrs[i]);
    CALL5(ctx, CTX_IASETVB, 0, 2, objs, strides, offsets);
    ptrs[0] = ptrs[1] = P(1);
    ptrs[2] = P(GUARD);
    CALL5(ctx, CTX_IAGETVB, 0, 2, ptrs, strides, offsets);
    CHECK(21, ptrs[0] != P(1) && ptrs[2] == P(GUARD));
    for (i = 0; i < 2; i++)
        if (ptrs[i]) RELEASE(ptrs[i]);
    RELEASE(objs[0]);
    RELEASE(objs[1]);

    /* DXGI : adaptateur (trois SIZE_T de 4 octets en x86). */
    CHECK(22, CALL2(dev, 0, &IID_IDXGIDevice, &dxgidev) >= 0 && dxgidev != 0);
    CHECK(23, CALL1(dxgidev, DXGIDEV_GETADAPTER, &adapter) >= 0 && adapter != 0);
    zero(&ad, sizeof ad);
    ad.guard = GUARD;
    CHECK(24, CALL1(adapter, ADAPTER_GETDESC, &ad.d) >= 0);
    CHECK(25, ad.d.Description[0] != 0 && ad.d.DedicatedVideoMemory != 0 && ad.guard == GUARD);

    /* Chaîne d'échange : DXGI_SWAP_CHAIN_DESC de 60 octets en x86. */
    CHECK(26, CreateDXGIFactory(&IID_IDXGIFactory, &factory) >= 0 && factory != 0);
    zero(&want, sizeof want);
    want.BufferDesc.Width = 64;
    want.BufferDesc.Height = 48;
    want.BufferDesc.Format = 28;
    want.Count = 1;
    want.BufferUsage = 0x20;
    want.BufferCount = 2;
    want.OutputWindow = hwnd;
    want.Windowed = 1;
    want.SwapEffect = 0;
    want.Flags = 2;
    CHECK(27, CALL3(factory, FACTORY_CREATESWAPCHAIN, dev, &want, &sc) >= 0 && sc != 0);
    zero(&scd, sizeof scd);
    scd.guard = GUARD;
    CHECK(28, CALL1(sc, SC_GETDESC, &scd.d) >= 0);
    CHECK(29, scd.d.OutputWindow == hwnd && scd.d.BufferCount == 2 && scd.d.Windowed == 1);
    CHECK(30, scd.d.Flags == 2 && scd.d.BufferDesc.Width == 64 && scd.guard == GUARD);
    CHECK(31, CALL3(sc, SC_GETBUFFER, 0, &IID_ID3D11Texture2D, &back) >= 0 && back != 0);
    CHECK(32, CALL2(sc, SC_PRESENT, 0, 0) >= 0);
    RELEASE(back);
    RELEASE(sc);

    /* D3D11CreateDeviceAndSwapChain lit la même structure. */
    CHECK(33, D3D11CreateDeviceAndSwapChain(0, 1, 0, 0, 0, 0, 7, &want, &sc2, &dev2, &fl, &ctx2)
                  >= 0);
    CHECK(34, sc2 != 0 && dev2 != 0 && ctx2 != 0);
    zero(&scd, sizeof scd);
    CHECK(35, CALL1(sc2, SC_GETDESC, &scd.d) >= 0 && scd.d.OutputWindow == hwnd);
    RELEASE(sc2);
    RELEASE(ctx2);
    RELEASE(dev2);

    RELEASE(factory);
    RELEASE(adapter);
    RELEASE(dxgidev);
    RELEASE(rtv);
    RELEASE(rt);
    RELEASE(ctx);
    RELEASE(dev);
    out("Direct3D 11 : tout est correct\n");
    ExitProcess(0);
}

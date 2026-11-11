/* WASAPI depuis un programme 32 bits (banc pygame32 : IMMDeviceEnumerator
 * refusé par wow32), construit deux fois par clang sans CRT : en x86
 * (wasapi32.exe) et en x64 (wasapi64.exe).
 * Couvre l'énumération des périphériques, le magasin de propriétés
 * (PROPVARIANT chaîne et BLOB, 16 octets en x86 et 24 en x64), IAudioClient
 * (deux REFERENCE_TIME sur la pile), le tampon de rendu, l'horloge et les
 * volumes (nombres flottants passés par valeur).
 * Rend 0 et écrit « WASAPI : tout est correct » si tout va bien, sinon le
 * numéro de la vérification qui a échoué. */
typedef unsigned int DWORD, UINT;
typedef int BOOL, HRESULT;
typedef unsigned short WORD;
typedef unsigned char BYTE;
typedef long long LONGLONG;
typedef unsigned long long ULONGLONG;
typedef void *HANDLE;
#ifdef _WIN64
typedef unsigned long long UINT_PTR;
#else
typedef unsigned int UINT_PTR;
#endif
#define WINAPI __stdcall
#define IMP __declspec(dllimport)
typedef struct { unsigned long d1; unsigned short d2, d3; unsigned char d4[8]; } GUID;

IMP HANDLE WINAPI GetStdHandle(DWORD);
IMP BOOL WINAPI WriteFile(HANDLE, const void *, DWORD, DWORD *, void *);
IMP void WINAPI ExitProcess(DWORD);
IMP HRESULT WINAPI CoInitializeEx(void *, DWORD);
IMP HRESULT WINAPI CoCreateInstance(const GUID *, void *, DWORD, const GUID *, void **);
IMP void WINAPI CoTaskMemFree(void *);

/* propidl.h, mmreg.h (extraits publics). */
typedef struct { GUID fmtid; DWORD pid; } PROPERTYKEY;
typedef struct {
    WORD vt, r1, r2, r3;
    union {
        LONGLONG align;
        DWORD ulVal;
        WORD *pwszVal;
        struct { DWORD cbSize; BYTE *pBlobData; } blob;
    } u; /* 8 octets en x86, 16 en x64 (BLOB : pointeur à 16) */
} PROPVARIANT;
typedef struct {
    WORD wFormatTag, nChannels;
    DWORD nSamplesPerSec, nAvgBytesPerSec;
    WORD nBlockAlign, wBitsPerSample, cbSize;
} WAVEFORMATEX;

typedef struct IUnk { void **vt; } IUnk;
typedef HRESULT(WINAPI *M0)(void *);
typedef HRESULT(WINAPI *M1)(void *, void *);
typedef HRESULT(WINAPI *M2)(void *, void *, void *);
typedef HRESULT(WINAPI *M3)(void *, void *, void *, void *);
typedef HRESULT(WINAPI *M4)(void *, void *, void *, void *, void *);
typedef HRESULT(WINAPI *Init)(void *, DWORD, DWORD, LONGLONG, LONGLONG, const WAVEFORMATEX *,
                              const GUID *);
typedef HRESULT(WINAPI *SetF)(void *, float, const GUID *);
typedef HRESULT(WINAPI *SetChF)(void *, UINT, float);
#define P(a) (void *)(UINT_PTR)(a)
#define VT(o) (((IUnk *)(o))->vt)
#define CALL0(o, i) ((M0)VT(o)[i])(o)
#define CALL1(o, i, a) ((M1)VT(o)[i])(o, P(a))
#define CALL2(o, i, a, b) ((M2)VT(o)[i])(o, P(a), P(b))
#define CALL3(o, i, a, b, c) ((M3)VT(o)[i])(o, P(a), P(b), P(c))
#define CALL4(o, i, a, b, c, d) ((M4)VT(o)[i])(o, P(a), P(b), P(c), P(d))
#define RELEASE(o) CALL0(o, 2)

enum {
    EN_ENUM = 3, EN_DEFAULT = 4,
    COLL_COUNT = 3, COLL_ITEM = 4,
    DEV_ACTIVATE = 3, DEV_PROPS = 4, DEV_GETID = 5, DEV_STATE = 6,
    EP_FLOW = 3,
    PS_COUNT = 3, PS_GETVALUE = 5,
    AC_INIT = 3, AC_BUFSIZE = 4, AC_PADDING = 6, AC_SUPPORTED = 7, AC_MIXFMT = 8,
    AC_PERIOD = 9, AC_START = 10, AC_STOP = 11, AC_RESET = 12, AC_SERVICE = 14,
    RC_GETBUF = 3, RC_RELBUF = 4,
    SV_SET = 3, SV_GET = 4,
    CLK_FREQ = 3, CLK_POS = 4,
    STV_COUNT = 3, STV_SET = 4, STV_GET = 5,
    EPV_SETSCALAR = 7, EPV_GETSCALAR = 9
};
static const GUID CLSID_MMDeviceEnumerator = {
    0xbcde0395, 0xe52f, 0x467c, {0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e}};
static const GUID IID_IMMDeviceEnumerator = {
    0xa95664d2, 0x9614, 0x4f35, {0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6}};
static const GUID IID_IMMEndpoint = {
    0x1be09788, 0x6894, 0x4089, {0x85, 0x86, 0x9a, 0x2a, 0x6c, 0x26, 0x5a, 0xc5}};
static const GUID IID_IAudioClient = {
    0x1cb9ad4c, 0xdbfa, 0x4c32, {0xb1, 0x78, 0xc2, 0xf5, 0x68, 0xa7, 0x03, 0xb2}};
static const GUID IID_IAudioRenderClient = {
    0xf294acfc, 0x3146, 0x4483, {0xa7, 0xbf, 0xad, 0xdc, 0xa7, 0xc2, 0x60, 0xe2}};
static const GUID IID_ISimpleAudioVolume = {
    0x87ce5498, 0x68d6, 0x44e5, {0x92, 0x15, 0x6d, 0xa4, 0x7e, 0xf8, 0x83, 0xd8}};
static const GUID IID_IAudioClock = {
    0xcd63314f, 0x3fba, 0x4a1b, {0x81, 0x2c, 0xef, 0x96, 0x35, 0x8d, 0x3a, 0xc9}};
static const GUID IID_IAudioStreamVolume = {
    0x93014887, 0x242d, 0x4068, {0x8a, 0x15, 0xcf, 0x5e, 0x93, 0xb9, 0x0f, 0xe3}};
static const GUID IID_IAudioEndpointVolume = {
    0x5cdf2c82, 0x841e, 0x4546, {0x97, 0x22, 0x0c, 0xf7, 0x40, 0x78, 0x22, 0x9a}};
static const PROPERTYKEY PKEY_Device_FriendlyName = {
    {0xa45c254e, 0xdf1c, 0x4efd, {0x80, 0x20, 0x67, 0xd1, 0x46, 0xa8, 0x50, 0xe0}}, 14};
static const PROPERTYKEY PKEY_AudioEngine_DeviceFormat = {
    {0xf19f064d, 0x082c, 0x4e27, {0xbc, 0x73, 0x68, 0x82, 0xa1, 0xbb, 0x8e, 0x4c}}, 0};

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

void entry(void) {
    void *en = 0, *coll = 0, *dev = 0, *dev2 = 0, *ep = 0, *ps = 0, *ac = 0, *rc = 0;
    void *sv = 0, *clk = 0, *stv = 0, *epv = 0;
    WORD *id = 0;
    UINT count = 0, frames = 0, pad = 0, channels = 0;
    DWORD state = 0, flow = 7;
    struct { PROPVARIANT v; DWORD guard; } pv;
    WAVEFORMATEX *wf = 0;
    LONGLONG def = 0, min = 0;
    ULONGLONG freq = 0, pos = 0, qpc = 0;
    BYTE *data = 0;
    float f = 0;

    CHECK(1, CoInitializeEx(0, 0) >= 0);
    CHECK(2, CoCreateInstance(&CLSID_MMDeviceEnumerator, 0, 0x17, &IID_IMMDeviceEnumerator, &en)
                 >= 0 && en != 0);
    CHECK(3, CALL3(en, EN_DEFAULT, 0, 0, &dev) >= 0 && dev != 0);
    CHECK(4, CALL3(en, EN_ENUM, 0, 1, &coll) >= 0 && coll != 0);
    CHECK(5, CALL1(coll, COLL_COUNT, &count) >= 0 && count >= 1);
    CHECK(6, CALL2(coll, COLL_ITEM, 0, &dev2) >= 0 && dev2 != 0);
    CHECK(7, CALL1(dev, DEV_GETID, &id) >= 0 && id != 0 && id[0] != 0);
    CoTaskMemFree(id);
    CHECK(8, CALL1(dev, DEV_STATE, &state) >= 0 && state == 1);
    CHECK(9, CALL2(dev, 0, &IID_IMMEndpoint, &ep) >= 0 && ep != 0);
    CHECK(10, CALL1(ep, EP_FLOW, &flow) >= 0 && flow == 0);

    /* Magasin de propriétés : chaîne puis BLOB (format du moteur). */
    CHECK(11, CALL2(dev, DEV_PROPS, 0, &ps) >= 0 && ps != 0);
    CHECK(12, CALL1(ps, PS_COUNT, &count) >= 0 && count >= 2);
    zero(&pv, sizeof pv);
    pv.guard = GUARD;
    CHECK(13, CALL2(ps, PS_GETVALUE, &PKEY_Device_FriendlyName, &pv.v) >= 0);
    CHECK(14, pv.v.vt == 31 && pv.v.u.pwszVal != 0 && pv.v.u.pwszVal[0] != 0);
    CHECK(15, pv.guard == GUARD);
    CoTaskMemFree(pv.v.u.pwszVal);
    zero(&pv, sizeof pv);
    pv.guard = GUARD;
    CHECK(16, CALL2(ps, PS_GETVALUE, &PKEY_AudioEngine_DeviceFormat, &pv.v) >= 0);
    CHECK(17, pv.v.vt == 65 && pv.v.u.blob.cbSize >= 18 && pv.v.u.blob.pBlobData != 0);
    wf = (WAVEFORMATEX *)pv.v.u.blob.pBlobData;
    CHECK(18, wf->nChannels >= 1 && wf->nSamplesPerSec >= 8000 && pv.guard == GUARD);
    CoTaskMemFree(wf);

    /* IAudioClient : format, Initialize (REFERENCE_TIME sur la pile). */
    CHECK(19, CALL4(dev, DEV_ACTIVATE, &IID_IAudioClient, 0x17, 0, &ac) >= 0 && ac != 0);
    CHECK(20, CALL1(ac, AC_MIXFMT, &wf) >= 0 && wf != 0 && wf->nChannels == 2);
    CHECK(21, CALL2(ac, AC_PERIOD, &def, &min) >= 0 && def > 0 && min > 0 && min <= def);
    CHECK(22, ((Init)VT(ac)[AC_INIT])(ac, 0, 0, 200000, 0, wf, 0) >= 0);
    CHECK(23, CALL1(ac, AC_BUFSIZE, &frames) >= 0 && frames >= wf->nSamplesPerSec / 50);
    CHECK(24, CALL2(ac, AC_SERVICE, &IID_IAudioRenderClient, &rc) >= 0 && rc != 0);
    CHECK(25, CALL2(rc, RC_GETBUF, frames / 2, &data) >= 0 && data != 0);
    zero(data, frames / 2 * wf->nBlockAlign);
    CHECK(26, CALL2(rc, RC_RELBUF, frames / 2, 0) >= 0);
    CHECK(27, CALL1(ac, AC_PADDING, &pad) >= 0 && pad == frames / 2);

    /* Volumes : nombres flottants par valeur. */
    CHECK(28, CALL2(ac, AC_SERVICE, &IID_ISimpleAudioVolume, &sv) >= 0 && sv != 0);
    CHECK(29, ((SetF)VT(sv)[SV_SET])(sv, 0.5f, 0) >= 0);
    CHECK(30, CALL1(sv, SV_GET, &f) >= 0 && f == 0.5f);
    CHECK(31, ((SetF)VT(sv)[SV_SET])(sv, 1.0f, 0) >= 0);
    CHECK(32, CALL2(ac, AC_SERVICE, &IID_IAudioStreamVolume, &stv) >= 0 && stv != 0);
    CHECK(33, CALL1(stv, STV_COUNT, &channels) >= 0 && channels == 2);
    CHECK(34, ((SetChF)VT(stv)[STV_SET])(stv, 1, 0.75f) >= 0);
    f = 0;
    CHECK(35, CALL2(stv, STV_GET, 1, &f) >= 0 && f == 0.75f);
    CHECK(36, CALL2(ac, AC_SERVICE, &IID_IAudioClock, &clk) >= 0 && clk != 0);
    CHECK(37, CALL1(clk, CLK_FREQ, &freq) >= 0 && freq > 0);
    CHECK(38, CALL0(ac, AC_START) >= 0);
    CHECK(39, CALL2(clk, CLK_POS, &pos, &qpc) >= 0);
    CHECK(40, CALL0(ac, AC_STOP) >= 0 && CALL0(ac, AC_RESET) >= 0);

    CHECK(41, CALL4(dev, DEV_ACTIVATE, &IID_IAudioEndpointVolume, 0x17, 0, &epv) >= 0 && epv != 0);
    CHECK(42, ((SetF)VT(epv)[EPV_SETSCALAR])(epv, 0.25f, 0) >= 0);
    f = 0;
    CHECK(43, CALL1(epv, EPV_GETSCALAR, &f) >= 0 && f > 0.24f && f < 0.26f);
    CHECK(44, ((SetF)VT(epv)[EPV_SETSCALAR])(epv, 1.0f, 0) >= 0);

    CoTaskMemFree(wf);
    RELEASE(epv);
    RELEASE(clk);
    RELEASE(stv);
    RELEASE(sv);
    RELEASE(rc);
    RELEASE(ac);
    RELEASE(ps);
    RELEASE(ep);
    RELEASE(dev2);
    RELEASE(coll);
    RELEASE(dev);
    RELEASE(en);
    out("WASAPI : tout est correct\n");
    ExitProcess(0);
}

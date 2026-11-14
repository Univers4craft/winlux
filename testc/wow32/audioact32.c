/* Activation audio asynchrone et fabriques des DLL audio depuis un
 * programme 32 bits (#2428), construit deux fois par clang sans CRT : en
 * x86 (audioact32.exe) et en x64 (audioact64.exe).
 * Couvre ActivateAudioInterfaceAsync (rappel
 * IActivateAudioInterfaceCompletionHandler écrit dans le programme, en
 * stdcall en x86, puis GetActivateResult qui rend un IAudioClient
 * utilisable) et DllGetClassObject de mmdevapi et de dsound
 * (IClassFactory::CreateInstance).
 * Rend 0 et écrit « Activation audio : tout est correct » si tout va bien,
 * sinon le numéro de la vérification qui a échoué. */
typedef unsigned int DWORD, UINT;
typedef int BOOL, HRESULT;
typedef unsigned short WORD;
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
IMP void WINAPI Sleep(DWORD);
IMP HANDLE WINAPI LoadLibraryA(const char *);
IMP void *WINAPI GetProcAddress(HANDLE, const char *);
IMP HRESULT WINAPI ActivateAudioInterfaceAsync(const unsigned short *, const GUID *, void *, void *,
                                               void **);
IMP HRESULT WINAPI DllGetClassObject(const GUID *, const GUID *, void **);

/* mmreg.h (extrait public). */
typedef struct {
    WORD wFormatTag, nChannels;
    DWORD nSamplesPerSec, nAvgBytesPerSec;
    WORD nBlockAlign, wBitsPerSample, cbSize;
} WAVEFORMATEX;

typedef struct Obj { void **vt; } Obj;
typedef HRESULT(WINAPI *M0)(void *);
typedef HRESULT(WINAPI *M1)(void *, void *);
typedef HRESULT(WINAPI *M2)(void *, void *, void *);
typedef HRESULT(WINAPI *M3)(void *, void *, void *, void *);
typedef HRESULT(WINAPI *GetObj)(const GUID *, const GUID *, void **);
#define VT(o) (((Obj *)(o))->vt)
#define RELEASE(o) ((M0)VT(o)[2])(o)

static const GUID IID_IUnknown = {0x00000000, 0x0000, 0x0000, {0xc0, 0, 0, 0, 0, 0, 0, 0x46}};
static const GUID IID_IClassFactory = {0x00000001, 0x0000, 0x0000, {0xc0, 0, 0, 0, 0, 0, 0, 0x46}};
static const GUID IID_IAudioClient = {0x1cb9ad4c, 0xdbfa, 0x4c32, {0xb1, 0x78, 0xc2, 0xf5, 0x68, 0xa7, 0x03, 0xb2}};
static const GUID IID_IActivateAudioInterfaceCompletionHandler = {
    0x41d949ab, 0x9862, 0x444a, {0x80, 0xf6, 0xc2, 0x61, 0x33, 0x4d, 0xa5, 0xeb}};
static const GUID CLSID_MMDeviceEnumerator = {0xbcde0395, 0xe52f, 0x467c, {0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e}};
static const GUID IID_IMMDeviceEnumerator = {0xa95664d2, 0x9614, 0x4f35, {0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6}};
static const GUID CLSID_DirectSound8 = {0x3901cc3f, 0x84b5, 0x4fa4, {0xba, 0x35, 0xaa, 0x81, 0x72, 0xb8, 0xa0, 0x9b}};
static const GUID IID_IDirectSound8 = {0xc50a7e93, 0xf395, 0x4834, {0x9e, 0xf6, 0x7f, 0xa9, 0x9d, 0xe5, 0x09, 0x66}};
/* DEVINTERFACE_AUDIO_RENDER : le périphérique de rendu par défaut. */
static const unsigned short RENDER[] = {'{', 'E', '6', '3', '2', '7', 'C', 'A', 'D', '-', 'D', 'C', 'E', 'C',
                                        '-', '4', '9', '4', '9', '-', 'A', 'E', '8', 'A', '-', '9', '9', '1',
                                        'E', '9', '7', '6', 'A', '7', '9', 'D', '2', '}', 0};

enum { AC_MIXFMT = 8, EN_DEFAULT = 4, CF_CREATE = 3, OP_RESULT = 3 };

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

static int same(const GUID *a, const GUID *b) {
    const unsigned char *x = (const unsigned char *)a, *y = (const unsigned char *)b;
    int i;
    for (i = 0; i < 16; i++)
        if (x[i] != y[i]) return 0;
    return 1;
}

/* IActivateAudioInterfaceCompletionHandler écrit dans le programme. */
static volatile UINT completed, refs = 1;
static volatile HRESULT activate_hr = -1;
static void *volatile client;
static HRESULT WINAPI h_qi(void *t, const GUID *riid, void **out) {
    if (same(riid, &IID_IUnknown) || same(riid, &IID_IActivateAudioInterfaceCompletionHandler)) {
        refs++;
        *out = t;
        return 0;
    }
    *out = 0;
    return (HRESULT)0x80004002u;
}
static UINT WINAPI h_addref(void *t) { (void)t; return ++refs; }
static UINT WINAPI h_release(void *t) { (void)t; return --refs; }
static HRESULT WINAPI h_completed(void *t, void *op) {
    HRESULT hr = -1;
    void *unk = 0;
    (void)t;
    if (((M2)VT(op)[OP_RESULT])(op, &hr, &unk) == 0) {
        activate_hr = hr;
        client = unk;
    }
    completed++;
    return 0;
}
static void *h_vt[4] = {(void *)h_qi, (void *)h_addref, (void *)h_release, (void *)h_completed};
static Obj handler = {h_vt};

void entry(void) {
    void *op = 0, *cf = 0, *en = 0, *dev = 0, *ds = 0;
    WAVEFORMATEX *wf = 0;
    GetObj dsound_get;
    HANDLE dsound;
    int i;

    CHECK(1, ActivateAudioInterfaceAsync(RENDER, &IID_IAudioClient, 0, &handler, &op) == 0 && op);
    for (i = 0; i < 1000 && !completed; i++) Sleep(5);
    CHECK(2, completed == 1);
    CHECK(3, activate_hr == 0 && client);
    /* L'interface rendue est bien un IAudioClient : GetMixFormat. */
    CHECK(4, ((M1)VT(client)[AC_MIXFMT])(client, &wf) == 0 && wf);
    CHECK(5, wf->nChannels > 0 && wf->nSamplesPerSec >= 8000);
    RELEASE(client);
    RELEASE(op);

    /* DllGetClassObject de mmdevapi : fabrique de MMDeviceEnumerator. */
    CHECK(6, DllGetClassObject(&CLSID_MMDeviceEnumerator, &IID_IClassFactory, &cf) == 0 && cf);
    CHECK(7, ((M3)VT(cf)[CF_CREATE])(cf, 0, (void *)&IID_IMMDeviceEnumerator, &en) == 0 && en);
    CHECK(8, ((M3)VT(en)[EN_DEFAULT])(en, 0, 0, &dev) == 0 && dev);
    RELEASE(dev);
    RELEASE(en);
    RELEASE(cf);

    /* DllGetClassObject de dsound : fabrique de DirectSound8. */
    dsound = LoadLibraryA("dsound.dll");
    CHECK(9, dsound != 0);
    dsound_get = (GetObj)GetProcAddress(dsound, "DllGetClassObject");
    CHECK(10, dsound_get != 0);
    cf = 0;
    CHECK(11, dsound_get(&CLSID_DirectSound8, &IID_IClassFactory, &cf) == 0 && cf);
    CHECK(12, ((M3)VT(cf)[CF_CREATE])(cf, 0, (void *)&IID_IDirectSound8, &ds) == 0 && ds);
    RELEASE(ds);
    RELEASE(cf);

    out("Activation audio : tout est correct\n");
    ExitProcess(0);
}

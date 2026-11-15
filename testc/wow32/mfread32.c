/* Chaîne de lecture Media Foundation depuis un programme 32 bits (#2428,
 * lot 4), construite deux fois par clang sans CRT : en x86 (mfread32.exe)
 * et en x64 (mfread64.exe).
 * Couvre MFTEnum, MFTEnumEx et MFTEnum2 (GUID de catégorie passé par
 * valeur : 16 octets sur la pile x86, les arguments suivants ne doivent pas
 * glisser), MFEnumDeviceSources et MFCreateDeviceSource, puis
 * MFCreateSourceReaderFromURL sur un WAV écrit par le programme : type
 * natif, durée (PROPVARIANT), lecture synchrone jusqu'à la fin du flux,
 * retour au début, et lecture asynchrone par un IMFSourceReaderCallback
 * écrit dans le programme (stdcall en x86), appelé par le fil du lecteur.
 * Sans FFmpeg (WINLUX_TEST_SANS_FFMPEG défini par le script), le lecteur
 * doit refuser proprement le fichier et la lecture n'est pas jouée.
 * Rend 0 et écrit « MF lecture : tout est correct » si tout va bien, sinon
 * le numéro de la vérification qui a échoué. */
typedef unsigned int DWORD, UINT, UINT32;
typedef int BOOL, HRESULT;
typedef unsigned short WORD, WCHAR;
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
IMP void WINAPI Sleep(DWORD);
IMP DWORD WINAPI GetTempPathW(DWORD, WCHAR *);
IMP HANDLE WINAPI CreateFileW(const WCHAR *, DWORD, DWORD, void *, DWORD, DWORD, HANDLE);
IMP BOOL WINAPI CloseHandle(HANDLE);
IMP BOOL WINAPI DeleteFileW(const WCHAR *);
IMP DWORD WINAPI GetEnvironmentVariableA(const char *, char *, DWORD);
IMP void WINAPI CoTaskMemFree(void *);
IMP HRESULT WINAPI MFStartup(DWORD, DWORD);
IMP HRESULT WINAPI MFShutdown(void);
IMP HRESULT WINAPI MFCreateAttributes(void **, UINT32);
IMP HRESULT WINAPI MFTEnum(GUID, UINT32, void *, void *, void *, GUID **, UINT32 *);
IMP HRESULT WINAPI MFTEnumEx(GUID, UINT32, const void *, const void *, void ***, UINT32 *);
IMP HRESULT WINAPI MFTEnum2(GUID, UINT32, const void *, const void *, void *, void ***, UINT32 *);
IMP HRESULT WINAPI MFEnumDeviceSources(void *, void ***, UINT32 *);
IMP HRESULT WINAPI MFCreateDeviceSource(void *, void **);
IMP HRESULT WINAPI MFCreateSourceReaderFromURL(const WCHAR *, void *, void **);

/* propidl.h (extrait public) : 16 octets en x86, 24 en x64. */
typedef struct {
    WORD vt, r1, r2, r3;
    union {
        LONGLONG hVal;
        ULONGLONG uhVal;
        struct { DWORD cbSize; BYTE *pBlobData; } blob;
    } u;
} PROPVARIANT;

typedef struct Obj { void **vt; } Obj;
typedef HRESULT(WINAPI *M0)(void *);
typedef HRESULT(WINAPI *M1)(void *, void *);
typedef HRESULT(WINAPI *M2)(void *, void *, void *);
typedef HRESULT(WINAPI *M3)(void *, void *, void *, void *);
typedef HRESULT(WINAPI *Read)(void *, DWORD, DWORD, DWORD *, DWORD *, LONGLONG *, void **);
#define P(a) (void *)(UINT_PTR)(a)
#define VT(o) (((Obj *)(o))->vt)
#define RELEASE(o) ((M0)VT(o)[2])(o)

static const GUID IID_IUnknown = {0x00000000, 0x0000, 0x0000, {0xc0, 0, 0, 0, 0, 0, 0, 0x46}};
static const GUID IID_IMFSourceReaderCallback = {0xdeec8d99, 0xfa1d, 0x4d82, {0x84, 0xc2, 0x2c, 0x89, 0x69, 0x94, 0x48, 0x67}};
static const GUID MFT_CATEGORY_AUDIO_DECODER = {0x9ea73fb4, 0xef7a, 0x4559, {0x8d, 0x5d, 0x71, 0x9d, 0x8f, 0x04, 0x26, 0xc7}};
static const GUID MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE = {0xc60ac5fe, 0x252a, 0x478f, {0xa0, 0xef, 0xbc, 0x8f, 0xa5, 0xf7, 0xca, 0xd3}};
static const GUID MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID = {0x8ac3587a, 0x4ae7, 0x42d8, {0x99, 0xe0, 0x0a, 0x60, 0x13, 0xee, 0xf9, 0x0f}};
static const GUID MFMediaType_Audio = {0x73647561, 0x0000, 0x0010, {0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71}};
static const GUID MF_MT_AUDIO_NUM_CHANNELS = {0x37e48bf5, 0x645e, 0x4c5b, {0x89, 0xde, 0xad, 0xa9, 0xe2, 0x9b, 0x69, 0x6a}};
static const GUID MF_PD_DURATION = {0x6c990d33, 0xbb8e, 0x477a, {0x85, 0x98, 0x0d, 0x5d, 0x96, 0xfc, 0xd8, 0x8a}};
static const GUID MF_SOURCE_READER_ASYNC_CALLBACK = {0x1e3dbeac, 0xbb43, 0x4c35, {0xb5, 0x07, 0xcd, 0x64, 0x44, 0x64, 0xc9, 0x65}};
static const GUID GUID_NULL = {0, 0, 0, {0, 0, 0, 0, 0, 0, 0, 0}};

enum {
    /* IMFAttributes */
    AT_GETUINT32 = 7, AT_SETGUID = 24, AT_SETUNKNOWN = 27, AT_GETCOUNT = 30,
    /* IMFMediaType */
    MT_MAJOR = 33,
    /* IMFSample */
    SA_TOTAL = 45,
    /* IMFSourceReader */
    SR_SELECT = 4, SR_NATIVE = 5, SR_POSITION = 8, SR_READ = 9, SR_ATTR = 12,
};
#define FIRST_AUDIO 0xfffffffdu
#define MEDIASOURCE 0xffffffffu
#define ALL_STREAMS 0xfffffffeu
#define ENDOFSTREAM 0x2u
#define VT_I8 20
#define VT_UI8 21
#define RATE 8000
#define SAMPLES 8000 /* une seconde */

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

static int same(const GUID *a, const GUID *b) {
    const BYTE *x = (const BYTE *)a, *y = (const BYTE *)b;
    int i;
    for (i = 0; i < 16; i++)
        if (x[i] != y[i]) return 0;
    return 1;
}

static void put32(BYTE *p, DWORD v) { p[0] = (BYTE)v; p[1] = (BYTE)(v >> 8); p[2] = (BYTE)(v >> 16); p[3] = (BYTE)(v >> 24); }
static void put16(BYTE *p, DWORD v) { p[0] = (BYTE)v; p[1] = (BYTE)(v >> 8); }

/* WAV PCM 16 bits mono, 8 kHz, une seconde de dents de scie. */
static BYTE wav[44 + SAMPLES * 2];
static WCHAR path[300];

static void write_wav(void) {
    static const WCHAR name[] = {'w', 'i', 'n', 'l', 'u', 'x', '-', 'm', 'f', 'r', 'e', 'a', 'd', '-',
                                 '#', '#', '.', 'w', 'a', 'v', 0};
    DWORD n, i, k;
    HANDLE h;
    for (i = 0; i < 4; i++) wav[i] = (BYTE)"RIFF"[i];
    put32(wav + 4, 36 + SAMPLES * 2);
    for (i = 0; i < 8; i++) wav[8 + i] = (BYTE)"WAVEfmt "[i];
    put32(wav + 16, 16);
    put16(wav + 20, 1);
    put16(wav + 22, 1);
    put32(wav + 24, RATE);
    put32(wav + 28, RATE * 2);
    put16(wav + 32, 2);
    put16(wav + 34, 16);
    for (i = 0; i < 4; i++) wav[36 + i] = (BYTE)"data"[i];
    put32(wav + 40, SAMPLES * 2);
    for (i = 0; i < SAMPLES; i++) put16(wav + 44 + 2 * i, (i * 512) & 0x7fff);
    n = GetTempPathW(260, path);
    CHECK(30, n > 0 && n < 260);
    for (k = 0; name[k]; k++) path[n + k] = name[k];
    path[n + k] = 0;
    /* Un nom par ABI : les deux versions peuvent tourner à la suite. */
    path[n + 14] = sizeof(void *) == 4 ? '3' : '6';
    path[n + 15] = sizeof(void *) == 4 ? '2' : '4';
    h = CreateFileW(path, 0x40000000 /* GENERIC_WRITE */, 0, 0, 2 /* CREATE_ALWAYS */, 0x80, 0);
    CHECK(31, h != (HANDLE)(UINT_PTR)-1);
    CHECK(32, WriteFile(h, wav, sizeof(wav), &n, 0) && n == sizeof(wav));
    CloseHandle(h);
}

/* IMFSourceReaderCallback écrit dans le programme. */
static volatile UINT async_samples, async_eos, async_calls;
static volatile HRESULT async_hr;
static HRESULT WINAPI cb_qi(void *t, const GUID *riid, void **o) {
    if (same(riid, &IID_IUnknown) || same(riid, &IID_IMFSourceReaderCallback)) {
        *o = t;
        return 0;
    }
    *o = 0;
    return (HRESULT)0x80004002u;
}
static UINT WINAPI cb_ref(void *t) { (void)t; return 1; }
static HRESULT WINAPI cb_read(void *t, HRESULT hr, DWORD index, DWORD flags, LONGLONG ts, void *sample) {
    (void)t; (void)index; (void)ts;
    if (hr < 0) async_hr = hr;
    if (sample) async_samples++;
    if (flags & ENDOFSTREAM) async_eos = 1;
    async_calls++;
    return 0;
}
static HRESULT WINAPI cb_flush(void *t, DWORD i) { (void)t; (void)i; return 0; }
static HRESULT WINAPI cb_event(void *t, DWORD i, void *e) { (void)t; (void)i; (void)e; return 0; }
static void *cb_vt[6] = {(void *)cb_qi, (void *)cb_ref, (void *)cb_ref, (void *)cb_read, (void *)cb_flush,
                         (void *)cb_event};
static Obj callback = {cb_vt};

void entry(void) {
    struct { UINT32 before; UINT32 count; UINT32 after; } c;
    struct { void *before; void *list; void *after; } l;
    void *attrs = 0, *src = (void *)1, *reader = 0, *mt = 0, *sample = 0, *ar = 0;
    PROPVARIANT pv;
    GUID major;
    UINT32 channels = 0;
    DWORD idx, flags, total = 0, n, calls;
    LONGLONG ts;
    char env[8];
    int i, sans_ffmpeg = GetEnvironmentVariableA("WINLUX_TEST_SANS_FFMPEG", env, sizeof(env)) != 0;
    HRESULT hr;

    CHECK(1, MFStartup(0x20070, 0) == 0);

    /* MFTEnum* : catégorie par valeur, listes vides, rien d'écrit à côté. */
    c.before = c.after = 0x5a5a5a5a; c.count = 77;
    l.before = l.after = (void *)0x5a5a5a5a; l.list = (void *)1;
    CHECK(2, MFTEnumEx(MFT_CATEGORY_AUDIO_DECODER, 0x70, 0, 0, (void ***)&l.list, &c.count) == 0);
    CHECK(3, c.count == 0 && l.list == 0);
    CHECK(4, c.before == 0x5a5a5a5a && c.after == 0x5a5a5a5a && l.before == (void *)0x5a5a5a5a &&
                 l.after == (void *)0x5a5a5a5a);
    c.count = 77; l.list = (void *)1;
    CHECK(5, MFTEnum2(MFT_CATEGORY_AUDIO_DECODER, 0x70, 0, 0, 0, (void ***)&l.list, &c.count) == 0);
    CHECK(6, c.count == 0 && l.list == 0 && c.after == 0x5a5a5a5a && l.after == (void *)0x5a5a5a5a);
    c.count = 77; l.list = (void *)1;
    CHECK(7, MFTEnum(MFT_CATEGORY_AUDIO_DECODER, 0, 0, 0, 0, (GUID **)&l.list, &c.count) == 0);
    CHECK(8, c.count == 0 && l.list == 0 && c.after == 0x5a5a5a5a && l.after == (void *)0x5a5a5a5a);

    /* Caméras : la liste rendue est faite d'IMFActivate utilisables. */
    CHECK(9, MFCreateAttributes(&attrs, 1) == 0 && attrs);
    CHECK(10, ((M2)VT(attrs)[AT_SETGUID])(attrs, (void *)&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                                          (void *)&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID) == 0);
    c.count = 77; l.list = (void *)1;
    CHECK(11, MFEnumDeviceSources(attrs, (void ***)&l.list, &c.count) == 0);
    CHECK(12, c.count < 64 && (c.count == 0) == (l.list == 0) && c.after == 0x5a5a5a5a);
    for (n = 0; n < c.count; n++) {
        void *a = ((void **)l.list)[n];
        UINT32 k = 0;
        CHECK(13, ((M1)VT(a)[AT_GETCOUNT])(a, &k) == 0 && k > 0);
        RELEASE(a);
    }
    if (l.list) CoTaskMemFree(l.list);
    hr = MFCreateDeviceSource(attrs, &src);
    CHECK(14, (hr == 0) == (src != 0));
    if (src) RELEASE(src);
    RELEASE(attrs);

    /* Lecteur de source sur un WAV. */
    write_wav();
    hr = MFCreateSourceReaderFromURL(path, 0, &reader);
    if (sans_ffmpeg) {
        CHECK(15, hr < 0 && reader == 0);
        DeleteFileW(path);
        MFShutdown();
        out("MF lecture : tout est correct (sans FFmpeg : lecture non jouee)\n");
        ExitProcess(0);
    }
    CHECK(16, hr == 0 && reader);
    CHECK(17, ((M3)VT(reader)[SR_NATIVE])(reader, P(FIRST_AUDIO), P(0), &mt) == 0 && mt);
    CHECK(18, ((M1)VT(mt)[MT_MAJOR])(mt, &major) == 0 && same(&major, &MFMediaType_Audio));
    CHECK(19, ((M2)VT(mt)[AT_GETUINT32])(mt, (void *)&MF_MT_AUDIO_NUM_CHANNELS, &channels) == 0 &&
                  channels == 1);
    RELEASE(mt);
    pv.vt = 0; pv.u.uhVal = 0;
    CHECK(20, ((M3)VT(reader)[SR_ATTR])(reader, P(MEDIASOURCE), (void *)&MF_PD_DURATION, &pv) == 0);
    CHECK(21, pv.vt == VT_UI8 && pv.u.uhVal >= 9000000 && pv.u.uhVal <= 11000000);
    for (i = 0; i < 1000; i++) {
        sample = 0; flags = 0;
        CHECK(22, ((Read)VT(reader)[SR_READ])(reader, FIRST_AUDIO, 0, &idx, &flags, &ts, &sample) == 0);
        if (sample) {
            DWORD len = 0;
            CHECK(23, ((M1)VT(sample)[SA_TOTAL])(sample, &len) == 0);
            total += len;
            RELEASE(sample);
        }
        if (flags & ENDOFSTREAM) break;
    }
    CHECK(24, flags & ENDOFSTREAM);
    CHECK(25, total >= SAMPLES * 2 * 9 / 10);
    pv.vt = VT_I8; pv.u.hVal = 0;
    CHECK(26, ((M2)VT(reader)[SR_POSITION])(reader, (void *)&GUID_NULL, &pv) == 0);
    sample = 0;
    CHECK(27, ((Read)VT(reader)[SR_READ])(reader, FIRST_AUDIO, 0, &idx, &flags, &ts, &sample) == 0 &&
                  sample);
    RELEASE(sample);
    RELEASE(reader);

    /* Mode asynchrone : le rappel du programme reçoit les échantillons. */
    CHECK(40, MFCreateAttributes(&ar, 1) == 0 && ar);
    CHECK(41, ((M2)VT(ar)[AT_SETUNKNOWN])(ar, (void *)&MF_SOURCE_READER_ASYNC_CALLBACK, &callback) == 0);
    reader = 0;
    CHECK(42, MFCreateSourceReaderFromURL(path, ar, &reader) == 0 && reader);
    for (i = 0; i < 1000 && !async_eos; i++) {
        calls = async_calls;
        CHECK(43, ((Read)VT(reader)[SR_READ])(reader, FIRST_AUDIO, 0, 0, 0, 0, 0) == 0);
        for (n = 0; n < 1000 && async_calls == calls; n++) Sleep(5);
        CHECK(44, async_calls != calls);
    }
    CHECK(45, async_eos && async_samples > 0 && async_hr == 0);
    RELEASE(reader);
    RELEASE(ar);
    DeleteFileW(path);
    MFShutdown();
    out("MF lecture : tout est correct\n");
    ExitProcess(0);
}

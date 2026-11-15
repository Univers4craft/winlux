/* Objets de base de Media Foundation depuis un programme 32 bits (#2428,
 * lot 3), construit deux fois par clang sans CRT : en x86 (mf32.exe) et en
 * x64 (mf64.exe).
 * Couvre IMFAttributes (UINT64 et double par valeur, chaîne allouée, BLOB,
 * PROPVARIANT de 16 octets en x86 et 24 en x64, interface rangée puis
 * relue), IMFMediaType, IMFMediaBuffer (pointeur rendu par Lock), le tampon
 * enveloppe et le tampon aligné, IMFSample (LONGLONG par valeur), la
 * collection, l'événement, la file d'événements (lecture directe et rappel
 * IMFAsyncCallback écrit en x86), IMFAsyncResult et MFInvokeCallback, les
 * descripteurs de flux et de présentation (tableaux d'interfaces passés),
 * et la mise en flux des attributs dans un IStream.
 * Rend 0 et écrit « MF : tout est correct » si tout va bien, sinon le
 * numéro de la vérification qui a échoué.
 * Prototypes et numéros de méthodes : mfapi.h, mfobjects.h et mfidl.h de
 * mingw-w64 (domaine public). */
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
IMP void WINAPI Sleep(DWORD);
IMP HRESULT WINAPI CoInitializeEx(void *, DWORD);
IMP void WINAPI CoTaskMemFree(void *);
IMP HRESULT WINAPI CreateStreamOnHGlobal(void *, BOOL, void **);

IMP HRESULT WINAPI MFStartup(DWORD, DWORD);
IMP HRESULT WINAPI MFShutdown(void);
IMP HRESULT WINAPI MFCreateAttributes(void **, UINT);
IMP HRESULT WINAPI MFCreateMediaType(void **);
IMP HRESULT WINAPI MFCreateSample(void **);
IMP HRESULT WINAPI MFCreateMemoryBuffer(DWORD, void **);
IMP HRESULT WINAPI MFCreateAlignedMemoryBuffer(DWORD, DWORD, void **);
IMP HRESULT WINAPI MFCreateMediaBufferWrapper(void *, DWORD, DWORD, void **);
IMP HRESULT WINAPI MFCreateMediaBufferFromMediaType(void *, LONGLONG, DWORD, DWORD, void **);
IMP HRESULT WINAPI MFCreateCollection(void **);
IMP HRESULT WINAPI MFCreateEventQueue(void **);
IMP HRESULT WINAPI MFCreateMediaEvent(DWORD, const GUID *, HRESULT, const void *, void **);
IMP HRESULT WINAPI MFCreateAsyncResult(void *, void *, void *, void **);
IMP HRESULT WINAPI MFInvokeCallback(void *);
IMP HRESULT WINAPI MFCreateStreamDescriptor(DWORD, DWORD, void **, void **);
IMP HRESULT WINAPI MFCreatePresentationDescriptor(DWORD, void **, void **);
IMP HRESULT WINAPI MFSerializeAttributesToStream(void *, DWORD, void *);
IMP HRESULT WINAPI MFDeserializeAttributesFromStream(void *, DWORD, void *);
IMP HRESULT WINAPI MFShutdownObject(void *);

/* propidl.h (extrait public). */
typedef struct {
    WORD vt, r1, r2, r3;
    union {
        LONGLONG align;
        DWORD ulVal;
        WORD *pwszVal;
        struct { DWORD cbSize; BYTE *pBlobData; } blob;
    } u; /* 8 octets en x86, 16 en x64 */
} PROPVARIANT;

typedef struct IUnk { void **vt; } IUnk;
typedef HRESULT(WINAPI *M0)(void *);
typedef HRESULT(WINAPI *M1)(void *, void *);
typedef HRESULT(WINAPI *M2)(void *, void *, void *);
typedef HRESULT(WINAPI *M3)(void *, void *, void *, void *);
typedef HRESULT(WINAPI *M4)(void *, void *, void *, void *, void *);
typedef HRESULT(WINAPI *SetQ)(void *, const GUID *, ULONGLONG);
typedef HRESULT(WINAPI *SetD)(void *, const GUID *, double);
typedef HRESULT(WINAPI *SetLL)(void *, LONGLONG);
typedef HRESULT(WINAPI *Seek)(void *, LONGLONG, DWORD, ULONGLONG *);
#define P(a) (void *)(UINT_PTR)(a)
#define VT(o) (((IUnk *)(o))->vt)
#define CALL0(o, i) ((M0)VT(o)[i])(o)
#define CALL1(o, i, a) ((M1)VT(o)[i])(o, P(a))
#define CALL2(o, i, a, b) ((M2)VT(o)[i])(o, P(a), P(b))
#define CALL3(o, i, a, b, c) ((M3)VT(o)[i])(o, P(a), P(b), P(c))
#define CALL4(o, i, a, b, c, d) ((M4)VT(o)[i])(o, P(a), P(b), P(c), P(d))
#define RELEASE(o) CALL0(o, 2)

enum {
    /* IMFAttributes */
    AT_GETITEM = 3, AT_COMPARE = 6, AT_GETUINT32 = 7, AT_GETUINT64 = 8, AT_GETDOUBLE = 9,
    AT_GETGUID = 10, AT_GETALLOCSTRING = 13, AT_GETBLOBSIZE = 14, AT_GETBLOB = 15,
    AT_GETUNKNOWN = 17, AT_SETITEM = 18, AT_SETUINT32 = 21, AT_SETUINT64 = 22,
    AT_SETDOUBLE = 23, AT_SETGUID = 24, AT_SETSTRING = 25, AT_SETBLOB = 26,
    AT_SETUNKNOWN = 27, AT_GETCOUNT = 30, AT_COPYALL = 32,
    /* IMFMediaType */
    MT_MAJOR = 33, MT_ISEQUAL = 35,
    /* IMFMediaBuffer */
    MB_LOCK = 3, MB_UNLOCK = 4, MB_GETCUR = 5, MB_SETCUR = 6, MB_GETMAX = 7,
    /* IMFSample */
    SA_GETTIME = 35, SA_SETTIME = 36, SA_COUNT = 39, SA_BYINDEX = 40, SA_ADD = 42,
    SA_TOTAL = 45,
    /* IMFCollection */
    CO_COUNT = 3, CO_GET = 4, CO_ADD = 5,
    /* IMFMediaEventQueue */
    EQ_GET = 3, EQ_BEGIN = 4, EQ_END = 5, EQ_QUEUE = 6, EQ_PARAMVAR = 7, EQ_SHUTDOWN = 9,
    /* IMFMediaEvent */
    EV_TYPE = 33, EV_STATUS = 35, EV_VALUE = 36,
    /* IMFAsyncResult */
    AR_STATUS = 4, AR_SETSTATUS = 5, AR_OBJECT = 6,
    /* IMFPresentationDescriptor, IMFStreamDescriptor, IMFMediaTypeHandler */
    PD_COUNT = 33, PD_BYINDEX = 34, PD_CLONE = 37,
    SD_ID = 33, SD_HANDLER = 34,
    TH_COUNT = 4, TH_BYINDEX = 5, TH_MAJOR = 8,
    /* IStream */
    ST_SEEK = 5
};

static const GUID IID_IUnknown = {0, 0, 0, {0xc0, 0, 0, 0, 0, 0, 0, 0x46}};
static const GUID IID_IMFAttributes = {
    0x2cd2d921, 0xc447, 0x44a7, {0xa1, 0x3c, 0x4a, 0xda, 0xbf, 0xc2, 0x47, 0xe3}};
static const GUID IID_IMFMediaType = {
    0x44ae0fa8, 0xea31, 0x4109, {0x8d, 0x2e, 0x4c, 0xae, 0x49, 0x97, 0xc5, 0x55}};
static const GUID IID_IMFAsyncCallback = {
    0xa27003cf, 0x2354, 0x4f2a, {0x8d, 0x6a, 0xab, 0x7c, 0xff, 0x15, 0x43, 0x7e}};
static const GUID GUID_NULL = {0, 0, 0, {0, 0, 0, 0, 0, 0, 0, 0}};
static const GUID MF_MT_MAJOR_TYPE = {
    0x48eba18e, 0xf8c9, 0x4687, {0xbf, 0x11, 0x0a, 0x74, 0xc9, 0xf9, 0x6a, 0x8f}};
static const GUID MF_MT_SAMPLE_SIZE = {
    0xdad3ab78, 0x1990, 0x408b, {0xbc, 0xe2, 0xeb, 0xa6, 0x73, 0xda, 0xcc, 0x10}};
static const GUID MFMediaType_Audio = {
    0x73647561, 0x0000, 0x0010, {0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71}};
/* Clés propres au test. */
static const GUID K_U32 = {0x11111111, 1, 1, {1, 2, 3, 4, 5, 6, 7, 8}};
static const GUID K_U64 = {0x11111111, 1, 2, {1, 2, 3, 4, 5, 6, 7, 8}};
static const GUID K_DBL = {0x11111111, 1, 3, {1, 2, 3, 4, 5, 6, 7, 8}};
static const GUID K_STR = {0x11111111, 1, 4, {1, 2, 3, 4, 5, 6, 7, 8}};
static const GUID K_BLOB = {0x11111111, 1, 5, {1, 2, 3, 4, 5, 6, 7, 8}};
static const GUID K_UNK = {0x11111111, 1, 6, {1, 2, 3, 4, 5, 6, 7, 8}};
static const GUID K_VAR = {0x11111111, 1, 7, {1, 2, 3, 4, 5, 6, 7, 8}};

#define GUARD 0x5a5a5a5au
#define MF_VERSION 0x00020070u

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

static int same(const void *a, const void *b, unsigned n) {
    const BYTE *x = a, *y = b;
    while (n--)
        if (*x++ != *y++) return 0;
    return 1;
}

/* IMFAsyncCallback écrit en x86 : le runtime l'appelle depuis l'autre côté. */
typedef struct Cb {
    void **vt;
    void *queue;
    volatile int invoked;
    volatile DWORD met;
    void *object;
} Cb;
static HRESULT WINAPI cb_qi(Cb *self, const GUID *iid, void **out) {
    if (same(iid, &IID_IUnknown, 16) || same(iid, &IID_IMFAsyncCallback, 16)) {
        *out = self;
        return 0;
    }
    *out = 0;
    return (HRESULT)0x80004002u;
}
static DWORD WINAPI cb_ref(Cb *self) { (void)self; return 1; }
static HRESULT WINAPI cb_params(Cb *self, DWORD *flags, DWORD *queue) {
    (void)self; (void)flags; (void)queue;
    return (HRESULT)0x80004001u;
}
static HRESULT WINAPI cb_invoke(Cb *self, void *result) {
    void *ev = 0, *obj = 0;
    DWORD met = 0;
    if (self->queue) {
        /* File d'événements : EndGetEvent rend l'événement attendu. */
        if (CALL2(self->queue, EQ_END, result, &ev) >= 0 && ev
            && CALL1(ev, EV_TYPE, &met) >= 0)
            self->met = met;
        if (ev) RELEASE(ev);
    } else if (CALL1(result, AR_OBJECT, &obj) >= 0 && obj) {
        /* MFInvokeCallback : l'objet rangé dans le résultat revient. */
        self->object = obj;
    }
    self->invoked++;
    return 0;
}
static void *cb_vt[] = {(void *)cb_qi, (void *)cb_ref, (void *)cb_ref, (void *)cb_params,
                        (void *)cb_invoke};

void entry(void) {
    void *at = 0, *at2 = 0, *unk = 0, *mt = 0, *mt2 = 0, *buf = 0, *buf2 = 0, *wrap = 0;
    void *sa = 0, *got = 0, *coll = 0, *ev = 0, *ev2 = 0, *eq = 0, *ar = 0, *sd = 0;
    void *sd2 = 0, *th = 0, *pd = 0, *pd2 = 0, *stm = 0, *at3 = 0, *at4 = 0;
    UINT u = 0;
    ULONGLONG q = 0;
    double d = 0;
    WORD *s = 0;
    BYTE blob[5] = {9, 8, 7, 6, 5}, back[8];
    BYTE *p = 0, *p2 = 0;
    DWORD max = 0, cur = 0, met = 0, n = 0;
    LONGLONG t = 0;
    GUID g;
    BOOL b = 0, sel = 0;
    HRESULT hr = 0;
    struct { PROPVARIANT v; DWORD guard; } pv;
    Cb cb = {cb_vt, 0, 0, 0, 0};
    Cb cb2 = {cb_vt, 0, 0, 0, 0};
    int i;

    CHECK(1, CoInitializeEx(0, 0) >= 0);
    CHECK(2, MFStartup(MF_VERSION, 0) >= 0);

    /* IMFAttributes */
    CHECK(3, MFCreateAttributes(&at, 4) >= 0 && at != 0);
    CHECK(4, CALL2(at, AT_SETUINT32, &K_U32, 0x12345678) >= 0);
    CHECK(5, CALL2(at, AT_GETUINT32, &K_U32, &u) >= 0 && u == 0x12345678);
    CHECK(6, ((SetQ)VT(at)[AT_SETUINT64])(at, &K_U64, 0x1122334455667788ull) >= 0);
    CHECK(7, CALL2(at, AT_GETUINT64, &K_U64, &q) >= 0 && q == 0x1122334455667788ull);
    CHECK(8, ((SetD)VT(at)[AT_SETDOUBLE])(at, &K_DBL, 2.5) >= 0);
    CHECK(9, CALL2(at, AT_GETDOUBLE, &K_DBL, &d) >= 0 && d == 2.5);
    CHECK(10, CALL2(at, AT_SETSTRING, &K_STR, L"Winlux") >= 0);
    CHECK(11, CALL3(at, AT_GETALLOCSTRING, &K_STR, &s, &u) >= 0 && s != 0 && u == 6
                  && s[0] == 'W' && s[5] == 'x' && s[6] == 0);
    CoTaskMemFree(s);
    CHECK(12, CALL3(at, AT_SETBLOB, &K_BLOB, blob, 5) >= 0);
    CHECK(13, CALL2(at, AT_GETBLOBSIZE, &K_BLOB, &u) >= 0 && u == 5);
    zero(back, sizeof back);
    CHECK(14, CALL4(at, AT_GETBLOB, &K_BLOB, back, 8, &u) >= 0 && u == 5 && same(back, blob, 5));
    /* PROPVARIANT : VT_UI4 posé, puis relu avec sa garde intacte. */
    zero(&pv, sizeof pv);
    pv.v.vt = 19;
    pv.v.u.ulVal = 77;
    CHECK(15, CALL2(at, AT_SETITEM, &K_VAR, &pv.v) >= 0);
    zero(&pv, sizeof pv);
    pv.guard = GUARD;
    CHECK(16, CALL2(at, AT_GETITEM, &K_VAR, &pv.v) >= 0 && pv.v.vt == 19 && pv.v.u.ulVal == 77);
    CHECK(17, pv.guard == GUARD);
    /* Interface rangée puis relue : le second objet revient appelable. */
    CHECK(18, MFCreateAttributes(&at2, 1) >= 0 && at2 != 0);
    CHECK(19, CALL2(at2, AT_SETUINT32, &K_U32, 5) >= 0);
    CHECK(20, CALL2(at, AT_SETUNKNOWN, &K_UNK, at2) >= 0);
    CHECK(21, CALL3(at, AT_GETUNKNOWN, &K_UNK, &IID_IMFAttributes, &unk) >= 0 && unk != 0);
    CHECK(22, CALL2(unk, AT_GETUINT32, &K_U32, &u) >= 0 && u == 5);
    RELEASE(unk);
    CHECK(23, CALL1(at, AT_GETCOUNT, &u) >= 0 && u == 7);
    /* Compare avec une copie (avec lui-même, il se bloque : #2474). */
    CHECK(24, MFCreateAttributes(&at4, 1) >= 0 && CALL1(at2, AT_COPYALL, at4) >= 0
                  && CALL3(at2, AT_COMPARE, at4, 0, &b) >= 0 && b);
    RELEASE(at4);

    /* IMFMediaType */
    CHECK(25, MFCreateMediaType(&mt) >= 0 && mt != 0);
    CHECK(26, CALL2(mt, AT_SETGUID, &MF_MT_MAJOR_TYPE, &MFMediaType_Audio) >= 0);
    zero(&g, sizeof g);
    CHECK(27, CALL1(mt, MT_MAJOR, &g) >= 0 && same(&g, &MFMediaType_Audio, 16));
    zero(&g, sizeof g);
    CHECK(28, CALL2(mt, AT_GETGUID, &MF_MT_MAJOR_TYPE, &g) >= 0
                  && same(&g, &MFMediaType_Audio, 16));
    CHECK(29, CALL1(at2, AT_COPYALL, mt) >= 0);
    CHECK(30, CALL2(mt, AT_GETUINT32, &K_U32, &u) >= 0 && u == 5);
    CHECK(31, MFCreateMediaType(&mt2) >= 0 && CALL1(mt, AT_COPYALL, mt2) >= 0);
    n = 0;
    CHECK(32, CALL2(mt, MT_ISEQUAL, mt2, &n) == 0);

    /* IMFMediaBuffer : Lock rend un pointeur que le code x86 écrit. */
    CHECK(33, MFCreateMemoryBuffer(64, &buf) >= 0 && buf != 0);
    CHECK(34, CALL3(buf, MB_LOCK, &p, &max, &cur) >= 0 && p != 0 && max == 64 && cur == 0);
    for (i = 0; i < 16; i++) p[i] = (BYTE)i;
    CHECK(35, CALL0(buf, MB_UNLOCK) >= 0);
    CHECK(36, CALL1(buf, MB_SETCUR, 16) >= 0);
    CHECK(37, CALL1(buf, MB_GETCUR, &cur) >= 0 && cur == 16);
    CHECK(38, MFCreateMediaBufferWrapper(buf, 4, 8, &wrap) >= 0 && wrap != 0);
    CHECK(39, CALL3(wrap, MB_LOCK, &p2, &max, &cur) >= 0 && p2 == p + 4 && p2[0] == 4
                  && max == 8);
    CHECK(40, CALL0(wrap, MB_UNLOCK) >= 0);
    CHECK(41, MFCreateAlignedMemoryBuffer(100, 15, &buf2) >= 0 && buf2 != 0);
    CHECK(42, CALL1(buf2, MB_GETMAX, &max) >= 0 && max >= 100);
    RELEASE(buf2);
    buf2 = 0;
    /* LONGLONG par valeur (durée) avant deux DWORD. */
    CHECK(43, CALL2(mt2, AT_SETUINT32, &MF_MT_SAMPLE_SIZE, 4096) >= 0);
    CHECK(44, MFCreateMediaBufferFromMediaType(mt2, 10000000, 100, 0, &buf2) >= 0 && buf2 != 0);
    CHECK(45, CALL1(buf2, MB_GETMAX, &max) >= 0 && max == 4096);

    /* IMFSample */
    CHECK(46, MFCreateSample(&sa) >= 0 && sa != 0);
    CHECK(47, CALL1(sa, SA_ADD, buf) >= 0);
    CHECK(48, ((SetLL)VT(sa)[SA_SETTIME])(sa, 0x0000000500000007ll) >= 0);
    CHECK(49, CALL1(sa, SA_GETTIME, &t) >= 0 && t == 0x0000000500000007ll);
    CHECK(50, CALL1(sa, SA_COUNT, &n) >= 0 && n == 1);
    CHECK(51, CALL2(sa, SA_BYINDEX, 0, &got) >= 0 && got != 0);
    CHECK(52, CALL1(got, MB_GETCUR, &cur) >= 0 && cur == 16);
    RELEASE(got);
    CHECK(53, CALL1(sa, SA_TOTAL, &n) >= 0 && n == 16);

    /* IMFCollection : un type de média rangé revient sous IUnknown. */
    CHECK(54, MFCreateCollection(&coll) >= 0 && coll != 0);
    CHECK(55, CALL1(coll, CO_ADD, mt) >= 0);
    CHECK(56, CALL1(coll, CO_COUNT, &n) >= 0 && n == 1);
    CHECK(57, CALL2(coll, CO_GET, 0, &got) >= 0 && got != 0);
    CHECK(58, CALL2(got, 0, &IID_IMFMediaType, &unk) >= 0 && unk != 0);
    zero(&g, sizeof g);
    CHECK(59, CALL1(unk, MT_MAJOR, &g) >= 0 && same(&g, &MFMediaType_Audio, 16));
    RELEASE(unk);
    RELEASE(got);

    /* IMFMediaEvent */
    zero(&pv, sizeof pv);
    pv.v.vt = 19;
    pv.v.u.ulVal = 42;
    CHECK(60, MFCreateMediaEvent(301, &GUID_NULL, 0, &pv.v, &ev) >= 0 && ev != 0);
    CHECK(61, CALL1(ev, EV_TYPE, &met) >= 0 && met == 301);
    CHECK(62, CALL1(ev, EV_STATUS, &hr) >= 0 && hr == 0);
    zero(&pv, sizeof pv);
    pv.guard = GUARD;
    CHECK(63, CALL1(ev, EV_VALUE, &pv.v) >= 0 && pv.v.vt == 19 && pv.v.u.ulVal == 42
                  && pv.guard == GUARD);

    /* IMFMediaEventQueue : lecture directe puis rappel x86. */
    CHECK(64, MFCreateEventQueue(&eq) >= 0 && eq != 0);
    CHECK(65, CALL1(eq, EQ_QUEUE, ev) >= 0);
    CHECK(66, CALL2(eq, EQ_GET, 1, &ev2) >= 0 && ev2 != 0);
    CHECK(67, CALL1(ev2, EV_TYPE, &met) >= 0 && met == 301);
    RELEASE(ev2);
    ev2 = 0;
    zero(&pv, sizeof pv);
    CHECK(68, CALL4(eq, EQ_PARAMVAR, 302, &GUID_NULL, 0, &pv.v) >= 0);
    cb.queue = eq;
    CHECK(69, CALL2(eq, EQ_BEGIN, &cb, 0) >= 0);
    for (i = 0; i < 500 && !cb.invoked; i++) Sleep(10);
    CHECK(70, cb.invoked == 1 && cb.met == 302);

    /* IMFAsyncResult et MFInvokeCallback. */
    CHECK(71, MFCreateAsyncResult(at2, &cb2, 0, &ar) >= 0 && ar != 0);
    CHECK(72, CALL0(ar, AR_STATUS) == 0);
    CHECK(73, MFInvokeCallback(ar) >= 0);
    for (i = 0; i < 500 && !cb2.invoked; i++) Sleep(10);
    CHECK(74, cb2.invoked == 1 && cb2.object != 0);
    /* GetObject rend un IUnknown : QueryInterface avant IMFAttributes. */
    CHECK(75, CALL2(cb2.object, 0, &IID_IMFAttributes, &unk) >= 0 && unk != 0
                  && CALL2(unk, AT_GETUINT32, &K_U32, &u) >= 0 && u == 5);
    RELEASE(unk);
    RELEASE(cb2.object);
    CHECK(76, CALL1(ar, AR_SETSTATUS, 0x80004005u) >= 0
                  && CALL0(ar, AR_STATUS) == (HRESULT)0x80004005u);

    /* Descripteurs : tableaux d'interfaces passés au runtime. */
    CHECK(77, MFCreateStreamDescriptor(7, 1, &mt, &sd) >= 0 && sd != 0);
    CHECK(78, CALL1(sd, SD_ID, &n) >= 0 && n == 7);
    CHECK(79, CALL1(sd, SD_HANDLER, &th) >= 0 && th != 0);
    CHECK(80, CALL1(th, TH_COUNT, &n) >= 0 && n == 1);
    CHECK(81, CALL2(th, TH_BYINDEX, 0, &got) >= 0 && got != 0);
    zero(&g, sizeof g);
    CHECK(82, CALL1(got, MT_MAJOR, &g) >= 0 && same(&g, &MFMediaType_Audio, 16));
    RELEASE(got);
    zero(&g, sizeof g);
    CHECK(83, CALL1(th, TH_MAJOR, &g) >= 0 && same(&g, &MFMediaType_Audio, 16));
    CHECK(84, MFCreatePresentationDescriptor(1, &sd, &pd) >= 0 && pd != 0);
    CHECK(85, CALL1(pd, PD_COUNT, &n) >= 0 && n == 1);
    CHECK(86, CALL3(pd, PD_BYINDEX, 0, &sel, &sd2) >= 0 && sd2 != 0);
    CHECK(87, CALL1(sd2, SD_ID, &n) >= 0 && n == 7);
    CHECK(88, CALL1(pd, PD_CLONE, &pd2) >= 0 && pd2 != 0);
    CHECK(89, CALL1(pd2, PD_COUNT, &n) >= 0 && n == 1);

    /* Attributs mis en flux dans un IStream, puis relus. */
    CHECK(90, CreateStreamOnHGlobal(0, 1, &stm) >= 0 && stm != 0);
    CHECK(91, MFSerializeAttributesToStream(at2, 0, stm) >= 0);
    CHECK(92, ((Seek)VT(stm)[ST_SEEK])(stm, 0, 0, 0) >= 0);
    CHECK(93, MFCreateAttributes(&at3, 1) >= 0 && at3 != 0);
    CHECK(94, MFDeserializeAttributesFromStream(at3, 0, stm) >= 0);
    CHECK(95, CALL2(at3, AT_GETUINT32, &K_U32, &u) >= 0 && u == 5);
    CHECK(96, MFShutdownObject(eq) >= 0);
    CHECK(97, CALL0(eq, EQ_SHUTDOWN) >= 0);

    RELEASE(stm);
    RELEASE(at3);
    RELEASE(pd2);
    RELEASE(pd);
    RELEASE(sd2);
    RELEASE(th);
    RELEASE(sd);
    RELEASE(ar);
    RELEASE(eq);
    RELEASE(ev);
    RELEASE(coll);
    RELEASE(sa);
    RELEASE(buf2);
    RELEASE(wrap);
    RELEASE(buf);
    RELEASE(mt2);
    RELEASE(mt);
    RELEASE(at2);
    RELEASE(at);
    CHECK(98, MFShutdown() >= 0);
    out("MF : tout est correct\n");
    ExitProcess(0);
}

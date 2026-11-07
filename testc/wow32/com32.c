/* COM dans un programme Windows 32 bits (#232, étape 6).
 * MinGW i686, en C : CoInitialize, GUID, IStream du runtime (Write, Seek
 * à LARGE_INTEGER, Stat, Clone, CopyTo vers un flux x86), identité COM,
 * IMalloc, fabrique de classes x86 déposée par CoRegisterClassObject puis
 * rendue par CoCreateInstance, BSTR, VARIANT (16 octets en x86) et
 * IErrorInfo. Rend 0 si tout va bien, sinon le numéro de la vérification
 * en échec. */
#define COBJMACROS
#define INITGUID
#include <windows.h>
#include <objbase.h>
#include <oleauto.h>
#include <stdio.h>
#include <string.h>

static int step;
#define CHECK(c, what) do { step++; if (!(c)) { \
    printf("com32 : vérification %d en échec : %s\n", step, what); \
    fflush(stdout); ExitProcess(step); } } while (0)

/* ---- Objet x86 : ITest, créé par une fabrique x86 ---- */

DEFINE_GUID(CLSID_WinluxTest, 0x5d1c8a10, 0x2b3e, 0x4f6a, 0x9c, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77);
DEFINE_GUID(IID_ITest, 0x5d1c8a11, 0x2b3e, 0x4f6a, 0x9c, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77);
DEFINE_GUID(IID_Inconnue, 0x01020304, 0x0506, 0x0708, 1, 2, 3, 4, 5, 6, 7, 8);

typedef struct ITest ITest;
typedef struct {
    HRESULT (STDMETHODCALLTYPE *QueryInterface)(ITest *, REFIID, void **);
    ULONG (STDMETHODCALLTYPE *AddRef)(ITest *);
    ULONG (STDMETHODCALLTYPE *Release)(ITest *);
    int (STDMETHODCALLTYPE *Add)(ITest *, int, int);
} ITestVtbl;
struct ITest { const ITestVtbl *lpVtbl; LONG refs; };

static HRESULT STDMETHODCALLTYPE test_qi(ITest *t, REFIID iid, void **out)
{
    if (IsEqualGUID(iid, &IID_IUnknown) || IsEqualGUID(iid, &IID_ITest)) {
        *out = t;
        t->refs++;
        return S_OK;
    }
    *out = NULL;
    return E_NOINTERFACE;
}
static ULONG STDMETHODCALLTYPE test_addref(ITest *t) { return ++t->refs; }
static ULONG STDMETHODCALLTYPE test_release(ITest *t) { return --t->refs; }
static int STDMETHODCALLTYPE test_add(ITest *t, int a, int b) { (void)t; return a + b; }
static const ITestVtbl test_vtbl = { test_qi, test_addref, test_release, test_add };
static ITest the_test = { &test_vtbl, 0 };

static int factory_creates;
static HRESULT STDMETHODCALLTYPE cf_qi(IClassFactory *f, REFIID iid, void **out)
{
    if (IsEqualGUID(iid, &IID_IUnknown) || IsEqualGUID(iid, &IID_IClassFactory)) {
        *out = f;
        return S_OK;
    }
    *out = NULL;
    return E_NOINTERFACE;
}
static ULONG STDMETHODCALLTYPE cf_addref(IClassFactory *f) { (void)f; return 2; }
static ULONG STDMETHODCALLTYPE cf_release(IClassFactory *f) { (void)f; return 1; }
static HRESULT STDMETHODCALLTYPE cf_create(IClassFactory *f, IUnknown *outer, REFIID iid, void **out)
{
    (void)f;
    factory_creates++;
    if (outer)
        return CLASS_E_NOAGGREGATION;
    return test_qi(&the_test, iid, out);
}
static HRESULT STDMETHODCALLTYPE cf_lock(IClassFactory *f, BOOL l) { (void)f; (void)l; return S_OK; }
static IClassFactoryVtbl cf_vtbl = { cf_qi, cf_addref, cf_release, cf_create, cf_lock };
static IClassFactory the_factory = { &cf_vtbl };

/* ---- Flux x86 qui reçoit les octets de IStream::CopyTo ---- */

static char sink_data[64];
static ULONG sink_len;
static HRESULT STDMETHODCALLTYPE sink_qi(IStream *s, REFIID iid, void **out)
{
    if (IsEqualGUID(iid, &IID_IUnknown) || IsEqualGUID(iid, &IID_IStream)
        || IsEqualGUID(iid, &IID_ISequentialStream)) {
        *out = s;
        return S_OK;
    }
    *out = NULL;
    return E_NOINTERFACE;
}
static ULONG STDMETHODCALLTYPE sink_ref(IStream *s) { (void)s; return 1; }
static HRESULT STDMETHODCALLTYPE sink_read(IStream *s, void *p, ULONG n, ULONG *got)
{ (void)s; (void)p; (void)n; if (got) *got = 0; return S_FALSE; }
static HRESULT STDMETHODCALLTYPE sink_write(IStream *s, const void *p, ULONG n, ULONG *done)
{
    (void)s;
    if (sink_len + n > sizeof sink_data)
        return STG_E_MEDIUMFULL;
    memcpy(sink_data + sink_len, p, n);
    sink_len += n;
    if (done)
        *done = n;
    return S_OK;
}
static HRESULT STDMETHODCALLTYPE sink_seek(IStream *s, LARGE_INTEGER m, DWORD o, ULARGE_INTEGER *p)
{ (void)s; (void)m; (void)o; if (p) p->QuadPart = sink_len; return S_OK; }
static HRESULT STDMETHODCALLTYPE sink_notimpl(IStream *s) { (void)s; return E_NOTIMPL; }
static IStreamVtbl sink_vtbl = {
    sink_qi, sink_ref, sink_ref, sink_read, sink_write, sink_seek,
    (void *)sink_notimpl, (void *)sink_notimpl, (void *)sink_notimpl, (void *)sink_notimpl,
    (void *)sink_notimpl, (void *)sink_notimpl, (void *)sink_notimpl, (void *)sink_notimpl,
};
static IStream the_sink = { &sink_vtbl };

int main(void)
{
    HRESULT hr = CoInitialize(NULL);
    CHECK(hr == S_OK || hr == S_FALSE, "CoInitialize");

    /* GUID ↔ texte. */
    GUID g;
    CHECK(CoCreateGuid(&g) == S_OK, "CoCreateGuid");
    WCHAR text[40];
    CHECK(StringFromGUID2(&g, text, 40) == 39, "StringFromGUID2");
    GUID back;
    CHECK(CLSIDFromString(text, &back) == S_OK && IsEqualGUID(&g, &back), "CLSIDFromString");
    LPOLESTR s = NULL;
    /* Case de 4 octets voisine : StringFromCLSID ne doit pas l'écraser. */
    struct { LPOLESTR p; DWORD guard; } cell = { NULL, 0xCAFEBABE };
    CHECK(StringFromCLSID(&g, &cell.p) == S_OK && cell.guard == 0xCAFEBABE, "StringFromCLSID (case 32 bits)");
    s = cell.p;
    CHECK(s && lstrcmpW(s, text) == 0, "StringFromCLSID (texte)");
    CoTaskMemFree(s);

    /* IStream du runtime. */
    struct { IStream *p; DWORD guard; } st = { NULL, 0x12345678 };
    CHECK(CreateStreamOnHGlobal(NULL, TRUE, &st.p) == S_OK && st.p && st.guard == 0x12345678,
          "CreateStreamOnHGlobal");
    IStream *stm = st.p;
    ULONG done = 0;
    CHECK(IStream_Write(stm, "Bonjour", 7, &done) == S_OK && done == 7, "IStream::Write");
    LARGE_INTEGER zero; zero.QuadPart = 0;
    LARGE_INTEGER two; two.QuadPart = 2;
    ULARGE_INTEGER pos; pos.QuadPart = 99;
    CHECK(IStream_Seek(stm, two, STREAM_SEEK_SET, &pos) == S_OK && pos.QuadPart == 2,
          "IStream::Seek (LARGE_INTEGER par valeur)");
    char buf[16] = {0};
    CHECK(IStream_Read(stm, buf, 5, &done) == S_OK && done == 5 && memcmp(buf, "njour", 5) == 0,
          "IStream::Read");
    STATSTG stat;
    memset(&stat, 0xAB, sizeof stat);
    CHECK(IStream_Stat(stm, &stat, STATFLAG_NONAME) == S_OK, "IStream::Stat");
    CHECK(stat.type == STGTY_STREAM && stat.cbSize.QuadPart == 7, "STATSTG converti");
    IStream *clone = NULL;
    CHECK(IStream_Clone(stm, &clone) == S_OK && clone, "IStream::Clone");
    CHECK(IStream_Seek(clone, zero, STREAM_SEEK_CUR, &pos) == S_OK && pos.QuadPart == 7, "clone à la même position");
    IStream_Release(clone);
    ULARGE_INTEGER sz; sz.QuadPart = 10;
    CHECK(IStream_SetSize(stm, sz) == S_OK, "IStream::SetSize");
    CHECK(IStream_Stat(stm, &stat, STATFLAG_NONAME) == S_OK && stat.cbSize.QuadPart == 10, "taille 10");
    HGLOBAL hg = NULL;
    CHECK(GetHGlobalFromStream(stm, &hg) == S_OK && hg, "GetHGlobalFromStream");
    char *mem = GlobalLock(hg);
    CHECK(mem && memcmp(mem, "Bonjour", 7) == 0, "GlobalLock du flux");
    GlobalUnlock(hg);

    /* CopyTo : le runtime écrit dans un flux x86. */
    CHECK(IStream_Seek(stm, zero, STREAM_SEEK_SET, NULL) == S_OK, "Seek à 0 (sans position)");
    ULARGE_INTEGER seven; seven.QuadPart = 7;
    ULARGE_INTEGER rd, wr;
    CHECK(IStream_CopyTo(stm, &the_sink, seven, &rd, &wr) == S_OK, "IStream::CopyTo vers un flux x86");
    CHECK(sink_len == 7 && memcmp(sink_data, "Bonjour", 7) == 0 && wr.QuadPart == 7, "octets reçus par le flux x86");

    /* Identité COM et interfaces refusées. */
    IUnknown *u1 = NULL, *u2 = NULL;
    CHECK(IStream_QueryInterface(stm, &IID_IUnknown, (void **)&u1) == S_OK, "QI IUnknown");
    CHECK(IUnknown_QueryInterface(u1, &IID_IUnknown, (void **)&u2) == S_OK && u1 == u2, "identité IUnknown");
    IUnknown_Release(u2);
    IUnknown_Release(u1);
    ISequentialStream *seq = NULL;
    CHECK(IStream_QueryInterface(stm, &IID_ISequentialStream, (void **)&seq) == S_OK && seq, "QI ISequentialStream");
    CHECK(ISequentialStream_Write(seq, "!", 1, NULL) == S_OK, "ISequentialStream::Write");
    ISequentialStream_Release(seq);
    void *nope = (void *)1;
    CHECK(IStream_QueryInterface(stm, &IID_Inconnue, &nope) == E_NOINTERFACE && nope == NULL, "QI refusée");
    IStream_Release(stm);

    /* IMalloc. */
    IMalloc *ma = NULL;
    CHECK(CoGetMalloc(1, &ma) == S_OK && ma, "CoGetMalloc");
    void *blk = IMalloc_Alloc(ma, 100);
    CHECK(blk && (DWORD)(ULONG_PTR)blk < 0x80000000u, "IMalloc::Alloc sous 2 Go");
    memset(blk, 1, 100);
    IMalloc_Free(ma, blk);
    void *t = CoTaskMemAlloc(32);
    CHECK(t != NULL, "CoTaskMemAlloc");
    CoTaskMemFree(t);

    /* Fabrique de classes x86. */
    DWORD cookie = 0;
    CHECK(CoRegisterClassObject(&CLSID_WinluxTest, (IUnknown *)&the_factory, CLSCTX_INPROC_SERVER,
                                REGCLS_MULTIPLEUSE, &cookie) == S_OK && cookie, "CoRegisterClassObject");
    ITest *obj = NULL;
    CHECK(CoCreateInstance(&CLSID_WinluxTest, NULL, CLSCTX_INPROC_SERVER, &IID_ITest, (void **)&obj) == S_OK,
          "CoCreateInstance (classe x86)");
    CHECK(obj == &the_test && factory_creates == 1, "objet x86 rendu tel quel");
    CHECK(obj->lpVtbl->Add(obj, 2, 3) == 5, "méthode de l'objet x86");
    obj->lpVtbl->Release(obj);
    IClassFactory *cf = NULL;
    CHECK(CoGetClassObject(&CLSID_WinluxTest, CLSCTX_INPROC_SERVER, NULL, &IID_IClassFactory, (void **)&cf) == S_OK
          && cf == &the_factory, "CoGetClassObject");
    CHECK(CoRevokeClassObject(cookie) == S_OK, "CoRevokeClassObject");
    CHECK(CoCreateInstance(&CLSID_WinluxTest, NULL, CLSCTX_INPROC_SERVER, &IID_ITest, (void **)&obj)
          == REGDB_E_CLASSNOTREG, "classe retirée");

    /* BSTR et VARIANT. */
    BSTR b = SysAllocString(L"Winlux");
    CHECK(b && SysStringLen(b) == 6, "SysAllocString");
    struct { VARIANT v; DWORD guard[4]; } vv;
    memset(&vv, 0x5A, sizeof vv);
    CHECK(sizeof(VARIANT) == 16, "VARIANT de 16 octets");
    VariantInit(&vv.v);
    CHECK(V_VT(&vv.v) == VT_EMPTY && vv.guard[0] == 0x5A5A5A5A, "VariantInit sans débordement");
    V_VT(&vv.v) = VT_I4;
    V_I4(&vv.v) = 42;
    VARIANT str;
    VariantInit(&str);
    CHECK(VariantChangeType(&str, &vv.v, 0, VT_BSTR) == S_OK && V_VT(&str) == VT_BSTR, "VariantChangeType");
    CHECK(lstrcmpW(V_BSTR(&str), L"42") == 0, "42 en texte");
    VARIANT copy;
    VariantInit(&copy);
    CHECK(VariantCopy(&copy, &str) == S_OK && V_VT(&copy) == VT_BSTR && lstrcmpW(V_BSTR(&copy), L"42") == 0,
          "VariantCopy");
    CHECK(VariantClear(&copy) == S_OK && V_VT(&copy) == VT_EMPTY, "VariantClear");
    VariantClear(&str);
    SysFreeString(b);

    /* IErrorInfo. */
    ICreateErrorInfo *cei = NULL;
    CHECK(CreateErrorInfo(&cei) == S_OK && cei, "CreateErrorInfo");
    CHECK(ICreateErrorInfo_SetDescription(cei, L"Erreur de test") == S_OK, "SetDescription");
    IErrorInfo *ei = NULL;
    CHECK(ICreateErrorInfo_QueryInterface(cei, &IID_IErrorInfo, (void **)&ei) == S_OK && ei, "QI IErrorInfo");
    CHECK(SetErrorInfo(0, ei) == S_OK, "SetErrorInfo");
    IErrorInfo *got = NULL;
    CHECK(GetErrorInfo(0, &got) == S_OK && got, "GetErrorInfo");
    BSTR desc = NULL;
    CHECK(IErrorInfo_GetDescription(got, &desc) == S_OK && desc && lstrcmpW(desc, L"Erreur de test") == 0,
          "GetDescription");
    SysFreeString(desc);
    IErrorInfo_Release(got);
    IErrorInfo_Release(ei);
    ICreateErrorInfo_Release(cei);

    CoUninitialize();
    printf("com32 : COM 32 bits, %d vérifications passées. Tout est correct\n", step);
    return 0;
}

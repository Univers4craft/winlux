/* Serveur COM en processus (#16), compilé par MinGW-w64 en DLL.
 *
 * Exporte DllGetClassObject et DllCanUnloadNow ; la classe ne connaît que
 * IUnknown et ICalc (une addition). com_suite.exe l'inscrit sous
 * HKCU\Software\Classes\CLSID\{…}\InprocServer32 et l'active par le
 * registre. Au déchargement (DLL_PROCESS_DETACH), la DLL le signale par la
 * variable d'environnement WINLUX_COM_DETACHED.
 */
#define COBJMACROS
#include <windows.h>
#include <objbase.h>

/* {776C0016-0001-4E00-9C00-57494E4C5558} */
static const CLSID CLSID_Calc = {0x776c0016, 0x0001, 0x4e00, {0x9c, 0x00, 0x57, 0x49, 0x4e, 0x4c, 0x55, 0x58}};
/* {776C0016-0002-4E00-9C00-57494E4C5558} */
static const IID IID_ICalc = {0x776c0016, 0x0002, 0x4e00, {0x9c, 0x00, 0x57, 0x49, 0x4e, 0x4c, 0x55, 0x58}};

static LONG objects;
static LONG locks;

typedef struct Calc Calc;
typedef struct {
    HRESULT(STDMETHODCALLTYPE *QueryInterface)(Calc *, REFIID, void **);
    ULONG(STDMETHODCALLTYPE *AddRef)(Calc *);
    ULONG(STDMETHODCALLTYPE *Release)(Calc *);
    HRESULT(STDMETHODCALLTYPE *Add)(Calc *, int, int, int *);
} CalcVtbl;
struct Calc {
    const CalcVtbl *vtbl;
    LONG refs;
};

static HRESULT STDMETHODCALLTYPE calc_qi(Calc *self, REFIID iid, void **out)
{
    if (!out)
        return E_POINTER;
    if (IsEqualIID(iid, &IID_IUnknown) || IsEqualIID(iid, &IID_ICalc)) {
        InterlockedIncrement(&self->refs);
        *out = self;
        return S_OK;
    }
    *out = NULL;
    return E_NOINTERFACE;
}
static ULONG STDMETHODCALLTYPE calc_addref(Calc *self) { return InterlockedIncrement(&self->refs); }
static ULONG STDMETHODCALLTYPE calc_release(Calc *self)
{
    LONG r = InterlockedDecrement(&self->refs);
    if (r == 0) {
        HeapFree(GetProcessHeap(), 0, self);
        InterlockedDecrement(&objects);
    }
    return r;
}
static HRESULT STDMETHODCALLTYPE calc_add(Calc *self, int a, int b, int *r)
{
    (void)self;
    if (!r)
        return E_POINTER;
    *r = a + b;
    return S_OK;
}
static const CalcVtbl calc_vtbl = {calc_qi, calc_addref, calc_release, calc_add};

static HRESULT STDMETHODCALLTYPE cf_qi(IClassFactory *self, REFIID iid, void **out)
{
    if (!out)
        return E_POINTER;
    if (IsEqualIID(iid, &IID_IUnknown) || IsEqualIID(iid, &IID_IClassFactory)) {
        *out = self;
        InterlockedIncrement(&locks);
        return S_OK;
    }
    *out = NULL;
    return E_NOINTERFACE;
}
static ULONG STDMETHODCALLTYPE cf_addref(IClassFactory *self) { (void)self; return InterlockedIncrement(&locks) + 1; }
static ULONG STDMETHODCALLTYPE cf_release(IClassFactory *self) { (void)self; return InterlockedDecrement(&locks) + 1; }
static HRESULT STDMETHODCALLTYPE cf_create(IClassFactory *self, IUnknown *outer, REFIID iid, void **out)
{
    (void)self;
    if (!out)
        return E_POINTER;
    *out = NULL;
    if (outer)
        return CLASS_E_NOAGGREGATION;
    Calc *c = HeapAlloc(GetProcessHeap(), 0, sizeof *c);
    if (!c)
        return E_OUTOFMEMORY;
    c->vtbl = &calc_vtbl;
    c->refs = 1;
    InterlockedIncrement(&objects);
    HRESULT hr = calc_qi(c, iid, out);
    calc_release(c);
    return hr;
}
static HRESULT STDMETHODCALLTYPE cf_lock(IClassFactory *self, BOOL lock)
{
    (void)self;
    if (lock)
        InterlockedIncrement(&locks);
    else
        InterlockedDecrement(&locks);
    return S_OK;
}
static IClassFactoryVtbl cf_vtbl = {cf_qi, cf_addref, cf_release, cf_create, cf_lock};
static IClassFactory factory = {&cf_vtbl};

__declspec(dllexport) HRESULT WINAPI DllGetClassObject(REFCLSID clsid, REFIID iid, void **out)
{
    if (!out)
        return E_POINTER;
    *out = NULL;
    if (!IsEqualCLSID(clsid, &CLSID_Calc))
        return CLASS_E_CLASSNOTAVAILABLE;
    return cf_qi(&factory, iid, out);
}

__declspec(dllexport) HRESULT WINAPI DllCanUnloadNow(void)
{
    return objects == 0 && locks == 0 ? S_OK : S_FALSE;
}

BOOL WINAPI DllMain(HINSTANCE dll, DWORD reason, LPVOID reserved)
{
    (void)dll;
    (void)reserved;
    if (reason == DLL_PROCESS_DETACH)
        SetEnvironmentVariableW(L"WINLUX_COM_DETACHED", L"1");
    return TRUE;
}

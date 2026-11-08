/* Serveur COM hors processus (LocalServer32, #16), compilé par MinGW-w64.
 *
 *   com_local_suite.exe              client : inscrit LocalServer32 (lui-même)
 *                                    sous HKCU, CoCreateInstance(CLSCTX_LOCAL_SERVER)
 *                                    lance le serveur, puis appels IDispatch ;
 *   com_local_suite.exe -Embedding   serveur : CoRegisterClassObject(LOCAL_SERVER),
 *                                    s'arrête quand son objet est libéré.
 *
 * L'objet du serveur est un IDispatch standard (CreateDispTypeInfo +
 * CreateStdDispatch agrégé). Code de sortie 0 et « COM hors processus :
 * tout est correct » si tout va bien.
 */
#define COBJMACROS
#include <windows.h>
#include <oleauto.h>
#include <stdio.h>
#include <string.h>

static const CLSID CLSID_Remote = {0x776c0016, 0x0020, 0x4e00, {0x9c, 0, 0x57, 0x49, 0x4e, 0x4c, 0x55, 0x58}};
#define REMOTE_KEY L"Software\\Classes\\CLSID\\{776C0016-0020-4E00-9C00-57494E4C5558}"

/* ── Serveur ── */
typedef struct Obj Obj;
typedef struct {
    HRESULT(STDMETHODCALLTYPE *QueryInterface)(Obj *, REFIID, void **);
    ULONG(STDMETHODCALLTYPE *AddRef)(Obj *);
    ULONG(STDMETHODCALLTYPE *Release)(Obj *);
    int(STDMETHODCALLTYPE *Pid)(Obj *);
    BSTR(STDMETHODCALLTYPE *Repeat)(Obj *, BSTR, int);
    HRESULT(STDMETHODCALLTYPE *Fail)(Obj *);
} ObjVtbl;
struct Obj {
    const ObjVtbl *vtbl;
    LONG refs;
    IUnknown *inner;
};
static HANDLE done;
static ITypeInfo *tinfo;

static HRESULT STDMETHODCALLTYPE o_qi(Obj *o, REFIID iid, void **out)
{
    if (IsEqualIID(iid, &IID_IUnknown)) {
        *out = o;
        InterlockedIncrement(&o->refs);
        return S_OK;
    }
    if (IsEqualIID(iid, &IID_IDispatch))
        return IUnknown_QueryInterface(o->inner, iid, out);
    *out = NULL;
    return E_NOINTERFACE;
}
static ULONG STDMETHODCALLTYPE o_addref(Obj *o) { return InterlockedIncrement(&o->refs); }
static ULONG STDMETHODCALLTYPE o_release(Obj *o)
{
    LONG r = InterlockedDecrement(&o->refs);
    if (r == 0) {
        IUnknown_Release(o->inner);
        SetEvent(done);
    }
    return r;
}
static int STDMETHODCALLTYPE o_pid(Obj *o) { (void)o; return (int)GetCurrentProcessId(); }
static BSTR STDMETHODCALLTYPE o_repeat(Obj *o, BSTR s, int n)
{
    (void)o;
    wchar_t buf[256] = L"";
    for (int i = 0; i < n && wcslen(buf) + wcslen(s) < 255; i++)
        wcscat(buf, s);
    return SysAllocString(buf);
}
static HRESULT STDMETHODCALLTYPE o_fail(Obj *o) { (void)o; return E_ACCESSDENIED; }
static const ObjVtbl obj_vtbl = {o_qi, o_addref, o_release, o_pid, o_repeat, o_fail};
static Obj the_obj = {&obj_vtbl, 0, NULL};

static PARAMDATA repeat_params[] = {{L"Text", VT_BSTR}, {L"Count", VT_I4}};
static METHODDATA methods[] = {
    {L"Pid", NULL, 1, 3, CC_STDCALL, 0, DISPATCH_METHOD | DISPATCH_PROPERTYGET, VT_I4},
    {L"Repeat", repeat_params, 2, 4, CC_STDCALL, 2, DISPATCH_METHOD, VT_BSTR},
    {L"Fail", NULL, 3, 5, CC_STDCALL, 0, DISPATCH_METHOD, VT_HRESULT},
};
static INTERFACEDATA idata = {methods, 3};

static HRESULT STDMETHODCALLTYPE cf_qi(IClassFactory *self, REFIID iid, void **out)
{
    if (IsEqualIID(iid, &IID_IUnknown) || IsEqualIID(iid, &IID_IClassFactory)) {
        *out = self;
        return S_OK;
    }
    *out = NULL;
    return E_NOINTERFACE;
}
static ULONG STDMETHODCALLTYPE cf_addref(IClassFactory *self) { (void)self; return 2; }
static ULONG STDMETHODCALLTYPE cf_release(IClassFactory *self) { (void)self; return 1; }
static HRESULT STDMETHODCALLTYPE cf_create(IClassFactory *self, IUnknown *outer, REFIID iid, void **out)
{
    (void)self;
    *out = NULL;
    if (outer)
        return CLASS_E_NOAGGREGATION;
    if (the_obj.inner)
        return E_UNEXPECTED; /* un seul objet dans ce test */
    the_obj.refs = 1;
    HRESULT hr = CreateStdDispatch((IUnknown *)&the_obj, &the_obj, tinfo, &the_obj.inner);
    if (FAILED(hr))
        return hr;
    hr = o_qi(&the_obj, iid, out);
    o_release(&the_obj);
    return hr;
}
static HRESULT STDMETHODCALLTYPE cf_lock(IClassFactory *self, BOOL lock) { (void)self; (void)lock; return S_OK; }
static IClassFactoryVtbl cf_vtbl = {cf_qi, cf_addref, cf_release, cf_create, cf_lock};
static IClassFactory factory = {&cf_vtbl};

static int server(void)
{
    done = CreateEventW(NULL, TRUE, FALSE, NULL);
    CoInitializeEx(NULL, COINIT_MULTITHREADED);
    if (FAILED(CreateDispTypeInfo(&idata, LOCALE_USER_DEFAULT, &tinfo)))
        return 2;
    DWORD cookie = 0;
    HRESULT hr = CoRegisterClassObject(&CLSID_Remote, (IUnknown *)&factory, CLSCTX_LOCAL_SERVER,
                                       REGCLS_MULTIPLEUSE, &cookie);
    if (FAILED(hr))
        return 3;
    DWORD w = WaitForSingleObject(done, 60000);
    CoRevokeClassObject(cookie);
    ITypeInfo_Release(tinfo);
    CoUninitialize();
    return w == WAIT_OBJECT_0 ? 0 : 4;
}

/* ── Client ── */
static int failures;
static void check(int ok, const char *what, HRESULT hr)
{
    if (!ok) {
        printf("ÉCHEC : %s (0x%08lx)\n", what, (unsigned long)hr);
        failures++;
    }
}

static DISPID id_of(IDispatch *d, const wchar_t *name)
{
    LPOLESTR n = (LPOLESTR)name;
    DISPID id = DISPID_UNKNOWN;
    IDispatch_GetIDsOfNames(d, &IID_NULL, &n, 1, LOCALE_USER_DEFAULT, &id);
    return id;
}

int main(int argc, char **argv)
{
    for (int i = 1; i < argc; i++)
        if (_stricmp(argv[i], "-Embedding") == 0 || _stricmp(argv[i], "/Embedding") == 0)
            return server();

    wchar_t self[MAX_PATH], line[MAX_PATH + 4];
    GetModuleFileNameW(NULL, self, MAX_PATH);
    swprintf(line, MAX_PATH + 4, L"\"%ls\"", self);
    HKEY k;
    check(RegCreateKeyExW(HKEY_CURRENT_USER, REMOTE_KEY L"\\LocalServer32", 0, NULL, 0, KEY_ALL_ACCESS, NULL, &k,
                          NULL) == ERROR_SUCCESS,
          "inscription LocalServer32", 0);
    RegSetValueExW(k, NULL, 0, REG_SZ, (const BYTE *)line, (DWORD)((wcslen(line) + 1) * 2));
    RegCloseKey(k);

    CoInitializeEx(NULL, COINIT_APARTMENTTHREADED);
    IUnknown *none = (IUnknown *)1;
    HRESULT hr = CoCreateInstance(&CLSID_Remote, NULL, CLSCTX_INPROC_SERVER, &IID_IUnknown, (void **)&none);
    check(hr == REGDB_E_CLASSNOTREG && none == NULL, "pas de serveur en processus : REGDB_E_CLASSNOTREG", hr);

    IDispatch *d = NULL;
    hr = CoCreateInstance(&CLSID_Remote, NULL, CLSCTX_LOCAL_SERVER, &IID_IDispatch, (void **)&d);
    check(hr == S_OK && d, "CoCreateInstance(CLSCTX_LOCAL_SERVER) lance le serveur", hr);
    if (!d)
        return 1;

    DISPID pid = id_of(d, L"pid"), repeat = id_of(d, L"Repeat"), fail = id_of(d, L"Fail");
    check(pid == 1 && repeat == 2 && fail == 3, "GetIDsOfNames à travers les processus", 0);

    VARIANT r;
    VariantInit(&r);
    DISPPARAMS none_args = {NULL, NULL, 0, 0};
    hr = IDispatch_Invoke(d, pid, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_PROPERTYGET, &none_args, &r, NULL, NULL);
    check(hr == S_OK && r.vt == VT_I4 && r.lVal != 0 && (DWORD)r.lVal != GetCurrentProcessId(),
          "l'objet vit dans un autre processus", hr);

    VARIANT args[2];
    args[1].vt = VT_BSTR; args[1].bstrVal = SysAllocString(L"é-");
    args[0].vt = VT_I2; args[0].iVal = 3;
    DISPPARAMS dp = {args, NULL, 2, 0};
    hr = IDispatch_Invoke(d, repeat, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_METHOD, &dp, &r, NULL, NULL);
    check(hr == S_OK && r.vt == VT_BSTR && wcscmp(r.bstrVal, L"é-é-é-") == 0, "BSTR aller et retour", hr);
    VariantClear(&r);
    VariantClear(&args[1]);

    EXCEPINFO ex;
    memset(&ex, 0, sizeof ex);
    hr = IDispatch_Invoke(d, fail, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_METHOD, &none_args, &r, &ex, NULL);
    check(hr == DISP_E_EXCEPTION && ex.scode == E_ACCESSDENIED, "exception du serveur rapportée", hr);

    args[0].vt = VT_DISPATCH; args[0].pdispVal = d;
    DISPPARAMS obj_arg = {args, NULL, 1, 0};
    hr = IDispatch_Invoke(d, repeat, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_METHOD, &obj_arg, &r, NULL, NULL);
    check(hr == DISP_E_BADVARTYPE, "objet en argument refusé : DISP_E_BADVARTYPE", hr);

    IDispatch_Release(d);
    CoUninitialize();
    RegDeleteTreeW(HKEY_CURRENT_USER, REMOTE_KEY);
    if (failures)
        return 1;
    printf("COM hors processus : tout est correct\n");
    return 0;
}

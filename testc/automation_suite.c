/* OLE Automation (#16), compilé par MinGW-w64.
 *
 * Un objet C décrit par INTERFACEDATA devient un IDispatch par
 * CreateDispTypeInfo + CreateStdDispatch ; on l'appelle ensuite par
 * GetIDsOfNames et Invoke : conversion d'arguments (VT_I2 → VT_I4, chaîne
 * → double), arguments nommés, propriété get/put en BSTR, HRESULT d'échec
 * rendu en DISP_E_EXCEPTION, mauvais nombre d'arguments, nom inconnu.
 * Puis DispCallFunc sur une fonction libre (entiers et doubles mêlés) et
 * la table des objets actifs (RegisterActiveObject, GetActiveObject,
 * RevokeActiveObject).
 *
 * Code de sortie 0 et « Automation OLE : tout est correct » si tout va bien.
 */
#define COBJMACROS
#include <windows.h>
#include <oleauto.h>
#include <stdio.h>

static int failures;
static void check(int ok, const char *what, HRESULT hr)
{
    if (!ok) {
        printf("ÉCHEC : %s (0x%08lx)\n", what, (unsigned long)hr);
        failures++;
    }
}

/* L'objet : une vtable C ordinaire, sans IDispatch à lui. */
typedef struct Obj Obj;
typedef struct {
    HRESULT(STDMETHODCALLTYPE *QueryInterface)(Obj *, REFIID, void **);
    ULONG(STDMETHODCALLTYPE *AddRef)(Obj *);
    ULONG(STDMETHODCALLTYPE *Release)(Obj *);
    int(STDMETHODCALLTYPE *Sub)(Obj *, int, int);
    double(STDMETHODCALLTYPE *Scale)(Obj *, double, int);
    BSTR(STDMETHODCALLTYPE *GetName)(Obj *);
    void(STDMETHODCALLTYPE *PutName)(Obj *, BSTR);
    HRESULT(STDMETHODCALLTYPE *Fail)(Obj *);
} ObjVtbl;
struct Obj {
    const ObjVtbl *vtbl;
    LONG refs;
    wchar_t name[32];
};

static HRESULT STDMETHODCALLTYPE o_qi(Obj *o, REFIID iid, void **out)
{
    if (IsEqualIID(iid, &IID_IUnknown)) {
        *out = o;
        InterlockedIncrement(&o->refs);
        return S_OK;
    }
    *out = NULL;
    return E_NOINTERFACE;
}
static ULONG STDMETHODCALLTYPE o_addref(Obj *o) { return InterlockedIncrement(&o->refs); }
static ULONG STDMETHODCALLTYPE o_release(Obj *o) { return InterlockedDecrement(&o->refs); }
static int STDMETHODCALLTYPE o_sub(Obj *o, int a, int b) { (void)o; return a - b; }
static double STDMETHODCALLTYPE o_scale(Obj *o, double x, int k) { (void)o; return x * k; }
static BSTR STDMETHODCALLTYPE o_get_name(Obj *o) { return SysAllocString(o->name); }
static void STDMETHODCALLTYPE o_put_name(Obj *o, BSTR v) { lstrcpynW(o->name, v, 32); }
static HRESULT STDMETHODCALLTYPE o_fail(Obj *o) { (void)o; return E_ACCESSDENIED; }
static const ObjVtbl obj_vtbl = {o_qi, o_addref, o_release, o_sub, o_scale, o_get_name, o_put_name, o_fail};

static double __stdcall free_mix(int a, double b, int c, double d, int e, double f)
{
    return a + b + c + d + e + f;
}

static PARAMDATA sub_params[] = {{L"Left", VT_I4}, {L"Right", VT_I4}};
static PARAMDATA scale_params[] = {{L"Value", VT_R8}, {L"Factor", VT_I4}};
static PARAMDATA put_params[] = {{L"Name", VT_BSTR}};
static METHODDATA methods[] = {
    {L"Sub", sub_params, 1, 3, CC_STDCALL, 2, DISPATCH_METHOD, VT_I4},
    {L"Scale", scale_params, 2, 4, CC_STDCALL, 2, DISPATCH_METHOD, VT_R8},
    {L"Name", NULL, 3, 5, CC_STDCALL, 0, DISPATCH_PROPERTYGET, VT_BSTR},
    {L"Name", put_params, 3, 6, CC_STDCALL, 1, DISPATCH_PROPERTYPUT, VT_EMPTY},
    {L"Fail", NULL, 4, 7, CC_STDCALL, 0, DISPATCH_METHOD, VT_HRESULT},
};
static INTERFACEDATA idata = {methods, 5};

int main(void)
{
    Obj obj = {&obj_vtbl, 1, L"départ"};
    ITypeInfo *ti = NULL;
    HRESULT hr = CreateDispTypeInfo(&idata, LOCALE_USER_DEFAULT, &ti);
    check(hr == S_OK && ti, "CreateDispTypeInfo", hr);
    if (!ti)
        return 1;
    IUnknown *unk = NULL;
    hr = CreateStdDispatch(NULL, &obj, ti, &unk);
    check(hr == S_OK && unk, "CreateStdDispatch", hr);
    if (!unk)
        return 1;
    IDispatch *disp = NULL;
    hr = IUnknown_QueryInterface(unk, &IID_IDispatch, (void **)&disp);
    check(hr == S_OK && disp, "QueryInterface(IDispatch)", hr);
    if (!disp)
        return 1;
    UINT tcount = 0;
    check(IDispatch_GetTypeInfoCount(disp, &tcount) == S_OK && tcount == 1, "GetTypeInfoCount", 0);

    /* Noms → DISPID, sans casse ; paramètres nommés. */
    LPOLESTR names[3] = {L"sub", L"RIGHT", L"left"};
    DISPID ids[3];
    hr = IDispatch_GetIDsOfNames(disp, &IID_NULL, names, 3, LOCALE_USER_DEFAULT, ids);
    check(hr == S_OK && ids[0] == 1 && ids[1] == 1 && ids[2] == 0, "GetIDsOfNames avec paramètres", hr);
    LPOLESTR unknown = L"Absent";
    DISPID uid = 0;
    hr = IDispatch_GetIDsOfNames(disp, &IID_NULL, &unknown, 1, LOCALE_USER_DEFAULT, &uid);
    check(hr == DISP_E_UNKNOWNNAME && uid == DISPID_UNKNOWN, "nom inconnu : DISP_E_UNKNOWNNAME", hr);

    /* Sub(10, 3) avec un VT_I2 converti ; rgvarg en ordre inverse. */
    VARIANT args[2], result;
    VariantInit(&result);
    args[1].vt = VT_I2; args[1].iVal = 10;
    args[0].vt = VT_I4; args[0].lVal = 3;
    DISPPARAMS dp = {args, NULL, 2, 0};
    hr = IDispatch_Invoke(disp, 1, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_METHOD, &dp, &result, NULL, NULL);
    check(hr == S_OK && result.vt == VT_I4 && result.lVal == 7, "Invoke Sub(10, 3) = 7", hr);

    /* Arguments nommés : Right=3, Left=10, dans le désordre. */
    DISPID named[2] = {1, 0};
    args[0].vt = VT_I4; args[0].lVal = 3;
    args[1].vt = VT_I4; args[1].lVal = 10;
    DISPPARAMS dpn = {args, named, 2, 2};
    hr = IDispatch_Invoke(disp, 1, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_METHOD, &dpn, &result, NULL, NULL);
    check(hr == S_OK && result.lVal == 7, "Invoke Sub avec arguments nommés", hr);

    /* Scale("2.5", 4) : chaîne convertie en double, retour en XMM0. */
    args[1].vt = VT_BSTR; args[1].bstrVal = SysAllocString(L"2.5");
    args[0].vt = VT_I4; args[0].lVal = 4;
    hr = IDispatch_Invoke(disp, 2, &IID_NULL, 0x409, DISPATCH_METHOD, &dp, &result, NULL, NULL);
    check(hr == S_OK && result.vt == VT_R8 && result.dblVal == 10.0, "Invoke Scale(\"2.5\", 4) = 10.0", hr);
    VariantClear(&args[1]);

    /* Type impossible : DISP_E_TYPEMISMATCH et rang de l'argument fautif. */
    args[1].vt = VT_BSTR; args[1].bstrVal = SysAllocString(L"pas un nombre");
    args[0].vt = VT_I4; args[0].lVal = 1;
    UINT argerr = 99;
    hr = IDispatch_Invoke(disp, 1, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_METHOD, &dp, &result, NULL, &argerr);
    check(hr == DISP_E_TYPEMISMATCH && argerr == 1, "argument non convertible : DISP_E_TYPEMISMATCH", hr);
    VariantClear(&args[1]);

    DISPPARAMS one = {args, NULL, 1, 0};
    hr = IDispatch_Invoke(disp, 1, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_METHOD, &one, &result, NULL, NULL);
    check(hr == DISP_E_BADPARAMCOUNT, "mauvais nombre d'arguments : DISP_E_BADPARAMCOUNT", hr);

    /* Propriété Name : put puis get. */
    DISPID put_id = DISPID_PROPERTYPUT;
    VARIANT value;
    value.vt = VT_BSTR; value.bstrVal = SysAllocString(L"Winlux é");
    DISPPARAMS dput = {&value, &put_id, 1, 1};
    hr = IDispatch_Invoke(disp, 3, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_PROPERTYPUT, &dput, NULL, NULL, NULL);
    check(hr == S_OK && lstrcmpW(obj.name, L"Winlux é") == 0, "propriété Name (put)", hr);
    VariantClear(&value);
    DISPPARAMS none = {NULL, NULL, 0, 0};
    hr = IDispatch_Invoke(disp, 3, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_PROPERTYGET | DISPATCH_METHOD, &none,
                          &result, NULL, NULL);
    check(hr == S_OK && result.vt == VT_BSTR && lstrcmpW(result.bstrVal, L"Winlux é") == 0, "propriété Name (get)", hr);
    VariantClear(&result);

    /* Méthode qui rend un HRESULT d'échec. */
    EXCEPINFO ex;
    memset(&ex, 0, sizeof ex);
    hr = IDispatch_Invoke(disp, 4, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_METHOD, &none, &result, &ex, NULL);
    check(hr == DISP_E_EXCEPTION && ex.scode == E_ACCESSDENIED, "HRESULT d'échec : DISP_E_EXCEPTION + scode", hr);
    hr = IDispatch_Invoke(disp, 42, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_METHOD, &none, &result, NULL, NULL);
    check(hr == DISP_E_MEMBERNOTFOUND, "DISPID inconnu : DISP_E_MEMBERNOTFOUND", hr);

    /* DispGetParam : rang 0 = premier argument (dernier de rgvarg). */
    args[1].vt = VT_I4; args[1].lVal = 5;
    args[0].vt = VT_BSTR; args[0].bstrVal = SysAllocString(L"12");
    VARIANT p;
    VariantInit(&p);
    hr = DispGetParam(&dp, 1, VT_I4, &p, &argerr);
    check(hr == S_OK && p.vt == VT_I4 && p.lVal == 12, "DispGetParam convertit le second argument", hr);
    VariantClear(&args[0]);

    /* DispCallFunc sur une fonction libre : entiers et doubles mêlés, pile. */
    VARIANT a[6], *pa[6];
    VARTYPE vts[6] = {VT_I4, VT_R8, VT_I4, VT_R8, VT_I4, VT_R8};
    double dv[3] = {0.5, 0.25, 0.125};
    for (int i = 0; i < 6; i++) {
        a[i].vt = vts[i];
        if (vts[i] == VT_I4)
            a[i].lVal = i + 1;
        else
            a[i].dblVal = dv[i / 2];
        pa[i] = &a[i];
    }
    VARIANT r;
    VariantInit(&r);
    hr = DispCallFunc(NULL, (ULONG_PTR)free_mix, CC_STDCALL, VT_R8, 6, vts, pa, &r);
    check(hr == S_OK && r.vt == VT_R8 && r.dblVal == 1 + 0.5 + 3 + 0.25 + 5 + 0.125, "DispCallFunc fonction libre", hr);
    VARIANT sa[2], *psa[2] = {&sa[0], &sa[1]};
    VARTYPE svt[2] = {VT_I4, VT_I4};
    sa[0].vt = VT_I4; sa[0].lVal = 50;
    sa[1].vt = VT_I4; sa[1].lVal = 8;
    hr = DispCallFunc(&obj, 3 * sizeof(void *), CC_STDCALL, VT_I4, 2, svt, psa, &r);
    check(hr == S_OK && r.vt == VT_I4 && r.lVal == 42, "DispCallFunc méthode de vtable", hr);

    /* Table des objets actifs. */
    static const CLSID clsid = {0x776c0016, 0x0010, 0x4e00, {0x9c, 0, 0x57, 0x49, 0x4e, 0x4c, 0x55, 0x58}};
    IUnknown *active = NULL;
    hr = GetActiveObject(&clsid, NULL, &active);
    check(hr == MK_E_UNAVAILABLE && active == NULL, "GetActiveObject avant inscription", hr);
    DWORD cookie = 0;
    LONG before = obj.refs;
    hr = RegisterActiveObject((IUnknown *)&obj, &clsid, ACTIVEOBJECT_STRONG, &cookie);
    check(hr == S_OK && cookie != 0 && obj.refs == before + 1, "RegisterActiveObject (fort)", hr);
    hr = GetActiveObject(&clsid, NULL, &active);
    check(hr == S_OK && active == (IUnknown *)&obj, "GetActiveObject rend l'objet", hr);
    if (active)
        IUnknown_Release(active);
    hr = RevokeActiveObject(cookie, NULL);
    check(hr == S_OK && obj.refs == before, "RevokeActiveObject rend la référence", hr);
    hr = GetActiveObject(&clsid, NULL, &active);
    check(hr == MK_E_UNAVAILABLE, "GetActiveObject après révocation", hr);

    IDispatch_Release(disp);
    IUnknown_Release(unk);
    ITypeInfo_Release(ti);
    check(obj.refs == 1, "références de l'objet rendues", 0);

    if (failures)
        return 1;
    printf("Automation OLE : tout est correct\n");
    return 0;
}

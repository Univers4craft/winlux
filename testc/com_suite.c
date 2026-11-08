/* Activation COM depuis le registre (#16), compilé par MinGW-w64.
 *
 * Inscrit com_server.dll (posée à côté de l'exe) sous
 * HKCU\Software\Classes\CLSID\{…}\InprocServer32 avec un ProgID, puis :
 * CLSIDFromProgID, CoCreateInstance, CoGetClassObject + IClassFactory,
 * CoCreateInstanceEx (MULTI_QI complet puis partiel), classe inconnue,
 * DLL introuvable, et CoFreeUnusedLibraries qui décharge le serveur.
 *
 * Code de sortie 0 et « COM par le registre : tout est correct » si tout va bien.
 */
#define COBJMACROS
#include <windows.h>
#include <objbase.h>
#include <stdio.h>
#include <wchar.h>

static const CLSID CLSID_Calc = {0x776c0016, 0x0001, 0x4e00, {0x9c, 0x00, 0x57, 0x49, 0x4e, 0x4c, 0x55, 0x58}};
static const IID IID_ICalc = {0x776c0016, 0x0002, 0x4e00, {0x9c, 0x00, 0x57, 0x49, 0x4e, 0x4c, 0x55, 0x58}};
static const CLSID CLSID_Missing = {0x776c0016, 0x0003, 0x4e00, {0x9c, 0x00, 0x57, 0x49, 0x4e, 0x4c, 0x55, 0x58}};
static const CLSID CLSID_NoDll = {0x776c0016, 0x0004, 0x4e00, {0x9c, 0x00, 0x57, 0x49, 0x4e, 0x4c, 0x55, 0x58}};
static const IID IID_Bogus = {0x776c0016, 0x0005, 0x4e00, {0x9c, 0x00, 0x57, 0x49, 0x4e, 0x4c, 0x55, 0x58}};

#define CALC_KEY L"Software\\Classes\\CLSID\\{776C0016-0001-4E00-9C00-57494E4C5558}"
#define NODLL_KEY L"Software\\Classes\\CLSID\\{776C0016-0004-4E00-9C00-57494E4C5558}"

typedef struct ICalc ICalc;
typedef struct {
    HRESULT(STDMETHODCALLTYPE *QueryInterface)(ICalc *, REFIID, void **);
    ULONG(STDMETHODCALLTYPE *AddRef)(ICalc *);
    ULONG(STDMETHODCALLTYPE *Release)(ICalc *);
    HRESULT(STDMETHODCALLTYPE *Add)(ICalc *, int, int, int *);
} ICalcVtbl;
struct ICalc {
    const ICalcVtbl *vtbl;
};

static int failures;

static void check(int ok, const char *what, HRESULT hr)
{
    if (!ok) {
        printf("ÉCHEC : %s (0x%08lx)\n", what, (unsigned long)hr);
        failures++;
    }
}

static int set_default(const wchar_t *key, const wchar_t *value)
{
    HKEY k;
    if (RegCreateKeyExW(HKEY_CURRENT_USER, key, 0, NULL, 0, KEY_ALL_ACCESS, NULL, &k, NULL) != ERROR_SUCCESS)
        return 0;
    LONG r = RegSetValueExW(k, NULL, 0, REG_SZ, (const BYTE *)value, (DWORD)((wcslen(value) + 1) * 2));
    RegCloseKey(k);
    return r == ERROR_SUCCESS;
}

static int add_with(ICalc *c, int a, int b)
{
    int r = 0;
    return SUCCEEDED(c->vtbl->Add(c, a, b, &r)) ? r : -1;
}

int main(void)
{
    wchar_t dll[MAX_PATH];
    GetModuleFileNameW(NULL, dll, MAX_PATH);
    wchar_t *slash = wcsrchr(dll, L'\\');
    if (!slash)
        slash = wcsrchr(dll, L'/');
    wcscpy(slash ? slash + 1 : dll, L"com_server.dll");

    check(set_default(CALC_KEY L"\\InprocServer32", dll), "inscription InprocServer32", 0);
    check(set_default(CALC_KEY L"\\ProgID", L"Winlux.Calc.1"), "inscription ProgID", 0);
    check(set_default(L"Software\\Classes\\Winlux.Calc.1\\CLSID", L"{776C0016-0001-4E00-9C00-57494E4C5558}"),
          "inscription CLSID du ProgID", 0);
    check(set_default(NODLL_KEY L"\\InprocServer32", L"C:\\absent\\introuvable.dll"), "inscription DLL absente", 0);

    HRESULT hr = CoInitializeEx(NULL, COINIT_APARTMENTTHREADED);
    check(SUCCEEDED(hr), "CoInitializeEx", hr);

    CLSID by_progid;
    hr = CLSIDFromProgID(L"Winlux.Calc.1", &by_progid);
    check(hr == S_OK && IsEqualCLSID(&by_progid, &CLSID_Calc), "CLSIDFromProgID", hr);

    ICalc *calc = NULL;
    hr = CoCreateInstance(&CLSID_Calc, NULL, CLSCTX_INPROC_SERVER, &IID_ICalc, (void **)&calc);
    check(hr == S_OK && calc, "CoCreateInstance par InprocServer32", hr);
    if (calc) {
        check(add_with(calc, 40, 2) == 42, "ICalc::Add dans la DLL", 0);
        calc->vtbl->Release(calc);
    }

    IClassFactory *cf = NULL;
    hr = CoGetClassObject(&CLSID_Calc, CLSCTX_INPROC_SERVER, NULL, &IID_IClassFactory, (void **)&cf);
    check(hr == S_OK && cf, "CoGetClassObject par InprocServer32", hr);
    if (cf) {
        ICalc *other = NULL;
        hr = IClassFactory_CreateInstance(cf, NULL, &IID_ICalc, (void **)&other);
        check(hr == S_OK && other && add_with(other, 7, 8) == 15, "IClassFactory::CreateInstance", hr);
        if (other)
            other->vtbl->Release(other);
        IClassFactory_Release(cf);
    }

    MULTI_QI qi[2] = {{&IID_IUnknown, NULL, 0}, {&IID_ICalc, NULL, 0}};
    hr = CoCreateInstanceEx(&CLSID_Calc, NULL, CLSCTX_INPROC_SERVER, NULL, 2, qi);
    check(hr == S_OK && qi[0].hr == S_OK && qi[1].hr == S_OK && qi[1].pItf, "CoCreateInstanceEx", hr);
    if (qi[1].pItf)
        check(add_with((ICalc *)qi[1].pItf, 1, 1) == 2, "CoCreateInstanceEx : interface utilisable", 0);
    for (int i = 0; i < 2; i++)
        if (qi[i].pItf)
            IUnknown_Release(qi[i].pItf);

    MULTI_QI partial[2] = {{&IID_ICalc, NULL, 0}, {&IID_Bogus, NULL, 0}};
    hr = CoCreateInstanceEx(&CLSID_Calc, NULL, CLSCTX_INPROC_SERVER, NULL, 2, partial);
    check(hr == CO_S_NOTALLINTERFACES && partial[0].hr == S_OK && partial[1].hr == E_NOINTERFACE &&
              partial[1].pItf == NULL,
          "CoCreateInstanceEx partiel (CO_S_NOTALLINTERFACES)", hr);
    if (partial[0].pItf)
        IUnknown_Release(partial[0].pItf);

    IUnknown *none = (IUnknown *)1;
    hr = CoCreateInstance(&CLSID_Missing, NULL, CLSCTX_INPROC_SERVER, &IID_IUnknown, (void **)&none);
    check(hr == REGDB_E_CLASSNOTREG && none == NULL, "classe non inscrite : REGDB_E_CLASSNOTREG", hr);

    none = (IUnknown *)1;
    hr = CoCreateInstance(&CLSID_NoDll, NULL, CLSCTX_INPROC_SERVER, &IID_IUnknown, (void **)&none);
    check(FAILED(hr) && none == NULL, "DLL inscrite mais absente : échec propre", hr);

    wchar_t flag[8] = L"";
    check(GetEnvironmentVariableW(L"WINLUX_COM_DETACHED", flag, 8) == 0, "serveur encore chargé", 0);
    CoFreeUnusedLibraries();
    check(GetEnvironmentVariableW(L"WINLUX_COM_DETACHED", flag, 8) == 1 && flag[0] == L'1',
          "CoFreeUnusedLibraries décharge le serveur libre", 0);

    CoUninitialize();
    RegDeleteTreeW(HKEY_CURRENT_USER, CALC_KEY);
    RegDeleteTreeW(HKEY_CURRENT_USER, NODLL_KEY);
    RegDeleteTreeW(HKEY_CURRENT_USER, L"Software\\Classes\\Winlux.Calc.1");

    if (failures)
        return 1;
    printf("COM par le registre : tout est correct\n");
    return 0;
}

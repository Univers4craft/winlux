/* WMI en lecture (CLSID_WbemLocator, #1198).
 *
 * Le parcours classique d'un installeur ou d'un outil d'inventaire :
 * CoInitializeSecurity, WbemLocator, ConnectServer(ROOT\CIMV2),
 * CoSetProxyBlanket, ExecQuery en WQL, Next, Get. Seuls les en-têtes
 * MinGW-w64 servent. */
#define COBJMACROS
#include <windows.h>
#include <ole2.h>
#include <wbemcli.h>
#include <stdio.h>

static int failures;

static void check(int ok, const char *what)
{
    printf("%s : %s\n", ok ? "OK" : "ÉCHEC", what);
    if (!ok)
        failures++;
}

/* Première instance d'une requête ; NULL si aucune. */
static IWbemClassObject *first(IWbemServices *svc, const wchar_t *query, ULONG *count)
{
    IEnumWbemClassObject *en = NULL;
    IWbemClassObject *obj = NULL, *more = NULL;
    BSTR lang = SysAllocString(L"WQL"), q = SysAllocString(query);
    HRESULT hr = IWbemServices_ExecQuery(svc, lang, q,
                                         WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                                         NULL, &en);
    ULONG got = 0;
    *count = 0;
    SysFreeString(lang);
    SysFreeString(q);
    if (FAILED(hr) || !en)
        return NULL;
    if (IEnumWbemClassObject_Next(en, WBEM_INFINITE, 1, &obj, &got) == WBEM_S_NO_ERROR && got == 1)
        *count = 1;
    while (IEnumWbemClassObject_Next(en, WBEM_INFINITE, 1, &more, &got) == WBEM_S_NO_ERROR && got == 1) {
        (*count)++;
        IWbemClassObject_Release(more);
    }
    IEnumWbemClassObject_Release(en);
    return obj;
}

/* Une propriété chaîne non vide. */
static int has_text(IWbemClassObject *obj, const wchar_t *name)
{
    VARIANT v;
    CIMTYPE type = 0;
    int ok;
    VariantInit(&v);
    if (FAILED(IWbemClassObject_Get(obj, name, 0, &v, &type, NULL)))
        return 0;
    ok = V_VT(&v) == VT_BSTR && V_BSTR(&v) && V_BSTR(&v)[0];
    if (ok)
        printf("  %ls = %ls\n", name, V_BSTR(&v));
    VariantClear(&v);
    return ok;
}

int main(void)
{
    IWbemLocator *loc = NULL;
    IWbemServices *svc = NULL;
    IWbemClassObject *obj;
    BSTR ns;
    HRESULT hr;
    ULONG n;
    VARIANT v;
    CIMTYPE type;
    SAFEARRAY *names = NULL;

    hr = CoInitializeEx(NULL, COINIT_MULTITHREADED);
    check(SUCCEEDED(hr), "CoInitializeEx");
    hr = CoInitializeSecurity(NULL, -1, NULL, NULL, RPC_C_AUTHN_LEVEL_DEFAULT,
                              RPC_C_IMP_LEVEL_IMPERSONATE, NULL, EOAC_NONE, NULL);
    check(SUCCEEDED(hr) || hr == RPC_E_TOO_LATE, "CoInitializeSecurity");
    hr = CoCreateInstance(&CLSID_WbemLocator, NULL, CLSCTX_INPROC_SERVER,
                          &IID_IWbemLocator, (void **)&loc);
    check(SUCCEEDED(hr) && loc, "CoCreateInstance(CLSID_WbemLocator)");
    if (!loc) {
        printf("WMI : %d échec(s)\n", failures);
        return 1;
    }
    ns = SysAllocString(L"ROOT\\CIMV2");
    hr = IWbemLocator_ConnectServer(loc, ns, NULL, NULL, NULL, 0, NULL, NULL, &svc);
    SysFreeString(ns);
    check(SUCCEEDED(hr) && svc, "ConnectServer(ROOT\\CIMV2)");
    if (!svc) {
        printf("WMI : %d échec(s)\n", failures);
        return 1;
    }
    hr = CoSetProxyBlanket((IUnknown *)svc, RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE, NULL,
                           RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, NULL, EOAC_NONE);
    check(SUCCEEDED(hr), "CoSetProxyBlanket");

    obj = first(svc, L"SELECT * FROM Win32_OperatingSystem", &n);
    check(obj && n == 1, "Win32_OperatingSystem : une instance");
    if (obj) {
        check(has_text(obj, L"Caption") && has_text(obj, L"Version"), "Caption et Version");
        VariantInit(&v);
        hr = IWbemClassObject_Get(obj, L"TotalVisibleMemorySize", 0, &v, &type, NULL);
        check(SUCCEEDED(hr) && type == CIM_UINT64 && V_VT(&v) == VT_BSTR,
              "uint64 rendu en chaîne (TotalVisibleMemorySize)");
        VariantClear(&v);
        hr = IWbemClassObject_Get(obj, L"Inexistante", 0, &v, NULL, NULL);
        check(hr == WBEM_E_NOT_FOUND, "propriété inconnue : WBEM_E_NOT_FOUND");
        hr = IWbemClassObject_GetNames(obj, NULL, WBEM_FLAG_NONSYSTEM_ONLY, NULL, &names);
        {
            LONG upper = -1;
            check(SUCCEEDED(hr) && names && SUCCEEDED(SafeArrayGetUBound(names, 1, &upper))
                      && upper > 10,
                  "GetNames");
        }
        if (names)
            SafeArrayDestroy(names);
        IWbemClassObject_Release(obj);
    }

    obj = first(svc, L"SELECT Name, NumberOfLogicalProcessors FROM Win32_Processor", &n);
    check(obj && n >= 1, "Win32_Processor");
    if (obj) {
        VariantInit(&v);
        hr = IWbemClassObject_Get(obj, L"NumberOfLogicalProcessors", 0, &v, &type, NULL);
        check(SUCCEEDED(hr) && V_VT(&v) == VT_I4 && V_I4(&v) >= 1 && type == CIM_UINT32,
              "NumberOfLogicalProcessors en VT_I4");
        VariantClear(&v);
        IWbemClassObject_Release(obj);
    }

    obj = first(svc, L"SELECT * FROM Win32_LogicalDisk WHERE DeviceID = 'C:'", &n);
    check(obj && n == 1, "Win32_LogicalDisk WHERE DeviceID = 'C:'");
    if (obj) {
        check(has_text(obj, L"FileSystem"), "FileSystem de C:");
        IWbemClassObject_Release(obj);
    }

    {
        static const wchar_t *classes[] = {
            L"SELECT * FROM Win32_ComputerSystem", L"SELECT * FROM Win32_BaseBoard",
            L"SELECT * FROM Win32_BIOS", L"SELECT * FROM Win32_VideoController",
            L"SELECT * FROM Win32_Process",
        };
        int i;
        for (i = 0; i < 5; i++) {
            obj = first(svc, classes[i], &n);
            printf("  %ls : %lu instance(s)\n", classes[i], n);
            check(obj != NULL, "classe servie");
            if (obj)
                IWbemClassObject_Release(obj);
        }
    }

    {
        IEnumWbemClassObject *en = NULL;
        BSTR lang = SysAllocString(L"WQL"), q = SysAllocString(L"SELECT * FROM Win32_Nothing");
        hr = IWbemServices_ExecQuery(svc, lang, q, 0, NULL, &en);
        check(hr == WBEM_E_INVALID_CLASS, "classe inconnue : WBEM_E_INVALID_CLASS");
        SysFreeString(lang);
        SysFreeString(q);
    }
    {
        BSTR path = SysAllocString(L"Win32_OperatingSystem=@");
        obj = NULL;
        hr = IWbemServices_GetObject(svc, path, 0, NULL, &obj, NULL);
        check(SUCCEEDED(hr) && obj, "GetObject(Win32_OperatingSystem=@)");
        if (obj)
            IWbemClassObject_Release(obj);
        SysFreeString(path);
    }

    IWbemServices_Release(svc);
    IWbemLocator_Release(loc);
    CoUninitialize();
    if (failures) {
        printf("WMI : %d échec(s)\n", failures);
        return 1;
    }
    printf("WMI : tout est correct\n");
    return 0;
}

/* Service d'annotations d'accessibilité (CLSID_AccPropServices, #1114).
 *
 * Ce qu'en fait la VCL de Delphi : création du service, annotations de
 * fenêtres et de menus (valeur, chaîne, serveur), effacement, chaînes
 * d'identité opaques composées puis décomposées. Compilé en x64
 * (test-mingw.sh) et en x86 (test-wow32.sh), où MSAAPROPID et VARIANT
 * passent par valeur sur la pile. Seuls les en-têtes MinGW-w64 servent. */
#define COBJMACROS
#include <initguid.h>
#include <windows.h>
#include <ole2.h>
#include <oleacc.h>
#include <stdio.h>

static int failures;

static void check(int ok, const char *what)
{
    printf("%s : %s\n", ok ? "OK" : "ÉCHEC", what);
    if (!ok)
        failures++;
}

/* Serveur d'annotations minimal : compte ses références. */
typedef struct {
    IAccPropServer iface;
    LONG refs;
} Server;

static HRESULT STDMETHODCALLTYPE srv_qi(IAccPropServer *This, REFIID riid, void **out)
{
    if (IsEqualIID(riid, &IID_IUnknown) || IsEqualIID(riid, &IID_IAccPropServer)) {
        *out = This;
        IAccPropServer_AddRef(This);
        return S_OK;
    }
    *out = NULL;
    return E_NOINTERFACE;
}

static ULONG STDMETHODCALLTYPE srv_add_ref(IAccPropServer *This)
{
    return InterlockedIncrement(&((Server *)This)->refs);
}

static ULONG STDMETHODCALLTYPE srv_release(IAccPropServer *This)
{
    return InterlockedDecrement(&((Server *)This)->refs);
}

static HRESULT STDMETHODCALLTYPE srv_get(IAccPropServer *This, const BYTE *id, DWORD len,
                                         MSAAPROPID prop, VARIANT *out, BOOL *has)
{
    (void)This; (void)id; (void)len; (void)prop;
    VariantInit(out);
    *has = FALSE;
    return S_OK;
}

static IAccPropServerVtbl server_vtbl = { srv_qi, srv_add_ref, srv_release, srv_get };

int main(void)
{
    IAccPropServices *svc = NULL;
    HRESULT hr;
    HWND hwnd = (HWND)(ULONG_PTR)0x10042;
    HMENU hmenu = (HMENU)(ULONG_PTR)0x20077;
    BYTE *id = NULL;
    DWORD len = 0;
    VARIANT v;
    Server server = { { &server_vtbl }, 1 };
    MSAAPROPID props[2];

    CoInitialize(NULL);
    hr = CoCreateInstance(&CLSID_AccPropServices, NULL, CLSCTX_INPROC_SERVER,
                          &IID_IAccPropServices, (void **)&svc);
    check(SUCCEEDED(hr) && svc != NULL, "CoCreateInstance(CLSID_AccPropServices)");
    if (!svc) {
        printf("AccPropServices : %d échec(s)\n", failures + 1);
        return 1;
    }

    /* Annotations d'une fenêtre : chaîne, valeur, effacement. */
    hr = IAccPropServices_SetHwndPropStr(svc, hwnd, OBJID_CLIENT, CHILDID_SELF,
                                         PROPID_ACC_NAME, L"Valider");
    check(hr == S_OK, "SetHwndPropStr(PROPID_ACC_NAME)");
    VariantInit(&v);
    V_VT(&v) = VT_I4;
    V_I4(&v) = ROLE_SYSTEM_PUSHBUTTON;
    hr = IAccPropServices_SetHwndProp(svc, hwnd, OBJID_CLIENT, CHILDID_SELF,
                                      PROPID_ACC_ROLE, v);
    check(hr == S_OK, "SetHwndProp(PROPID_ACC_ROLE, VT_I4)");
    props[0] = PROPID_ACC_NAME;
    props[1] = PROPID_ACC_ROLE;
    hr = IAccPropServices_ClearHwndProps(svc, hwnd, OBJID_CLIENT, CHILDID_SELF, props, 2);
    check(hr == S_OK, "ClearHwndProps");

    /* Serveur : retenu tant que l'annotation existe. */
    hr = IAccPropServices_SetHwndPropServer(svc, hwnd, OBJID_CLIENT, CHILDID_SELF,
                                            props, 1, &server.iface, ANNO_THIS);
    check(hr == S_OK && server.refs > 1, "SetHwndPropServer retient le serveur");
    hr = IAccPropServices_ClearHwndProps(svc, hwnd, OBJID_CLIENT, CHILDID_SELF, props, 1);
    check(hr == S_OK && server.refs == 1, "ClearHwndProps relâche le serveur");

    /* Chaîne d'identité d'une fenêtre : aller-retour. */
    hr = IAccPropServices_ComposeHwndIdentityString(svc, hwnd, OBJID_CLIENT, 3, &id, &len);
    check(hr == S_OK && id != NULL && len > 0, "ComposeHwndIdentityString");
    if (id) {
        HWND h = NULL;
        DWORD obj = 0, child = 0;
        HMENU m = NULL;
        hr = IAccPropServices_DecomposeHwndIdentityString(svc, id, len, &h, &obj, &child);
        check(hr == S_OK && h == hwnd && obj == (DWORD)OBJID_CLIENT && child == 3,
              "DecomposeHwndIdentityString rend fenêtre, objet et enfant");
        hr = IAccPropServices_DecomposeHmenuIdentityString(svc, id, len, &m, &child);
        check(hr == E_INVALIDARG, "une identité de fenêtre n'est pas un menu");
        V_VT(&v) = VT_I4;
        V_I4(&v) = STATE_SYSTEM_FOCUSABLE;
        hr = IAccPropServices_SetPropValue(svc, id, len, PROPID_ACC_STATE, v);
        check(hr == S_OK, "SetPropValue sur une chaîne d'identité");
        props[0] = PROPID_ACC_STATE;
        hr = IAccPropServices_ClearProps(svc, id, len, props, 1);
        check(hr == S_OK, "ClearProps");
        CoTaskMemFree(id);
    }

    /* Menus. */
    hr = IAccPropServices_SetHmenuPropStr(svc, hmenu, 2, PROPID_ACC_DESCRIPTION, L"Ouvrir");
    check(hr == S_OK, "SetHmenuPropStr");
    id = NULL;
    hr = IAccPropServices_ComposeHmenuIdentityString(svc, hmenu, 2, &id, &len);
    check(hr == S_OK && id != NULL, "ComposeHmenuIdentityString");
    if (id) {
        HMENU m = NULL;
        DWORD child = 0;
        hr = IAccPropServices_DecomposeHmenuIdentityString(svc, id, len, &m, &child);
        check(hr == S_OK && m == hmenu && child == 2, "DecomposeHmenuIdentityString");
        CoTaskMemFree(id);
    }
    props[0] = PROPID_ACC_DESCRIPTION;
    hr = IAccPropServices_ClearHmenuProps(svc, hmenu, 2, props, 1);
    check(hr == S_OK, "ClearHmenuProps");

    /* Arguments refusés. */
    hr = IAccPropServices_SetPropValue(svc, NULL, 0, PROPID_ACC_NAME, v);
    check(hr == E_INVALIDARG, "SetPropValue sans identité : E_INVALIDARG");

    IAccPropServices_Release(svc);
    CoUninitialize();
    if (failures) {
        printf("AccPropServices : %d échec(s)\n", failures);
        return 1;
    }
    printf("AccPropServices : tout est correct\n");
    return 0;
}

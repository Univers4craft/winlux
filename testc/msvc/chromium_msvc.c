// Fonctions que Chromium et Electron résolvent au démarrage et dont
// l'absence arrête le processus (CHECK) : relevé d'un vrai Electron 45 sous
// Winlux (#236). Format MSVC (clang en mode x86_64-pc-windows-msvc, sans CRT).
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) void *__stdcall LoadLibraryA(const char *);
__declspec(dllimport) void *__stdcall LoadLibraryExA(const char *, void *, u32);
__declspec(dllimport) void *__stdcall GetProcAddress(void *, const char *);
__declspec(dllimport) void *__stdcall GetCurrentProcess(void);

void *memset(void *d, int c, u64 n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}

typedef int(__stdcall *MitigationFn)(void *, u32, void *, u64);
typedef int(__stdcall *DpiCtxFn)(long long);
typedef long long(__stdcall *GetCtxFn)(void);
typedef int(__stdcall *AwarenessFn)(long long);
typedef int(__stdcall *DpiMonFn)(void *, u32, u32 *, u32 *);
typedef int(__stdcall *ScaleFn)(void *, u32 *);
typedef long(__stdcall *RegTypeLibFn)(const void *, u16, u16, u32, void **);
typedef int(__stdcall *SdControlFn)(const void *, u16 *, u32 *);
typedef int(__stdcall *InitSdFn)(void *, u32);
typedef int(__stdcall *SaclFn)(void *, int, void *, int);

static void *need(void *dll, const char *name, u32 code) {
    void *f = GetProcAddress(dll, name);
    if (!f) ExitProcess(code);
    return f;
}

void entry(void) {
    void *k32 = LoadLibraryA("kernel32.dll");
    void *adv = LoadLibraryA("advapi32.dll");
    void *u32dll = LoadLibraryA("user32.dll");
    if (!k32 || !adv || !u32dll) ExitProcess(1);

    // 1. Atténuations : lues comme désactivées.
    MitigationFn get = (MitigationFn)need(k32, "GetProcessMitigationPolicy", 2);
    u8 policy[8];
    memset(policy, 0xFF, sizeof policy);
    if (!get(GetCurrentProcess(), 8 /* ProcessSignaturePolicy */, policy, 4) || policy[0] || policy[3]) ExitProcess(3);
    need(k32, "SetProcessMitigationPolicy", 4);
    need(k32, "GetProcessInformation", 5);
    need(k32, "SetProcessInformation", 6);
    need(k32, "SetThreadInformation", 7);
    need(k32, "FlsGetValue2", 8);

    // 2. Descripteurs de sécurité.
    InitSdFn init = (InitSdFn)need(adv, "InitializeSecurityDescriptor", 9);
    SdControlFn ctl = (SdControlFn)need(adv, "GetSecurityDescriptorControl", 10);
    SaclFn setsacl = (SaclFn)need(adv, "SetSecurityDescriptorSacl", 11);
    need(adv, "GetSecurityDescriptorSacl", 12);
    need(adv, "MakeSelfRelativeSD", 13);
    need(adv, "MakeAbsoluteSD", 14);
    need(adv, "GetSecurityDescriptorRMControl", 15);
    static u8 sd[40];
    static u8 acl[8] = {2, 0, 8, 0, 0, 0, 0, 0};
    u16 control = 0;
    u32 revision = 0;
    if (!init(sd, 1) || !setsacl(sd, 1, acl, 0) || !ctl(sd, &control, &revision)) ExitProcess(16);
    if (control != 0x10 || revision != 1) ExitProcess(17);

    // 5. Veille ; 6. contextes de DPI.
    need(u32dll, "RegisterSuspendResumeNotification", 18);
    need(u32dll, "UnregisterSuspendResumeNotification", 19);
    DpiCtxFn setctx = (DpiCtxFn)need(u32dll, "SetProcessDpiAwarenessContext", 20);
    GetCtxFn getctx = (GetCtxFn)need(u32dll, "GetThreadDpiAwarenessContext", 21);
    AwarenessFn aw = (AwarenessFn)need(u32dll, "GetAwarenessFromDpiAwarenessContext", 22);
    if (!setctx(-4 /* PER_MONITOR_AWARE_V2 */) || aw(getctx()) != 2) ExitProcess(23);
    need(u32dll, "SetWinEventHook", 24);

    // shcore par son contrat d'API.
    void *sh = LoadLibraryExA("api-ms-win-shcore-scaling-l1-1-1.dll", 0, 0x800 /* SEARCH_SYSTEM32 */);
    if (!sh) ExitProcess(25);
    DpiMonFn dpi = (DpiMonFn)need(sh, "GetDpiForMonitor", 26);
    ScaleFn scale = (ScaleFn)need(sh, "GetScaleFactorForMonitor", 27);
    u32 x = 0, y = 0, s = 0;
    if (dpi((void *)1, 0, &x, &y) != 0 || x < 96 || x != y) ExitProcess(28);
    if (scale((void *)1, &s) != 0 || s < 100) ExitProcess(29);
    need(sh, "SetProcessDpiAwareness", 30);
    if (!LoadLibraryA("api-ms-win-downlevel-shell32-l1-1-0.dll")) ExitProcess(31);

    // 7. oleaut32 par ordinal (162 = LoadRegTypeLib) : erreur documentée.
    void *ole = LoadLibraryExA("oleaut32.dll", 0, 0x800);
    if (!ole) ExitProcess(32);
    RegTypeLibFn reg = (RegTypeLibFn)GetProcAddress(ole, (const char *)162);
    if (!reg) ExitProcess(33);
    void *lib = (void *)1;
    static u8 guid[16];
    if (reg(guid, 1, 0, 0, &lib) >= 0 || lib) ExitProcess(34);
    ExitProcess(0);
}

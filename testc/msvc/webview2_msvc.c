// WebView2 maison (#236) : comme une application WebView2, le programme
// charge WebView2Loader.dll, crée l'environnement puis le contrôleur dans
// sa fenêtre, et dialogue avec la page :
//   - la page envoie « bonjour » (postMessage d'une chaîne) ;
//   - l'application répond par PostWebMessageAsJson({"x":42}) ;
//   - la page renvoie {"echo":{"x":42}} ;
//   - ExecuteScript("6*7") doit rendre "42" ;
//   - un hôte virtuel (SetVirtualHostNameToFolderMapping) sert un fichier
//     du dossier donné en argument, qui envoie « hote-virtuel ».
// Tous les rappels arrivent par la boucle de messages, sur ce fil.
// Code de sortie : 0 si tout passe, sinon le numéro de l'étape ratée.
typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;
typedef long HRESULT;

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) u64 __stdcall GetTickCount64(void);
__declspec(dllimport) void __stdcall Sleep(u32);
__declspec(dllimport) void *__stdcall LoadLibraryW(const u16 *);
__declspec(dllimport) void *__stdcall GetProcAddress(void *, const char *);
__declspec(dllimport) u16 *__stdcall GetCommandLineW(void);
__declspec(dllimport) u16 __stdcall RegisterClassW(const void *);
__declspec(dllimport) void *__stdcall CreateWindowExW(u32, const u16 *, const u16 *, u32, int, int, int, int, void *, void *, void *, void *);
__declspec(dllimport) u64 __stdcall DefWindowProcW(void *, u32, u64, u64);
__declspec(dllimport) int __stdcall PeekMessageW(void *, void *, u32, u32, u32);
__declspec(dllimport) int __stdcall TranslateMessage(const void *);
__declspec(dllimport) u64 __stdcall DispatchMessageW(const void *);
__declspec(dllimport) void __stdcall CoTaskMemFree(void *);

void *memset(void *d, int c, u64 n) {
    u8 *o = d;
    while (n--) *o++ = (u8)c;
    return d;
}

static int wsame(const u16 *a, const char *b) {
    if (!a) return 0;
    while (*b)
        if (*a++ != (u16)*b++) return 0;
    return *a == 0;
}
#define W(s) (const u16 *)L##s

typedef HRESULT(__stdcall *fn)(void *, ...);
#define CALL(obj, slot, ...) (((fn *)(*(void ***)(obj)))[slot])((obj), ##__VA_ARGS__)

static u32 step = 1;    // étape en cours (code de sortie si échec)
static int finished = 0;
static void *hwnd;
static void *env, *controller, *webview;
static u16 folder[512];

// ── Gestionnaire COM générique : Invoke(this, a, b) ──
struct handler {
    void **vtbl;
    HRESULT (*on)(void *, void *);
};
static HRESULT __stdcall h_qi(void *t, const void *iid, void **out) {
    (void)iid;
    *out = t;
    return 0;
}
static u32 __stdcall h_ref(void *t) {
    (void)t;
    return 1;
}
static HRESULT __stdcall h_invoke(struct handler *t, void *a, void *b) { return t->on(a, b); }
static void *handler_vtbl[] = {(void *)h_qi, (void *)h_ref, (void *)h_ref, (void *)h_invoke};

static void fail(u32 code) { ExitProcess(code); }

static HRESULT on_script(void *hr, void *result) {
    if ((HRESULT)(u64)hr != 0 || !wsame(result, "42")) fail(40);
    step = 50;
    // Hôte virtuel : https://appli.exemple/ → dossier donné.
    if (CALL(webview, 3 + 58 + 7 + 3, W("appli.exemple"), folder, 0) != 0) fail(50);
    if (CALL(webview, 3 + 2, W("https://appli.exemple/index.html")) != 0) fail(51);
    return 0;
}
static struct handler script_done = {handler_vtbl, on_script};

static HRESULT on_message(void *sender, void *args) {
    (void)sender;
    u16 *json = 0, *text = 0;
    if (CALL(args, 3 + 1, &json) != 0 || !json) fail(20);
    HRESULT is_string = CALL(args, 3 + 2, &text);
    if (step == 10) {
        if (is_string != 0 || !wsame(text, "bonjour") || !wsame(json, "\"bonjour\"")) fail(10);
        step = 20;
        if (CALL(webview, 3 + 29, W("{\"x\":42}")) != 0) fail(21);
    } else if (step == 20) {
        if (is_string == 0 || !wsame(json, "{\"echo\":{\"x\":42}}")) fail(22);
        step = 40;
        if (CALL(webview, 3 + 26, W("6*7"), &script_done) != 0) fail(41);
    } else if (step == 50) {
        if (is_string != 0 || !wsame(text, "hote-virtuel")) fail(52);
        u16 *src = 0;
        if (CALL(args, 3 + 0, &src) != 0 || !wsame(src, "https://appli.exemple/index.html")) fail(53);
        finished = 1;
    }
    if (json) CoTaskMemFree(json);
    if (text) CoTaskMemFree(text);
    return 0;
}
static struct handler message = {handler_vtbl, on_message};

static HRESULT on_controller(void *hr, void *ctrl) {
    if ((HRESULT)(u64)hr != 0 || !ctrl) fail(4);
    controller = ctrl;
    if (CALL(controller, 3 + 22, &webview) != 0 || !webview) fail(5);
    long long token = 0;
    if (CALL(webview, 3 + 31, &message, &token) != 0) fail(6);
    int bounds[4] = {0, 0, 640, 480};
    if (CALL(controller, 3 + 3, bounds) != 0) fail(7);
    step = 10;
    if (CALL(webview, 3 + 3,
             W("<html><body><script>"
               "window.chrome.webview.addEventListener('message', e => window.chrome.webview.postMessage({echo: e.data}));"
               "window.chrome.webview.postMessage('bonjour');"
               "</script></body></html>")) != 0)
        fail(8);
    return 0;
}
static struct handler controller_done = {handler_vtbl, on_controller};

static HRESULT on_environment(void *hr, void *e) {
    if ((HRESULT)(u64)hr != 0 || !e) fail(2);
    env = e;
    step = 3;
    if (CALL(env, 3 + 0, hwnd, &controller_done) != 0) fail(3);
    return 0;
}
static struct handler environment_done = {handler_vtbl, on_environment};

static u64 __stdcall wndproc(void *h, u32 m, u64 w, u64 l) { return DefWindowProcW(h, m, w, l); }

struct wndclass {
    u32 style;
    u32 pad;
    void *proc;
    int cls, wnd;
    void *inst, *icon, *cursor, *bg;
    const u16 *menu, *name;
};

void entry(void) {
    // Dossier de l'hôte virtuel : dernier argument.
    const u16 *line = GetCommandLineW(), *last = line;
    for (const u16 *p = line; *p; p++)
        if (*p == ' ') last = p + 1;
    u32 n = 0;
    while (last[n] && last[n] != '"' && n < 511) folder[n] = last[n], n++;
    folder[n] = 0;

    struct wndclass wc;
    memset(&wc, 0, sizeof wc);
    wc.proc = (void *)wndproc;
    wc.name = W("TestWebView2");
    RegisterClassW(&wc);
    hwnd = CreateWindowExW(0, W("TestWebView2"), W("WebView2"), 0x10CF0000, 0, 0, 640, 480, 0, 0, 0, 0);
    if (!hwnd) fail(1);
    void *loader = LoadLibraryW(W("WebView2Loader.dll"));
    if (!loader) fail(1);
    HRESULT(__stdcall * create)(const u16 *, const u16 *, void *, void *) =
        (HRESULT(__stdcall *)(const u16 *, const u16 *, void *, void *))GetProcAddress(
            loader, "CreateCoreWebView2EnvironmentWithOptions");
    if (!create) fail(1);
    step = 2;
    if (create(0, 0, 0, &environment_done) != 0) fail(2);
    u64 end = GetTickCount64() + 45000;
    u8 msg[48];
    while (!finished) {
        if (GetTickCount64() > end) fail(100 + step);
        while (PeekMessageW(msg, 0, 0, 0, 1)) {
            TranslateMessage(msg);
            DispatchMessageW(msg);
        }
        Sleep(10);
    }
    CALL(controller, 3 + 21);
    ExitProcess(0);
}

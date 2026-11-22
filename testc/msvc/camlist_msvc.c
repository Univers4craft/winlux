// Inventaire des caméras au lancement (#3100), au format MSVC (clang en mode
// x86_64-pc-windows-msvc, sans CRT) : MFCreateAttributes, SetGUID
// (MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE = VIDCAP), puis MFEnumDeviceSources,
// comme une application qui compte ses caméras en démarrant. Lister n'est
// pas utiliser : l'appel doit revenir tout de suite, sans question.
// Code de sortie : 0 si l'inventaire revient en moins de 2 s, 1 si
// MFCreateAttributes échoue, 2 si SetGUID échoue, 3 si MFEnumDeviceSources
// échoue, 4 s'il a attendu plus de 2 s.
// Sources : Learn, « MFEnumDeviceSources », « MFCreateAttributes »,
// « IMFAttributes::SetGUID » ; SetGUID est la 25e case de la vtable, comme
// dans la table d'IMFAttributes que sert runtime/src/dlls/mfcapture.rs.
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;
typedef struct { u32 a; u16 b, c; u8 d[8]; } Guid;
typedef struct Attrs Attrs;
typedef struct {
    void *slots[24];
    long (__stdcall *SetGUID)(Attrs *, const Guid *, const Guid *);
} AttrsVtbl;
struct Attrs { const AttrsVtbl *vtbl; };

__declspec(dllimport) void __stdcall ExitProcess(u32);
__declspec(dllimport) u32 __stdcall GetTickCount(void);
__declspec(dllimport) long __stdcall MFCreateAttributes(Attrs **, u32);
__declspec(dllimport) long __stdcall MFEnumDeviceSources(Attrs *, void ***, u32 *);

static const Guid SOURCE_TYPE = {0xc60ac5fe, 0x252a, 0x478f, {0xa0, 0xef, 0xbc, 0x8f, 0xa5, 0xf7, 0xca, 0xd3}};
static const Guid VIDCAP = {0x8ac3587a, 0x4ae7, 0x42d8, {0x99, 0xe0, 0x0a, 0x60, 0x13, 0xee, 0xf9, 0x0f}};

void entry(void) {
    Attrs *attrs = 0;
    if (MFCreateAttributes(&attrs, 1) < 0 || !attrs)
        ExitProcess(1);
    if (attrs->vtbl->SetGUID(attrs, &SOURCE_TYPE, &VIDCAP) < 0)
        ExitProcess(2);
    void **devices = 0;
    u32 count = 0;
    u32 start = GetTickCount();
    if (MFEnumDeviceSources(attrs, &devices, &count) < 0)
        ExitProcess(3);
    if (GetTickCount() - start > 2000)
        ExitProcess(4);
    ExitProcess(0);
}

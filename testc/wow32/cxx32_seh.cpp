// Cas 14 de cxx32 (#1413, 7-Zip) : catch(...) prend une exception SEH
// (compilé avec -fasync-exceptions, comme /EHa) et `throw;` la relance
// telle quelle jusqu'au catch(...) de l'appelant.
typedef unsigned int DWORD;
extern "C" __declspec(dllimport) void __stdcall RaiseException(DWORD, DWORD, DWORD, const unsigned long *);

static int seh_seen;

__attribute__((noinline)) static void rethrow_seh_inner() {
    try {
        RaiseException(0xC0000005u, 0, 0, 0);
    } catch (...) {
        seh_seen = 1;
        throw;
    }
}

static DWORD seen_code;
static int filter(DWORD code) {
    seen_code = code;
    return 1; // EXCEPTION_EXECUTE_HANDLER
}

// L'appelant reçoit l'exception SEH d'origine (0xC0000005), pas une
// exception C++ (0xE06D7363) sans type.
int rethrow_seh() {
    int r = 0;
    __try {
        rethrow_seh_inner();
    } __except (filter(_exception_code())) {
        r = 10;
    }
    return seen_code == 0xC0000005u ? r + seh_seen : -1;
}

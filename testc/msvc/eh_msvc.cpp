// Exceptions C++ et blocs __try au format MSVC (compilé par clang en mode
// x86_64-pc-windows-msvc, sans CRT) : vérifie __CxxFrameHandler3,
// _CxxThrowException et __C_specific_handler de Winlux.
// Code de sortie : 0 si tout passe, sinon le numéro du premier test raté.
extern "C" __declspec(dllimport) void __stdcall ExitProcess(unsigned);
extern "C" __declspec(dllimport) void __stdcall RaiseException(unsigned, unsigned, unsigned, const unsigned long long *);

static int ctors, dtors, copies;

// Pas de CRT : les destructeurs virtuels réclament operator delete.
void operator delete(void *) noexcept {}
void operator delete(void *, unsigned long long) noexcept {}

struct Guard {
    Guard() { ctors++; }
    ~Guard() { dtors++; }
};

struct Base {
    int code;
    Base(int c) : code(c) { ctors++; }
    Base(const Base &o) : code(o.code) { copies++; }
    virtual ~Base() { dtors++; }
};
struct Derived : Base {
    Derived(int c) : Base(c) {}
};

__declspec(noinline) void throw_int(int v) { Guard g; throw v; }
__declspec(noinline) void throw_derived(int v) { Guard g; throw Derived(v); }

__declspec(noinline) int t_destructors() {
    ctors = dtors = 0;
    try { Guard a; throw_int(5); } catch (int v) { return (v == 5 && dtors == 2 && ctors == 2) ? 0 : 1; }
    return 1;
}

__declspec(noinline) int t_select_type() {
    try { throw_int(9); }
    catch (const Base &) { return 1; }
    catch (long) { return 1; }
    catch (int v) { return v == 9 ? 0 : 1; }
    return 1;
}

__declspec(noinline) int t_base_by_ref() {
    ctors = dtors = copies = 0;
    try { throw_derived(3); }
    catch (Base &b) { if (b.code != 3) return 1; }
    // l'objet lancé est détruit à la fin du catch (et le Guard)
    return (dtors == 2) ? 0 : 1;
}

__declspec(noinline) int t_by_value_copy() {
    copies = 0;
    try { throw_derived(4); }
    catch (Base b) { if (b.code != 4 || copies != 1) return 1; }
    return 0;
}

__declspec(noinline) int t_catch_all() {
    try { throw_int(1); } catch (...) { return 0; }
    return 1;
}

static int rethrows;
__declspec(noinline) void rethrower() {
    try { throw_int(7); } catch (int) { rethrows++; throw; }
}
__declspec(noinline) int t_rethrow() {
    rethrows = 0;
    try { rethrower(); } catch (int v) { return (v == 7 && rethrows == 1) ? 0 : 1; }
    return 1;
}

__declspec(noinline) int t_throw_from_catch() {
    try {
        try { throw_int(1); } catch (int) { throw_int(2); }
    } catch (int v) { return v == 2 ? 0 : 1; }
    return 1;
}

__declspec(noinline) int t_nested_in_catch() {
    int r = 1;
    try { throw_int(1); }
    catch (int) {
        try { throw_int(2); } catch (int v) { r = (v == 2) ? 0 : 1; }
    }
    return r;
}

static int filter_seen;
static int filt(unsigned code) { filter_seen = (int)code; return code == 0xE0001234 ? 1 : 0; }
__declspec(noinline) void raise_custom() { RaiseException(0xE0001234, 0, 0, 0); }
__declspec(noinline) int t_seh_except() {
    __try { raise_custom(); return 1; }
    __except (filt(__exception_code())) { return filter_seen == (int)0xE0001234 ? 0 : 1; }
}

static int finally_ran;
__declspec(noinline) void finally_inner() {
    __try { raise_custom(); }
    __finally { finally_ran++; }
}
__declspec(noinline) int t_seh_finally() {
    finally_ran = 0;
    __try { finally_inner(); }
    __except (1) { return finally_ran == 1 ? 0 : 1; }
    return 1;
}

extern "C" int entry() {
    int (*tests[])() = {t_destructors, t_select_type, t_base_by_ref, t_by_value_copy, t_catch_all,
                        t_rethrow, t_throw_from_catch, t_nested_in_catch, t_seh_except, t_seh_finally};
    for (unsigned i = 0; i < sizeof tests / sizeof *tests; i++)
        if (tests[i]()) ExitProcess(i + 1);
    ExitProcess(0);
    return 0;
}

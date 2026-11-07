// Exceptions C++ MSVC d'un programme x86 (#232, étape 7) : les tables que
// produit clang en cible i686-pc-windows-msvc, gérées par
// msvcrt!__CxxFrameHandler3 et _CxxThrowException de Winlux, comme celles de
// 7-Zip ou des installeurs MSVC 32 bits. Sans CRT ni en-têtes : kernel32 et
// msvcrt seulement. Rend 0 si tout va bien, sinon le numéro du cas en échec.
typedef unsigned int DWORD;
extern "C" {
__declspec(dllimport) void *__stdcall GetStdHandle(DWORD);
__declspec(dllimport) int __stdcall WriteFile(void *, const void *, DWORD, DWORD *, void *);
__declspec(dllimport) void __stdcall ExitProcess(DWORD);
__declspec(dllimport) int __stdcall lstrlenA(const char *);
}

// Table virtuelle de type_info, référencée par chaque TypeDescriptor : seule
// son adresse compte ici.
asm(".data\n.globl \"??_7type_info@@6B@\"\n\"??_7type_info@@6B@\":\n.long 0\n.text\n");

// Ni CRT ni bibliothèque C++ : delete (destructeurs virtuels) et __fltused.
void operator delete(void *) noexcept {}
void operator delete(void *, unsigned int) noexcept {}
extern "C" int _fltused = 0;

static void say(const char *s) {
    DWORD n = 0;
    WriteFile(GetStdHandle((DWORD)-11), s, lstrlenA(s), &n, 0);
}

static void fail(int n) {
    char m[] = "echec n. 00\r\n";
    m[9] = '0' + n / 10;
    m[10] = '0' + n % 10;
    say(m);
    ExitProcess(n);
}

static int live = 0;      // objets Guard vivants
static int copies = 0;    // constructeurs de copie de Boom

struct Guard {
    Guard() { ++live; }
    ~Guard() { --live; }
};

struct Base {
    int code;
    Base(int c) : code(c) {}
    virtual ~Base() {}
    virtual int kind() const { return 1; }
};

struct Boom : Base {
    Guard g;
    Boom(int c) : Base(c) {}
    Boom(const Boom &o) : Base(o.code) { ++copies; }
    int kind() const override { return 2; }
};

__attribute__((noinline)) static void deep(int depth, int what) {
    Guard g;
    if (depth > 0) {
        deep(depth - 1, what);
        return;
    }
    if (what == 1) throw 42;
    if (what == 2) throw Boom(7);
    if (what == 3) throw 3.5;
}

__attribute__((noinline)) static int rethrow_inner() {
    try {
        deep(2, 1);
    } catch (int v) {
        if (v != 42) fail(10);
        throw;
    }
    return 0;
}

__attribute__((noinline)) static int nested_in_catch() {
    int r = 0;
    try {
        deep(1, 1);
    } catch (int) {
        try {
            deep(1, 2);
        } catch (Base &b) {
            r = b.code;
        }
    }
    return r;
}

extern "C" __attribute__((used)) void __stdcall entry() {
    // 1-2 : entier, trois cadres déroulés, leurs Guard détruits.
    try {
        deep(3, 1);
        fail(1);
    } catch (int v) {
        if (v != 42) fail(1);
    }
    if (live != 0) fail(2);

    // 3-5 : objet dérivé capturé par référence vers la base (virtuelle),
    // puis détruit après le catch.
    try {
        deep(2, 2);
        fail(3);
    } catch (const Base &b) {
        if (b.code != 7 || b.kind() != 2) fail(4);
    }
    if (live != 0) fail(5);

    // 6-7 : capture par valeur : constructeur de copie appelé.
    copies = 0;
    try {
        deep(0, 2);
    } catch (Boom b) {
        if (b.code != 7 || copies != 1) fail(6);
    }
    if (live != 0) fail(7);

    // 8 : catch(...) et type sans correspondance ignoré.
    int got = 0;
    try {
        try {
            deep(1, 3);
        } catch (int) {
            fail(8);
        }
    } catch (...) {
        got = 1;
    }
    if (got != 1 || live != 0) fail(8);

    // 10-11 : throw; dans un catch, repris par l'appelant.
    try {
        rethrow_inner();
        fail(11);
    } catch (int v) {
        if (v != 42) fail(11);
    }
    if (live != 0) fail(11);

    // 12 : try/catch dans un catch.
    if (nested_in_catch() != 7 || live != 0) fail(12);

    // 13 : plusieurs centaines d'exceptions, la pile ne grossit pas.
    for (int i = 0; i < 500; i++) {
        try {
            deep(2, i % 2 ? 1 : 2);
        } catch (int) {
        } catch (Base &) {
        }
    }
    if (live != 0) fail(13);

    say("C++ MSVC 32 bits : tout est correct\r\n");
    ExitProcess(0);
}

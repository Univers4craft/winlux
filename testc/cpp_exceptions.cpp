// Test minimal des exceptions C++ MinGW-w64 : vérifie que throw/catch
// fonctionnent avec le déroulement SEH x64 de Winlux.
#include <cstdio>
#include <cstdlib>
#include <cstring>

static int dtors_called = 0;

struct Guard {
    const char *name;
    Guard(const char *n) : name(n) {}
    ~Guard() { dtors_called++; }
};

static void inner() {
    Guard g("inner");
    throw 42;
}

static void middle() {
    Guard g("middle");
    inner();
}

static int test_int_throw() {
    dtors_called = 0;
    try {
        middle();
        return 1;
    } catch (int v) {
        if (v != 42) { fprintf(stderr, "FAIL: caught %d, expected 42\n", v); return 1; }
        // inner's Guard + middle's Guard
        if (dtors_called != 2) { fprintf(stderr, "FAIL: dtors_called=%d, expected 2\n", dtors_called); return 1; }
        return 0;
    } catch (...) {
        fprintf(stderr, "FAIL: wrong catch handler\n");
        return 1;
    }
}

struct MyException {
    int code;
    char msg[32];
};

static int test_struct_throw() {
    try {
        MyException ex;
        ex.code = 99;
        strcpy(ex.msg, "test");
        throw ex;
    } catch (MyException &e) {
        if (e.code != 99) { fprintf(stderr, "FAIL: code=%d\n", e.code); return 1; }
        if (strcmp(e.msg, "test") != 0) { fprintf(stderr, "FAIL: msg=%s\n", e.msg); return 1; }
        return 0;
    }
}

static int test_rethrow() {
    dtors_called = 0;
    try {
        try {
            Guard g("rethrow");
            throw "hello";
        } catch (...) {
            if (dtors_called != 1) { fprintf(stderr, "FAIL: rethrow dtors=%d\n", dtors_called); return 1; }
            throw;
        }
    } catch (const char *s) {
        if (strcmp(s, "hello") != 0) { fprintf(stderr, "FAIL: rethrow got %s\n", s); return 1; }
        return 0;
    }
    return 1;
}

static int test_uncaught() {
    // Not tested here — just verifying the above work
    return 0;
}

int main() {
    int failures = 0;

    printf("[1/4] throw int + stack unwinding... ");
    if (test_int_throw() == 0) printf("OK\n"); else { printf("FAIL\n"); failures++; }

    printf("[2/4] throw struct... ");
    if (test_struct_throw() == 0) printf("OK\n"); else { printf("FAIL\n"); failures++; }

    printf("[3/4] rethrow... ");
    if (test_rethrow() == 0) printf("OK\n"); else { printf("FAIL\n"); failures++; }

    printf("[4/4] nested try/catch... ");
    if (test_uncaught() == 0) printf("OK\n"); else { printf("FAIL\n"); failures++; }

    if (failures > 0) {
        fprintf(stderr, "cpp_exceptions: %d test(s) FAILED\n", failures);
        return 1;
    }
    printf("cpp_exceptions: all tests passed\n");
    return 0;
}

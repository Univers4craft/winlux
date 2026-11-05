/* Programme x86 qui importe dll32.dll et charge dll32b.dll (#232, étape 4). */
#include <windows.h>
#include <stdio.h>
__declspec(dllimport) int __stdcall dll_add(int a, int b);
__declspec(dllimport) int dll_cdecl(int a);
__declspec(dllimport) int dll_threads(void);
__declspec(dllimport) int dll_tls(int v);
__declspec(dllimport) int dll_value;
static int fails;
#define CHECK(n, c) do { if (!(c)) { printf("echec %d : %s\n", n, #c); fails = n; } } while (0)
static DWORD WINAPI th(LPVOID p) { (void)p; return dll_tls(9); }
int main(void) {
    CHECK(1, dll_add(2, 3) == 1005);
    CHECK(2, dll_cdecl(7) == 21);
    CHECK(3, dll_value == 1234);
    CHECK(4, dll_tls(6) == 5 && dll_tls(7) == 6);
    HANDLE t = CreateThread(NULL, 0, th, NULL, 0, NULL);
    WaitForSingleObject(t, INFINITE);
    DWORD code = 0; GetExitCodeThread(t, &code); CloseHandle(t);
    CHECK(5, code == 5);
    CHECK(6, dll_threads() >= 1);
    HMODULE m = LoadLibraryA("dll32b.dll");
    CHECK(7, m != NULL);
    if (m) {
        int (__stdcall *add)(int, int) = (int (__stdcall *)(int, int))GetProcAddress(m, "dll_add@8");
        int *val = (int *)GetProcAddress(m, "dll_value");
        CHECK(8, add && add(1, 1) == 1002);
        CHECK(9, val && *val == 1234);
        CHECK(10, FreeLibrary(m));
    }
    CHECK(11, GetModuleHandleA("dll32.dll") != NULL);
    if (!fails) printf("DLL 32 bits : tout est correct\n");
    return fails;
}

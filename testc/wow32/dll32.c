/* DLL x86 de test (#232, étape 4) : DllMain (attache du processus et des
 * threads), exports stdcall, cdecl et donnée, TLS statique propre. Copiée
 * en dll32b.dll pour LoadLibrary (image déplacée : même adresse préférée). */
#include <windows.h>
static int attached, thread_attached;
static _Thread_local int per_thread = 5;
BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, LPVOID r) {
    (void)h; (void)r;
    if (reason == DLL_PROCESS_ATTACH) attached++;
    if (reason == DLL_THREAD_ATTACH) thread_attached++;
    return TRUE;
}
__declspec(dllexport) int __stdcall dll_add(int a, int b) { return a + b + attached * 1000; }
__declspec(dllexport) int dll_cdecl(int a) { return a * 3; }
__declspec(dllexport) int dll_threads(void) { return thread_attached; }
__declspec(dllexport) int dll_tls(int v) { int old = per_thread; per_thread = v; return old; }
__declspec(dllexport) int dll_value = 1234;

/* #3172 : installateur minimal qui demande les droits d'administrateur
 * (manifeste requireAdministrator, posé par lld-link /manifestuac). Le
 * script de test lui colle des centaines de Mo de données, comme un
 * installateur réel ; il sort aussitôt avec le code 42. */
__declspec(dllimport) void __stdcall ExitProcess(unsigned int code);

void entry(void) {
    ExitProcess(42);
}

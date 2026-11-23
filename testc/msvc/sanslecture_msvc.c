/* #3189 : programme ordinaire minimal. Le script de test lui colle des
 * centaines de Mo de données (gros programme, ou installateur d'un format
 * que Winlux ne reconnaît pas) et sert aussi d'amorce aux faux installateurs
 * 7z (module SFX) et Inno (ressource 11111) ; il sort aussitôt avec le
 * code 42. */
__declspec(dllimport) void __stdcall ExitProcess(unsigned int code);

void entry(void) {
    ExitProcess(42);
}

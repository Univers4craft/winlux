/* Fonctions demandées par de vrais jeux Windows 64 bits (SuperTuxKart 1.5,
 * Warzone 2100 4.7) et leurs bibliothèques : SDL, libpng/freetype
 * (setjmp), libcurl, OpenAL, libc++. Compilé deux fois : avec msvcrt
 * (comme SuperTuxKart) et avec l'UCRT (comme Warzone 2100). Voir docs/VRAIS-JEUX.md. */
#include <ctype.h>
#include <direct.h>
#include <errno.h>
#include <fcntl.h>
#include <io.h>
#include <math.h>
#include <setjmp.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <wchar.h>
#include <wctype.h>
#include <windows.h>
#include <windns.h>
#include <winhttp.h>
#include <shlobj.h>
#include <wincrypt.h>

static int failures;
#define CHECK(name, cond) do { if (cond) printf("  OK     %s\n", name); \
    else { printf("  ÉCHEC  %s (ligne %d)\n", name, __LINE__); failures++; } } while (0)

/* GCC prend à tort la récursion bornée par longjmp pour une boucle infinie. */
#pragma GCC diagnostic ignored "-Winfinite-recursion"
static jmp_buf jump;
static volatile int depth;
static void __attribute__((noinline)) deep(int n) {
    depth = n;
    if (n > 0) deep(n - 1);
    else longjmp(jump, 7);
}

/* Ligne de commande brute (lue par Warzone 2100 via __p__acmdln). */
__declspec(dllimport) char **__cdecl __p__acmdln(void);
/* hid.dll : déclaré à la main, comme le fait SDL. */
__declspec(dllimport) void __stdcall HidD_GetHidGuid(GUID *guid);

int main(void) {
    printf("== games_suite (%s) ==\n",
#ifdef _UCRT
           "UCRT"
#else
           "msvcrt"
#endif
    );

    /* setjmp / longjmp (libpng, freetype, libjpeg) */
    volatile double kept = 1.5;
    int r = setjmp(jump);
    if (r == 0) deep(20);
    CHECK("setjmp/longjmp depuis 20 appels", r == 7 && depth == 0 && kept == 1.5);

    /* Mathématiques C99 */
    CHECK("lrint/lround", lrint(2.5) == 2 && lround(2.5) == 3 && lroundf(-1.5f) == -2);
    CHECK("exp2/expm1/log1p", exp2(10.0) == 1024.0 && fabs(expm1(1e-10) - 1e-10) < 1e-20 && fabs(log1p(1e-10) - 1e-10) < 1e-20);
    CHECK("nextafterf/remainderf", nextafterf(1.0f, 2.0f) > 1.0f && remainderf(7.0f, 4.0f) == -1.0f);
    CHECK("hypot/logb", hypot(3.0, 4.0) == 5.0 && logb(8.0) == 3.0);
    char *end;
    CHECK("strtof", strtof("2.5x", &end) == 2.5f && *end == 'x');
    wchar_t *wend;
    CHECK("wcstod", wcstod(L" -1.25e2z", &wend) == -125.0 && *wend == L'z');
    CHECK("wcstoll", wcstoll(L"-123456789012", NULL, 10) == -123456789012LL);

    /* Multioctets et classes de caractères */
    wchar_t wc = 0;
    mbstate_t st;
    memset(&st, 0, sizeof st);
    CHECK("mbrtowc", mbrtowc(&wc, "A", 1, &st) == 1 && wc == L'A');
    char mb[8];
    CHECK("wcrtomb", wcrtomb(mb, L'z', &st) == 1 && mb[0] == 'z');
    CHECK("iswctype/iswalpha", iswalpha(L'é') && !iswdigit(L'x') && iswdigit(L'7') && iswspace(L'\t'));
    CHECK("towupper", towupper(L'é') == L'É');
    char rev[] = "abc";
    CHECK("_strrev", strcmp(_strrev(rev), "cba") == 0);
    char dst[4];
    CHECK("memcpy_s refuse le débordement", memcpy_s(dst, sizeof dst, "12345", 5) != 0 && memcpy_s(dst, sizeof dst, "123", 4) == 0);
    size_t len = 0;
    char val[64];
    CHECK("getenv_s", getenv_s(&len, val, sizeof val, "WINLUX_GAMES_TEST") == 0 && strcmp(val, "jeu") == 0 && len == 4);

    /* Ligne de commande brute */
    char *cmdline = *__p__acmdln();
    CHECK("__p__acmdln", cmdline && strstr(cmdline, "games_suite") != NULL);

    /* Fichiers : chemins larges, stat 64 bits, recherche */
    CHECK("_wmkdir", _wmkdir(L"dossier_jeu") == 0);
    FILE *f = _wfopen(L"dossier_jeu\\sauvegarde.dat", L"wb");
    CHECK("_wfopen", f != NULL);
    if (f) { fwrite("0123456789", 1, 10, f); fclose(f); }
    struct _stat64 s64;
    CHECK("_wstat64 fichier", _wstat64(L"dossier_jeu\\sauvegarde.dat", &s64) == 0 && s64.st_size == 10 && (s64.st_mode & _S_IFREG));
    CHECK("_stat64 dossier", _stat64("dossier_jeu", &s64) == 0 && (s64.st_mode & _S_IFDIR));
    CHECK("_wstat64 absent", _wstat64(L"dossier_jeu\\absent", &s64) == -1 && errno == ENOENT);
    struct _wfinddata64_t fd;
    intptr_t h = _wfindfirst64(L"dossier_jeu\\*.dat", &fd);
    CHECK("_wfindfirst64", h != -1 && wcscmp(fd.name, L"sauvegarde.dat") == 0 && fd.size == 10);
    if (h != -1) {
        CHECK("_wfindnext64 en fin de liste", _wfindnext64(h, &fd) == -1);
        _findclose(h);
    }
    WIN32_FIND_DATAW w32;
    HANDLE fh = FindFirstFileW(L"dossier_jeu\\*.dat", &w32);
    CHECK("FindFirstFileW : cFileName à sa place", fh != INVALID_HANDLE_VALUE && wcscmp(w32.cFileName, L"sauvegarde.dat") == 0 && w32.nFileSizeLow == 10);
    if (fh != INVALID_HANDLE_VALUE) FindClose(fh);
    wchar_t full[MAX_PATH], cwd[MAX_PATH];
    CHECK("_wgetcwd", _wgetcwd(cwd, MAX_PATH) != NULL && cwd[1] == L':');
    CHECK("_wfullpath", _wfullpath(full, L"dossier_jeu\\..\\dossier_jeu\\sauvegarde.dat", MAX_PATH) != NULL
          && wcsstr(full, L"\\dossier_jeu\\sauvegarde.dat") != NULL && wcsstr(full, L"..") == NULL);
    int fdd = _wopen(L"dossier_jeu\\sauvegarde.dat", _O_RDONLY | _O_BINARY);
    CHECK("_filelengthi64", fdd >= 0 && _filelengthi64(fdd) == 10);
    FILE *fs = fdd >= 0 ? _fdopen(fdd, "rb") : NULL;
    char head[4] = {0};
    CHECK("_fdopen", fs && fread(head, 1, 3, fs) == 3 && strcmp(head, "012") == 0);
    if (fs) fclose(fs);
    FILE *log = fopen("dossier_jeu\\journal.txt", "w");
    FILE *re = log ? freopen("dossier_jeu\\journal2.txt", "w", log) : NULL;
    if (re) { fputs("ok", re); fclose(re); }
    CHECK("freopen", re != NULL && _access("dossier_jeu\\journal2.txt", 0) == 0);
    CHECK("SHCreateDirectoryExW", SHCreateDirectoryExW(NULL, L"dossier_jeu\\a\\b\\c", NULL) == ERROR_SUCCESS
          && GetFileAttributesW(L"dossier_jeu\\a\\b\\c") != INVALID_FILE_ATTRIBUTES);
    _wremove(L"dossier_jeu\\sauvegarde.dat");
    _wremove(L"dossier_jeu\\journal.txt");
    _wremove(L"dossier_jeu\\journal2.txt");
    _wrmdir(L"dossier_jeu\\a\\b\\c");
    _wrmdir(L"dossier_jeu\\a\\b");
    _wrmdir(L"dossier_jeu\\a");
    CHECK("_wremove/_wrmdir", _wrmdir(L"dossier_jeu") == 0);

    /* kernel32 */
    FILETIME a = {1, 0}, b = {2, 0};
    CHECK("CompareFileTime", CompareFileTime(&a, &b) == -1 && CompareFileTime(&b, &a) == 1 && CompareFileTime(&a, &a) == 0);
    CHECK("GetThreadId", GetThreadId(GetCurrentThread()) == GetCurrentThreadId());
    int probe = 42, copy = 0;
    SIZE_T got = 0;
    CHECK("ReadProcessMemory (soi-même)", ReadProcessMemory(GetCurrentProcess(), &probe, &copy, sizeof copy, &got) && copy == 42 && got == sizeof copy);

    /* user32 : rectangles, messages de thread, entrée brute */
    RECT r1 = {0, 0, 10, 10}, r2 = {5, 5, 20, 20}, out;
    CHECK("IntersectRect", IntersectRect(&out, &r1, &r2) && out.left == 5 && out.right == 10);
    SetRectEmpty(&out);
    CHECK("SetRectEmpty", IsRectEmpty(&out));
    MSG msg;
    PeekMessageW(&msg, NULL, 0, 0, PM_NOREMOVE); /* crée la file du thread */
    CHECK("PostThreadMessageW", PostThreadMessageW(GetCurrentThreadId(), WM_APP + 3, 11, 22));
    CHECK("message de thread reçu", PeekMessageW(&msg, NULL, WM_APP + 3, WM_APP + 3, PM_REMOVE) && msg.hwnd == NULL && msg.wParam == 11 && msg.lParam == 22);
    RAWINPUTDEVICE rid = {1, 2, 0, NULL};
    CHECK("RegisterRawInputDevices", RegisterRawInputDevices(&rid, 1, sizeof rid));
    UINT count = 99;
    CHECK("GetRawInputDeviceList", GetRawInputDeviceList(NULL, &count, sizeof(RAWINPUTDEVICELIST)) == 0 && count == 0);

    /* hid, dnsapi, crypto, winhttp */
    GUID hid;
    memset(&hid, 0, sizeof hid);
    HidD_GetHidGuid(&hid);
    CHECK("HidD_GetHidGuid", hid.Data1 == 0x4D1E55B2 && hid.Data4[7] == 0x30);
    PDNS_RECORD rec = NULL;
    DNS_STATUS ds = DnsQuery_W(L"nom-qui-n-existe-pas.invalid", DNS_TYPE_SRV, DNS_QUERY_STANDARD, NULL, &rec, NULL);
    CHECK("DnsQuery_W (nom inexistant)", ds != 0 && rec == NULL);
    HCRYPTPROV prov;
    HCRYPTHASH hash;
    BYTE digest[16];
    DWORD dlen = sizeof digest;
    BOOL hashed = CryptAcquireContextW(&prov, NULL, NULL, PROV_RSA_FULL, CRYPT_VERIFYCONTEXT)
        && CryptCreateHash(prov, CALG_MD5, 0, 0, &hash) && CryptHashData(hash, (const BYTE *)"abc", 3, 0)
        && CryptGetHashParam(hash, HP_HASHVAL, digest, &dlen, 0);
    CHECK("CryptoAPI MD5", hashed && dlen == 16 && digest[0] == 0x90 && digest[15] == 0x72);
    URL_COMPONENTSW uc;
    memset(&uc, 0, sizeof uc);
    uc.dwStructSize = sizeof uc;
    uc.dwHostNameLength = uc.dwUrlPathLength = 1;
    BOOL cracked = WinHttpCrackUrl(L"https://exemple.org:8080/jeu/maj.json", 0, 0, &uc);
    CHECK("WinHttpCrackUrl", cracked && uc.nPort == 8080 && uc.nScheme == INTERNET_SCHEME_HTTPS
          && uc.dwHostNameLength == 11 && wcsncmp(uc.lpszHostName, L"exemple.org", 11) == 0
          && wcsncmp(uc.lpszUrlPath, L"/jeu/maj.json", uc.dwUrlPathLength) == 0);

    printf("%s\n", failures ? "games_suite : des tests ont échoué" : "games_suite : tout est bon");
    return failures;
}

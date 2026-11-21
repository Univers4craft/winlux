# msvcr120 : fonctions sécurisées *_s (gldriverquery64.exe de Steam, #1980).
_c = (ROOT / "runtime/src/dlls/crt_secure.rs").read_text()
_w = (ROOT / "runtime/src/wow32/crt.rs").read_text()
for _n in ("sscanf_s", "sprintf_s", "printf_s", "vsnprintf_s", "fscanf_s", "asctime_s",
           "_ctime64_s", "tmpnam_s", "fread_s", "qsort_s", "mbsrtowcs_s", "_dupenv_s"):
    ok(f'"{_n}" => fn ' in _c, f"msvcr120 exporte {_n} (#1980)")
ok("fn scanf_s_x86_garde_la_taille_apres_chaque_tampon(" in _w,
   "x86 : sscanf_s en mode sécurisé (taille après %s), avec test (#1980)")
ok("fn qsort_s32(" in _w, "x86 : qsort_s passe son contexte au comparateur (#1980)")
ok("fn sscanf_s_lit_avec_la_taille()" in _c and "fn famille_s_servie_par_msvcrt()" in _c,
   "tests : famille *_s servie, sscanf_s avec taille (#1980)")

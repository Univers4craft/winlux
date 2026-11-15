# #2056 : pas d'icu.dll partielle (Paint.NET avortait sur u_charsToUChars).
step "Pas d'icu.dll partielle pour .NET (#2056)" cargo test -q -p winlux -- pas_d_icu_dll_partielle

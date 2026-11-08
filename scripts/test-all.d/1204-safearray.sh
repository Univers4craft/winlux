# SAFEARRAY à la disposition publique de oaidl.h (#1204, WMI, marshaling .NET) :
# champs lus directement, bornes dans l'ordre de Windows, BSTR copiées.
step "SAFEARRAY : structure publique et copie des chaînes (#1204)" cargo test -q -p winlux -- safearray_a_la_disposition_publique_et_copie_les_chaines safearray_bornes_dans_l_ordre_de_windows

# #1609 : GetTarget d'une cible HDC la rétablit, et BindDC rétablit le HDC mis de côté.
step "Direct2D : GetTarget et BindDC rétablissent le HDC (#1609)" cargo test -q -p winlux -- cible_hdc_get_target_et_bind_dc_retablissent_le_hdc

# #1368 : boutons de Paint.NET dessinés en Direct2D sur un fond GDI.
step "Direct2D : la cible de DC garde le fond GDI (#1368)" cargo test -q -p winlux -- cible_compatible_et_cible_de_dc

# #1761 : palette flottante (WS_EX_TOOLWINDOW) reconnue comme fenêtre outil.
step "palettes flottantes de Paint.NET sans Réduire ni Agrandir (#1761)" cargo test -q -p winlux -- user32::tests::palette_flottante_reconnue

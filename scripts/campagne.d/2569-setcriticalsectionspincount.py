# #2569 : kernel32!SetCriticalSectionSpinCount est servie et rend l'ancien
# compte. Le comportement est gardé par le test unitaire nommé.
t = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
ok("fn compte_de_rotation_d_une_section_critique" in t, "SetCriticalSectionSpinCount servie par kernel32 (#2569)")

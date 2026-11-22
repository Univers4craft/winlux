# #2876 : SetWindowSubclass d'un programme 32 bits (Notepad++) appelait la procédure x86 en 64 bits.
step "sous-classement comctl32 en x86 : SUBCLASSPROC à six arguments (#2876)" sh scripts/test-subclass32.sh

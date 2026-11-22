# #2918 : winlux --inspect projette le programme au lieu de le copier en
# mémoire (300 Mo inspectés sous une limite de 150 Mo de données).
step "winlux --inspect : gros .exe projeté, pas copié (#2918)" sh scripts/test-inspect-projete.sh

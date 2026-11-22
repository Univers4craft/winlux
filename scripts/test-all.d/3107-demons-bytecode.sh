# #3107 : les démons Python résidents sont lancés depuis leur bytecode en
# cache (winlux-python) : même script, mêmes noms, moins de mémoire privée.
step "Démons Python lancés depuis leur bytecode en cache, moins de mémoire privée (#3107)" /usr/bin/python3 -B scripts/test-python-bytecode.py

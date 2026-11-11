"""Programme de test PyInstaller win32 (#1346).

Écrit sur la console, ouvre une fenêtre tkinter, la ferme seule après deux
secondes, puis écrit « PYI32 OK » et rend 0.
"""
import math
import sys
import tkinter as tk

print("PYI32 début", sys.version, flush=True)
racine = tk.Tk()
racine.title("PyInstaller 32 bits")
tk.Label(racine, text="Winlux : PyInstaller win32").pack(padx=24, pady=24)
racine.after(2000, racine.destroy)
racine.mainloop()
# Fonctions mathématiques du CRT x86 (#1333).
assert abs(math.sin(math.pi / 2) - 1.0) < 1e-12
assert abs(math.pow(2.0, 10.0) - 1024.0) < 1e-9
assert abs(math.fmod(7.5, 2.0) - 1.5) < 1e-12
print("PYI32 OK", flush=True)
sys.exit(0)

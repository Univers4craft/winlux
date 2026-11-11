# #1677 : une fenêtre « message seulement » rattachée par SetParent à une vraie fenêtre reçoit une fenêtre X (ToolStrip de Paint.NET).
src = (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
ok("fn leave_message_only" in src and "leave_message_only(hwnd, new_parent)" in src,
   "SetParent : une fenêtre sortie de HWND_MESSAGE devient affichable (#1677)")

# #1712 : HSTRING à la disposition commune (C++/WinRT, windows-rs).
c = (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8")
ok("HSTRING_MAGIC" not in c and "fn hstring_count<" in c,
   "HSTRING bâti par le programme lu sans signature propre à Winlux (#1712)")
# Noms : hstring_alloc, l'allocateur kernel32::heap_alloc et le test chaines_winrt (#2711).
ok("fn hstring_alloc(" in c and "kernel32::heap_alloc" in c and "fn chaines_winrt" in c,
   "HSTRING alloué sur le tas du processus, dupliqué par compteur de références (#1712)")

# #1355 : RoGetActivationFactory connaît Windows.System.DispatcherQueue.
ok("pub unsafe fn activation_factory" in (runtime_dlls / "coremessaging.rs").read_text(encoding="utf-8")
   and "coremessaging::activation_factory" in (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8"),
   "Fabrique WinRT de DispatcherQueue (#1355)")

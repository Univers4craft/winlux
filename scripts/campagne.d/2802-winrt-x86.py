# #2802 : un programme x86 reçoit les fabriques et les objets WinRT du
# runtime (RoGetActivationFactory, RoActivateInstance) par des mandataires
# IInspectable et IActivationFactory. Comportement gardé par le test
# wow32::com_winrt::winrt_decrit_pour_le_code_x86.
winrt_2802 = (ROOT / "runtime" / "src" / "wow32" / "com_winrt.rs").read_text(encoding="utf-8")
com_2802 = (ROOT / "runtime" / "src" / "wow32" / "com.rs").read_text(encoding="utf-8")
ok("static IACTIVATIONFACTORY: Iface" in winrt_2802
   and "fn winrt_decrit_pour_le_code_x86" in winrt_2802
   and "com_winrt::ALL" in com_2802,
   "WinRT pour un programme x86 : IInspectable et IActivationFactory décrites (#2802)")

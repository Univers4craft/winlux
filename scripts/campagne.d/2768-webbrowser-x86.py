# #2768 : un programme x86 qui héberge le contrôle WebBrowser (installeur de
# Creative Cloud) reçoit IOleObject, IOleInPlaceObject et IWebBrowser2 par
# des mandataires, et le contrôle appelle son site (IOleClientSite,
# IOleInPlaceSite) par des mandataires. Comportement gardé par le test
# wow32::com_webbrowser::controle_webbrowser_decrit_pour_le_code_x86.
wb_2768 = (ROOT / "runtime" / "src" / "wow32" / "com_webbrowser.rs").read_text(encoding="utf-8")
com_2768 = (ROOT / "runtime" / "src" / "wow32" / "com.rs").read_text(encoding="utf-8")
ok("static IWEBBROWSER2: Iface" in wb_2768
   and "fn controle_webbrowser_decrit_pour_le_code_x86" in wb_2768
   and "com_webbrowser::ALL" in com_2768,
   "contrôle WebBrowser hébergé par un programme x86 : interfaces décrites (#2768)")

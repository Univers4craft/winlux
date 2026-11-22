# Le contrôle WebBrowser rend IViewObject à un hôte x86 par un mandataire,
# avec l'abonné IAdviseSink de SetAdvise (#2971).
_wb = (ROOT / "runtime" / "src" / "wow32" / "com_webbrowser.rs").read_text(encoding="utf-8")
ok('name: "IViewObject"' in _wb and 'name: "IAdviseSink"' in _wb
   and "&IVIEWOBJECT," in _wb and "fn view_object_decrit_2971" in _wb,
   "IViewObject du contrôle WebBrowser et IAdviseSink sont décrites pour les programmes x86 (#2971)")

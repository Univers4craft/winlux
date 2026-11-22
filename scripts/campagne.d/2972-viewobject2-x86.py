# Le contrôle WebBrowser rend IViewObject2 à un hôte x86 par un mandataire
# (#2972).
_wb = (ROOT / "runtime" / "src" / "wow32" / "com_webbrowser.rs").read_text(encoding="utf-8")
ok('name: "IViewObject2"' in _wb and "&IVIEWOBJECT2," in _wb
   and "fn view_object2_decrit_2972" in _wb,
   "IViewObject2 du contrôle WebBrowser est décrite pour les programmes x86 (#2972)")

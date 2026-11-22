# #2788 : IXMLDOMNode::put_nodeTypedValue pose une chaîne comme contenu
# d'un élément sans type de données au lieu de rendre E_FAIL (Creative
# Cloud, « Error contructing CAD xml »). Comportement gardé par le test
# dlls::msxml::valeur_typee_d_un_element_2788.
msxml_2788 = (ROOT / "runtime" / "src" / "dlls" / "msxml.rs").read_text(encoding="utf-8")
ok("fn valeur_typee_d_un_element_2788" in msxml_2788
   and "fn n_put_node_typed_value" in msxml_2788,
   "MSXML : put_nodeTypedValue sur un élément pose son contenu (#2788)")

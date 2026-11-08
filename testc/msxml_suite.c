/* MSXML intégré (#1084), compilé par MinGW-w64.
 *
 * Crée un DOMDocument de chaque version (sans version, 2.6, 3.0, 4.0, 6.0),
 * puis : loadXML et parseError, documentElement, getAttribute,
 * selectNodes/selectSingleNode, createElement/appendChild/setAttribute,
 * texte Unicode, xml, save puis load d'un fichier, attributs (NamedNodeMap),
 * nextNode, cloneNode, removeChild, getElementsByTagName, insertBefore.
 *
 * Code de sortie 0 et « MSXML : tout est correct » si tout va bien.
 */
#define COBJMACROS
#include <windows.h>
#include <msxml2.h>
#include <stdio.h>
#include <wchar.h>

static int failures;
static void check(int ok, const char *what, HRESULT hr)
{
    if (!ok) {
        printf("ÉCHEC : %s (0x%08lx)\n", what, (unsigned long)hr);
        failures++;
    }
}

static int bstr_is(BSTR b, const wchar_t *want)
{
    int ok = b && wcscmp(b, want) == 0;
    if (!ok)
        printf("  obtenu : « %ls », attendu : « %ls »\n", b ? b : L"(nul)", want);
    SysFreeString(b);
    return ok;
}

static BSTR B(const wchar_t *s) { return SysAllocString(s); }

int main(void)
{
    CoInitializeEx(NULL, COINIT_APARTMENTTHREADED);
    const CLSID *classes[] = {&CLSID_DOMDocument, &CLSID_DOMDocument26, &CLSID_DOMDocument30,
                              &CLSID_DOMDocument40, &CLSID_DOMDocument60};
    for (int i = 0; i < 5; i++) {
        IXMLDOMDocument *d = NULL;
        HRESULT hr = CoCreateInstance(classes[i], NULL, CLSCTX_INPROC_SERVER, &IID_IXMLDOMDocument, (void **)&d);
        check(hr == S_OK && d, "CoCreateInstance(DOMDocument, toutes versions)", hr);
        if (d)
            IXMLDOMDocument_Release(d);
    }

    IXMLDOMDocument2 *doc = NULL;
    HRESULT hr = CoCreateInstance(&CLSID_DOMDocument60, NULL, CLSCTX_INPROC_SERVER, &IID_IXMLDOMDocument2,
                                  (void **)&doc);
    check(hr == S_OK && doc, "DOMDocument60 en IXMLDOMDocument2", hr);
    if (!doc)
        return 1;
    VARIANT_BOOL ok = VARIANT_TRUE;

    /* Document mal formé : parseError. */
    BSTR bad = B(L"<a>\n  <b></c>\n</a>");
    hr = IXMLDOMDocument2_loadXML(doc, bad, &ok);
    SysFreeString(bad);
    check(hr == S_FALSE && ok == VARIANT_FALSE, "loadXML refuse un document mal formé", hr);
    IXMLDOMParseError *err = NULL;
    IXMLDOMDocument2_get_parseError(doc, &err);
    long code = 0, line = 0;
    if (err) {
        IXMLDOMParseError_get_errorCode(err, &code);
        IXMLDOMParseError_get_line(err, &line);
        IXMLDOMParseError_Release(err);
    }
    check(code != 0 && line == 2, "parseError : code et ligne", code);

    BSTR text = B(L"<?xml version=\"1.0\"?>\n<config version=\"2\">\n"
                  L"  <server name=\"a\"><host>alpha</host></server>\n"
                  L"  <server name=\"b\"><host>bêta</host></server>\n</config>");
    hr = IXMLDOMDocument2_loadXML(doc, text, &ok);
    SysFreeString(text);
    check(hr == S_OK && ok == VARIANT_TRUE, "loadXML", hr);

    IXMLDOMElement *root = NULL;
    IXMLDOMDocument2_get_documentElement(doc, &root);
    check(root != NULL, "documentElement", 0);
    if (!root)
        return 1;
    BSTR name = NULL;
    IXMLDOMElement_get_tagName(root, &name);
    check(bstr_is(name, L"config"), "tagName", 0);
    VARIANT v;
    VariantInit(&v);
    BSTR an = B(L"version");
    hr = IXMLDOMElement_getAttribute(root, an, &v);
    check(hr == S_OK && v.vt == VT_BSTR && wcscmp(v.bstrVal, L"2") == 0, "getAttribute", hr);
    VariantClear(&v);
    SysFreeString(an);

    /* XPath. */
    IXMLDOMNodeList *list = NULL;
    BSTR q = B(L"/config/server/host");
    hr = IXMLDOMDocument2_selectNodes(doc, q, &list);
    SysFreeString(q);
    long len = 0;
    if (list)
        IXMLDOMNodeList_get_length(list, &len);
    check(hr == S_OK && len == 2, "selectNodes", hr);
    if (list) {
        IXMLDOMNode *n = NULL;
        BSTR t = NULL;
        IXMLDOMNodeList_nextNode(list, &n);
        IXMLDOMNodeList_nextNode(list, &n);
        if (n) {
            IXMLDOMNode_get_text(n, &t);
            IXMLDOMNode_Release(n);
        }
        check(bstr_is(t, L"bêta"), "nextNode puis text (Unicode)", 0);
        IXMLDOMNodeList_Release(list);
    }
    IXMLDOMNode *single = NULL;
    q = B(L"//server[@name='a']/host");
    hr = IXMLDOMElement_selectSingleNode(root, q, &single);
    SysFreeString(q);
    BSTR t = NULL;
    if (single) {
        IXMLDOMNode_get_text(single, &t);
        IXMLDOMNode_Release(single);
    }
    check(hr == S_OK && bstr_is(t, L"alpha"), "selectSingleNode avec prédicat", hr);

    /* Construction. */
    IXMLDOMElement *e = NULL;
    BSTR en = B(L"option");
    IXMLDOMDocument2_createElement(doc, en, &e);
    SysFreeString(en);
    check(e != NULL, "createElement", 0);
    if (e) {
        BSTR k = B(L"clé");
        VARIANT val;
        val.vt = VT_I4;
        val.lVal = 42;
        hr = IXMLDOMElement_setAttribute(e, k, val);
        check(hr == S_OK, "setAttribute (VT_I4 converti)", hr);
        SysFreeString(k);
        BSTR txt = B(L"a<b & \"c\"");
        IXMLDOMElement_put_text(e, txt);
        SysFreeString(txt);
        IXMLDOMNode *added = NULL;
        hr = IXMLDOMElement_appendChild(root, (IXMLDOMNode *)e, &added);
        check(hr == S_OK && added, "appendChild", hr);
        if (added)
            IXMLDOMNode_Release(added);
        BSTR x = NULL;
        IXMLDOMElement_get_xml(e, &x);
        check(bstr_is(x, L"<option clé=\"42\">a&lt;b &amp; \"c\"</option>"), "xml d'un élément construit", 0);

        IXMLDOMNamedNodeMap *attrs = NULL;
        IXMLDOMElement_get_attributes(e, &attrs);
        long na = 0;
        if (attrs) {
            IXMLDOMNamedNodeMap_get_length(attrs, &na);
            IXMLDOMNode *a0 = NULL;
            IXMLDOMNamedNodeMap_get_item(attrs, 0, &a0);
            BSTR nn = NULL;
            if (a0) {
                IXMLDOMNode_get_nodeName(a0, &nn);
                IXMLDOMNode_Release(a0);
            }
            check(bstr_is(nn, L"clé"), "NamedNodeMap::get_item", 0);
            IXMLDOMNamedNodeMap_Release(attrs);
        }
        check(na == 1, "attributs : longueur", 0);

        /* Copie profonde insérée en tête, puis retirée. */
        IXMLDOMNode *copy = NULL, *first = NULL, *ins = NULL, *gone = NULL;
        IXMLDOMElement_cloneNode(e, VARIANT_TRUE, &copy);
        IXMLDOMElement_get_firstChild(root, &first);
        VARIANT ref;
        ref.vt = VT_DISPATCH;
        ref.pdispVal = (IDispatch *)first;
        hr = IXMLDOMElement_insertBefore(root, copy, ref, &ins);
        check(hr == S_OK && ins, "insertBefore", hr);
        IXMLDOMNode *now_first = NULL;
        IXMLDOMElement_get_firstChild(root, &now_first);
        BSTR fn = NULL;
        if (now_first)
            IXMLDOMNode_get_nodeName(now_first, &fn);
        check(bstr_is(fn, L"option"), "la copie est première", 0);
        hr = IXMLDOMElement_removeChild(root, now_first, &gone);
        check(hr == S_OK && gone, "removeChild", hr);
        if (gone)
            IXMLDOMNode_Release(gone);
        if (now_first)
            IXMLDOMNode_Release(now_first);
        if (ins)
            IXMLDOMNode_Release(ins);
        if (first)
            IXMLDOMNode_Release(first);
        if (copy)
            IXMLDOMNode_Release(copy);
        IXMLDOMElement_Release(e);
    }
    IXMLDOMNodeList *byname = NULL;
    BSTR sn = B(L"server");
    IXMLDOMDocument2_getElementsByTagName(doc, sn, &byname);
    SysFreeString(sn);
    len = 0;
    if (byname) {
        IXMLDOMNodeList_get_length(byname, &len);
        IXMLDOMNodeList_Release(byname);
    }
    check(len == 2, "getElementsByTagName", 0);

    /* Enregistrement et relecture d'un fichier. */
    VARIANT file;
    file.vt = VT_BSTR;
    file.bstrVal = B(L"msxml-essai.xml");
    hr = IXMLDOMDocument2_save(doc, file);
    check(hr == S_OK, "save dans un fichier", hr);
    IXMLDOMDocument *again = NULL;
    CoCreateInstance(&CLSID_DOMDocument30, NULL, CLSCTX_INPROC_SERVER, &IID_IXMLDOMDocument, (void **)&again);
    if (again) {
        IXMLDOMDocument_put_async(again, VARIANT_FALSE);
        hr = IXMLDOMDocument_load(again, file, &ok);
        check(hr == S_OK && ok == VARIANT_TRUE, "load d'un fichier", hr);
        IXMLDOMNode *opt = NULL;
        q = B(L"/config/option/@clé");
        IXMLDOMDocument_selectSingleNode(again, q, &opt);
        SysFreeString(q);
        BSTR ov = NULL;
        if (opt) {
            IXMLDOMNode_get_text(opt, &ov);
            IXMLDOMNode_Release(opt);
        }
        check(bstr_is(ov, L"42"), "contenu relu après save/load", 0);
        VARIANT missing;
        missing.vt = VT_BSTR;
        missing.bstrVal = B(L"absent\\introuvable.xml");
        hr = IXMLDOMDocument_load(again, missing, &ok);
        check(hr == S_FALSE && ok == VARIANT_FALSE, "load d'un fichier absent", hr);
        VariantClear(&missing);
        IXMLDOMDocument_Release(again);
    }
    VariantClear(&file);
    DeleteFileW(L"msxml-essai.xml");

    IXMLDOMElement_Release(root);
    IXMLDOMDocument2_Release(doc);
    CoUninitialize();
    if (failures)
        return 1;
    printf("MSXML : tout est correct\n");
    return 0;
}

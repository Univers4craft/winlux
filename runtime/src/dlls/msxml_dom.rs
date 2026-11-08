//! Arbre DOM de MSXML (#1084) : analyse XML, sérialisation, opérations
//! sur les nœuds et sous-ensemble XPath de `selectNodes`.
//!
//! Écrit d'après la recommandation XML 1.0 du W3C, le DOM niveau 1 et la
//! documentation publique de MSXML (comportements de `loadXML`, `xml`,
//! `text`, `preserveWhiteSpace`, `parseError`). Les interfaces COM sont
//! dans msxml.rs.
//!
//! Les nœuds d'un document vivent dans une arène (`Doc::nodes`) ; un nœud
//! retiré de l'arbre reste dans l'arène, détaché, tant que le document vit.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Element = 1,
    Attribute = 2,
    Text = 3,
    CData = 4,
    Pi = 7,
    Comment = 8,
    Document = 9,
    Fragment = 11,
}

#[derive(Clone, Debug)]
pub struct NodeData {
    pub kind: Kind,
    /// Nom qualifié (élément, attribut), cible (instruction).
    pub name: String,
    /// Texte (texte, CDATA, commentaire, attribut, données d'instruction).
    pub value: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub attrs: Vec<usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParseError {
    pub code: i32,
    pub reason: String,
    pub src_text: String,
    pub line: i32,
    pub linepos: i32,
    pub filepos: i32,
    pub url: String,
}

pub struct Doc {
    pub nodes: Vec<NodeData>,
    pub preserve_ws: bool,
    pub validate_on_parse: bool,
    pub resolve_externals: bool,
    pub asynchronous: bool,
    pub error: ParseError,
    pub url: String,
    /// Propriétés de setProperty (nom, valeur).
    pub properties: Vec<(String, String)>,
}

pub const DOC: usize = 0;

// Codes de parseError, tels que la documentation MSXML les nomme.
pub const XML_E_UNEXPECTED_EOF: i32 = 0xC00C_E558u32 as i32;
pub const XML_E_ENDTAGMISMATCH: i32 = 0xC00C_E56Du32 as i32;
pub const XML_E_BADNAMECHAR: i32 = 0xC00C_E501u32 as i32;
pub const XML_E_MISSINGROOT: i32 = 0xC00C_E55Bu32 as i32;
pub const XML_E_MULTIPLEROOTS: i32 = 0xC00C_E556u32 as i32;
pub const XML_E_UNCLOSEDTAG: i32 = 0xC00C_E569u32 as i32;
pub const XML_E_INVALIDATROOTLEVEL: i32 = 0xC00C_E52Du32 as i32;
pub const XML_E_BADENTITY: i32 = 0xC00C_E50Du32 as i32;
pub const XML_E_FILENOTFOUND: i32 = 0x800C_0006u32 as i32;

impl Default for Doc {
    fn default() -> Self {
        Self::new()
    }
}

impl Doc {
    pub fn new() -> Doc {
        Doc {
            nodes: vec![NodeData {
                kind: Kind::Document,
                name: String::new(),
                value: String::new(),
                parent: None,
                children: Vec::new(),
                attrs: Vec::new(),
            }],
            preserve_ws: false,
            validate_on_parse: true,
            resolve_externals: false,
            asynchronous: true,
            error: ParseError::default(),
            url: String::new(),
            properties: Vec::new(),
        }
    }

    pub fn node(&self, id: usize) -> &NodeData {
        &self.nodes[id]
    }

    pub fn create(&mut self, kind: Kind, name: &str, value: &str) -> usize {
        self.nodes.push(NodeData {
            kind,
            name: name.to_string(),
            value: value.to_string(),
            parent: None,
            children: Vec::new(),
            attrs: Vec::new(),
        });
        self.nodes.len() - 1
    }

    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn set_property(&mut self, name: &str, value: &str) {
        match self
            .properties
            .iter_mut()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            Some(p) => p.1 = value.to_string(),
            None => self.properties.push((name.to_string(), value.to_string())),
        }
    }

    // ── Arbre ──────────────────────────────────────────────────────

    pub fn document_element(&self) -> Option<usize> {
        self.nodes[DOC]
            .children
            .iter()
            .copied()
            .find(|&c| self.nodes[c].kind == Kind::Element)
    }

    pub fn detach(&mut self, id: usize) {
        if let Some(p) = self.nodes[id].parent.take() {
            if self.nodes[id].kind == Kind::Attribute {
                self.nodes[p].attrs.retain(|&a| a != id);
            } else {
                self.nodes[p].children.retain(|&c| c != id);
            }
        }
    }

    /// `id` est-il `ancestor` ou l'un de ses descendants ?
    pub fn is_inside(&self, id: usize, ancestor: usize) -> bool {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self.nodes[c].parent;
        }
        false
    }

    /// Le nœud `child` peut-il être enfant de `parent` ?
    pub fn can_hold(&self, parent: usize, child: usize) -> bool {
        let pk = self.nodes[parent].kind;
        let ck = self.nodes[child].kind;
        if ck == Kind::Attribute || ck == Kind::Document || self.is_inside(parent, child) {
            return false;
        }
        match pk {
            Kind::Element | Kind::Fragment => true,
            Kind::Document => match ck {
                Kind::Element => self.document_element().is_none_or(|e| e == child),
                Kind::Text | Kind::CData => false,
                _ => true,
            },
            _ => false,
        }
    }

    /// Insère `child` dans `parent` avant `before` (à la fin si None). Un
    /// fragment est remplacé par ses enfants. Rend false si refusé.
    pub fn insert(&mut self, parent: usize, child: usize, before: Option<usize>) -> bool {
        if let Some(b) = before {
            if self.nodes[b].parent != Some(parent) || b == child {
                return before == Some(child);
            }
        }
        let items: Vec<usize> = if self.nodes[child].kind == Kind::Fragment {
            self.nodes[child].children.clone()
        } else {
            vec![child]
        };
        if items.iter().any(|&c| !self.can_hold(parent, c)) {
            return false;
        }
        for c in items {
            self.detach(c);
            let at = before
                .and_then(|b| self.nodes[parent].children.iter().position(|&x| x == b))
                .unwrap_or(self.nodes[parent].children.len());
            self.nodes[parent].children.insert(at, c);
            self.nodes[c].parent = Some(parent);
        }
        true
    }

    pub fn clone_node(&mut self, id: usize, deep: bool) -> usize {
        let src = self.nodes[id].clone();
        let copy = self.create(src.kind, &src.name, &src.value);
        for a in src.attrs {
            let ac = self.clone_node(a, true);
            self.nodes[ac].parent = Some(copy);
            self.nodes[copy].attrs.push(ac);
        }
        if deep {
            for c in src.children {
                let cc = self.clone_node(c, true);
                self.nodes[cc].parent = Some(copy);
                self.nodes[copy].children.push(cc);
            }
        }
        copy
    }

    /// Copie le sous-arbre `id` d'un autre document dans celui-ci.
    pub fn import(&mut self, other: &Doc, id: usize, deep: bool) -> usize {
        let src = &other.nodes[id];
        let kind = if src.kind == Kind::Document {
            Kind::Fragment
        } else {
            src.kind
        };
        let copy = self.create(kind, &src.name, &src.value);
        for &a in &src.attrs {
            let ac = self.import(other, a, true);
            self.nodes[ac].parent = Some(copy);
            self.nodes[copy].attrs.push(ac);
        }
        if deep {
            for &c in &src.children {
                let cc = self.import(other, c, true);
                self.nodes[cc].parent = Some(copy);
                self.nodes[copy].children.push(cc);
            }
        }
        copy
    }

    // ── Attributs ─────────────────────────────────────────────────

    pub fn attribute(&self, element: usize, name: &str) -> Option<usize> {
        self.nodes[element]
            .attrs
            .iter()
            .copied()
            .find(|&a| self.nodes[a].name == name)
    }

    pub fn set_attribute(&mut self, element: usize, name: &str, value: &str) {
        match self.attribute(element, name) {
            Some(a) => self.nodes[a].value = value.to_string(),
            None => {
                let a = self.create(Kind::Attribute, name, value);
                self.nodes[a].parent = Some(element);
                self.nodes[element].attrs.push(a);
            }
        }
    }

    /// Pose le nœud attribut `attr` sur `element` ; rend l'ancien.
    pub fn set_attribute_node(&mut self, element: usize, attr: usize) -> Option<usize> {
        let name = self.nodes[attr].name.clone();
        let old = self.attribute(element, &name);
        if old == Some(attr) {
            return None;
        }
        if let Some(o) = old {
            self.detach(o);
        }
        self.detach(attr);
        self.nodes[attr].parent = Some(element);
        self.nodes[element].attrs.push(attr);
        old
    }

    // ── Texte ─────────────────────────────────────────────────────

    /// Propriété `text` : texte du nœud ou de ses descendants.
    pub fn text(&self, id: usize) -> String {
        let n = &self.nodes[id];
        match n.kind {
            Kind::Text | Kind::CData | Kind::Comment | Kind::Pi | Kind::Attribute => {
                n.value.clone()
            }
            _ => {
                let mut out = String::new();
                self.collect_text(id, &mut out);
                if self.preserve_ws {
                    out
                } else {
                    out.trim().to_string()
                }
            }
        }
    }

    fn collect_text(&self, id: usize, out: &mut String) {
        for &c in &self.nodes[id].children {
            match self.nodes[c].kind {
                Kind::Text | Kind::CData => out.push_str(&self.nodes[c].value),
                Kind::Element | Kind::Fragment => self.collect_text(c, out),
                _ => {}
            }
        }
    }

    pub fn set_text(&mut self, id: usize, text: &str) {
        match self.nodes[id].kind {
            Kind::Element | Kind::Fragment => {
                for c in self.nodes[id].children.clone() {
                    self.nodes[c].parent = None;
                }
                self.nodes[id].children.clear();
                if !text.is_empty() {
                    let t = self.create(Kind::Text, "", text);
                    self.nodes[t].parent = Some(id);
                    self.nodes[id].children.push(t);
                }
            }
            Kind::Document => {}
            _ => self.nodes[id].value = text.to_string(),
        }
    }

    /// Fusionne les textes voisins et retire les textes vides.
    pub fn normalize(&mut self, id: usize) {
        let kids = self.nodes[id].children.clone();
        let mut out: Vec<usize> = Vec::new();
        for c in kids {
            if self.nodes[c].kind == Kind::Text {
                if self.nodes[c].value.is_empty() {
                    self.nodes[c].parent = None;
                    continue;
                }
                if let Some(&last) = out.last() {
                    if self.nodes[last].kind == Kind::Text {
                        let v = std::mem::take(&mut self.nodes[c].value);
                        self.nodes[last].value.push_str(&v);
                        self.nodes[c].parent = None;
                        continue;
                    }
                }
            } else if self.nodes[c].kind == Kind::Element {
                self.normalize(c);
            }
            out.push(c);
        }
        self.nodes[id].children = out;
    }

    // ── Noms ──────────────────────────────────────────────────────

    pub fn node_name(&self, id: usize) -> String {
        let n = &self.nodes[id];
        match n.kind {
            Kind::Element | Kind::Attribute | Kind::Pi => n.name.clone(),
            Kind::Text => "#text".into(),
            Kind::CData => "#cdata-section".into(),
            Kind::Comment => "#comment".into(),
            Kind::Document => "#document".into(),
            Kind::Fragment => "#document-fragment".into(),
        }
    }

    pub fn type_string(&self, id: usize) -> &'static str {
        match self.nodes[id].kind {
            Kind::Element => "element",
            Kind::Attribute => "attribute",
            Kind::Text => "text",
            Kind::CData => "cdatasection",
            Kind::Pi => "processinginstruction",
            Kind::Comment => "comment",
            Kind::Document => "document",
            Kind::Fragment => "documentfragment",
        }
    }

    pub fn prefix_and_base(&self, id: usize) -> (String, String) {
        let n = &self.nodes[id];
        if !matches!(n.kind, Kind::Element | Kind::Attribute) {
            return (String::new(), self.node_name(id));
        }
        match n.name.split_once(':') {
            Some((p, b)) => (p.to_string(), b.to_string()),
            None => (String::new(), n.name.clone()),
        }
    }

    /// Espace de noms du nœud : déclaration `xmlns[:préfixe]` la plus proche.
    pub fn namespace_uri(&self, id: usize) -> String {
        let n = &self.nodes[id];
        if !matches!(n.kind, Kind::Element | Kind::Attribute) {
            return String::new();
        }
        let (prefix, _) = self.prefix_and_base(id);
        if n.kind == Kind::Attribute && (prefix.is_empty() || n.name == "xmlns") {
            return if n.name == "xmlns" || prefix == "xmlns" {
                "http://www.w3.org/2000/xmlns/".into()
            } else {
                String::new()
            };
        }
        if prefix == "xml" {
            return "http://www.w3.org/XML/1998/namespace".into();
        }
        let wanted = if prefix.is_empty() {
            "xmlns".to_string()
        } else {
            format!("xmlns:{prefix}")
        };
        let mut cur = if n.kind == Kind::Attribute {
            n.parent
        } else {
            Some(id)
        };
        while let Some(c) = cur {
            if let Some(a) = self.attribute(c, &wanted) {
                return self.nodes[a].value.clone();
            }
            cur = self.nodes[c].parent;
        }
        String::new()
    }

    /// Éléments descendants de nom `name` (`*` : tous), ordre du document.
    pub fn elements_by_tag(&self, id: usize, name: &str, out: &mut Vec<usize>) {
        for &c in &self.nodes[id].children {
            if self.nodes[c].kind == Kind::Element {
                if name == "*" || self.nodes[c].name == name {
                    out.push(c);
                }
                self.elements_by_tag(c, name, out);
            }
        }
    }

    // ── Sérialisation ─────────────────────────────────────────────

    pub fn xml(&self, id: usize) -> String {
        let mut out = String::new();
        if self.nodes[id].kind == Kind::Document {
            for &c in &self.nodes[id].children {
                self.write(c, &mut out);
                out.push_str("\r\n");
            }
        } else {
            self.write(id, &mut out);
        }
        out
    }

    fn write(&self, id: usize, out: &mut String) {
        let n = &self.nodes[id];
        match n.kind {
            Kind::Element => {
                out.push('<');
                out.push_str(&n.name);
                for &a in &n.attrs {
                    out.push(' ');
                    out.push_str(&self.nodes[a].name);
                    out.push_str("=\"");
                    escape(&self.nodes[a].value, true, out);
                    out.push('"');
                }
                if n.children.is_empty() {
                    out.push_str("/>");
                } else {
                    out.push('>');
                    for &c in &n.children {
                        self.write(c, out);
                    }
                    out.push_str("</");
                    out.push_str(&n.name);
                    out.push('>');
                }
            }
            Kind::Attribute => {
                out.push_str(&n.name);
                out.push_str("=\"");
                escape(&n.value, true, out);
                out.push('"');
            }
            Kind::Text => escape(&n.value, false, out),
            Kind::CData => {
                out.push_str("<![CDATA[");
                out.push_str(&n.value);
                out.push_str("]]>");
            }
            Kind::Comment => {
                out.push_str("<!--");
                out.push_str(&n.value);
                out.push_str("-->");
            }
            Kind::Pi => {
                out.push_str("<?");
                out.push_str(&n.name);
                if !n.value.is_empty() {
                    out.push(' ');
                    out.push_str(&n.value);
                }
                out.push_str("?>");
            }
            Kind::Document | Kind::Fragment => {
                for &c in &n.children {
                    self.write(c, out);
                }
            }
        }
    }

    // ── Chargement ────────────────────────────────────────────────

    /// Remplace le contenu du document par `text` analysé. En cas
    /// d'erreur, le document est vide et `error` décrit la faute.
    pub fn load_str(&mut self, text: &str) -> bool {
        let keep = (
            self.preserve_ws,
            self.validate_on_parse,
            self.resolve_externals,
            self.asynchronous,
            std::mem::take(&mut self.properties),
            std::mem::take(&mut self.url),
        );
        *self = Doc::new();
        self.preserve_ws = keep.0;
        self.validate_on_parse = keep.1;
        self.resolve_externals = keep.2;
        self.asynchronous = keep.3;
        self.properties = keep.4;
        self.url = keep.5;
        let mut p = Parser {
            s: text,
            i: 0,
            doc: self,
        };
        match p.document() {
            Ok(()) => true,
            Err((code, reason, at)) => {
                let (line, linepos, src) = position(text, at);
                let url = self.url.clone();
                *self = Doc {
                    preserve_ws: keep.0,
                    validate_on_parse: keep.1,
                    resolve_externals: keep.2,
                    asynchronous: keep.3,
                    properties: std::mem::take(&mut self.properties),
                    ..Doc::new()
                };
                self.url = url.clone();
                self.error = ParseError {
                    code,
                    reason,
                    src_text: src,
                    line,
                    linepos,
                    filepos: text[..at.min(text.len())].chars().count() as i32,
                    url,
                };
                false
            }
        }
    }

    // ── XPath (sous-ensemble) ─────────────────────────────────────

    /// `selectNodes` : chemins `/a/b`, `a//b`, `//b`, `.`, `..`, `*`,
    /// `@nom`, `@*`, `text()`, `node()`, `comment()`, prédicats `[n]`,
    /// `[last()]`, `[@a]`, `[@a='v']`, `[nom]`, `[nom='v']`, `[text()='v']`,
    /// unions `|`. Rend None si l'expression n'est pas comprise.
    pub fn select(&self, context: usize, expr: &str) -> Option<Vec<usize>> {
        let mut all = Vec::new();
        for part in split_top(expr, '|') {
            for id in self.select_path(context, part.trim())? {
                if !all.contains(&id) {
                    all.push(id);
                }
            }
        }
        if split_top(expr, '|').len() > 1 {
            let order = self.document_order();
            all.sort_by_key(|id| order.iter().position(|x| x == id).unwrap_or(usize::MAX));
        }
        Some(all)
    }

    fn document_order(&self) -> Vec<usize> {
        let mut out = Vec::new();
        fn walk(d: &Doc, id: usize, out: &mut Vec<usize>) {
            out.push(id);
            out.extend(d.nodes[id].attrs.iter().copied());
            for &c in &d.nodes[id].children {
                walk(d, c, out);
            }
        }
        walk(self, DOC, &mut out);
        out
    }

    fn root_of(&self, id: usize) -> usize {
        let mut cur = id;
        while let Some(p) = self.nodes[cur].parent {
            cur = p;
        }
        cur
    }

    fn select_path(&self, context: usize, expr: &str) -> Option<Vec<usize>> {
        if expr.is_empty() {
            return None;
        }
        let (mut set, rest) = if let Some(r) = expr.strip_prefix("//") {
            (self.descendants_or_self(self.root_of(context)), r)
        } else if let Some(r) = expr.strip_prefix('/') {
            if r.is_empty() {
                return Some(vec![self.root_of(context)]);
            }
            (vec![self.root_of(context)], r)
        } else {
            (vec![context], expr)
        };
        let steps = split_steps(rest)?;
        for (descend, step) in steps {
            let mut next = Vec::new();
            for &n in &set {
                let bases = if descend {
                    self.descendants_or_self(n)
                } else {
                    vec![n]
                };
                for b in bases {
                    for m in self.step(b, step)? {
                        if !next.contains(&m) {
                            next.push(m);
                        }
                    }
                }
            }
            set = next;
        }
        Some(set)
    }

    fn descendants_or_self(&self, id: usize) -> Vec<usize> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            let n = out[i];
            // Ordre du document : les enfants juste après leur parent.
            let kids = self.nodes[n].children.clone();
            for (k, c) in kids.into_iter().enumerate() {
                out.insert(i + 1 + k, c);
            }
            i += 1;
        }
        // L'insertion ci-dessus garde l'ordre préfixe.
        out
    }

    /// Une étape : axe + test de nœud + prédicats.
    fn step(&self, base: usize, step: &str) -> Option<Vec<usize>> {
        let (test, preds) = split_predicates(step)?;
        let mut found: Vec<usize> = match test {
            "." => vec![base],
            ".." => self.nodes[base].parent.into_iter().collect(),
            _ => {
                if let Some(a) = test
                    .strip_prefix('@')
                    .or_else(|| test.strip_prefix("attribute::"))
                {
                    self.nodes[base]
                        .attrs
                        .iter()
                        .copied()
                        .filter(|&x| a == "*" || self.nodes[x].name == a)
                        .collect()
                } else {
                    let test = test.strip_prefix("child::").unwrap_or(test);
                    self.nodes[base]
                        .children
                        .iter()
                        .copied()
                        .filter(|&c| self.matches(c, test))
                        .collect()
                }
            }
        };
        for p in preds {
            found = self.filter(found, p)?;
        }
        Some(found)
    }

    fn matches(&self, id: usize, test: &str) -> bool {
        let n = &self.nodes[id];
        match test {
            "node()" => true,
            "text()" => matches!(n.kind, Kind::Text | Kind::CData),
            "comment()" => n.kind == Kind::Comment,
            "processing-instruction()" => n.kind == Kind::Pi,
            "*" => n.kind == Kind::Element,
            name => n.kind == Kind::Element && n.name == name,
        }
    }

    fn filter(&self, set: Vec<usize>, pred: &str) -> Option<Vec<usize>> {
        let pred = pred.trim();
        let len = set.len();
        if let Ok(k) = pred.parse::<usize>() {
            return Some(set.into_iter().nth(k.wrapping_sub(1)).into_iter().collect());
        }
        if pred == "last()" {
            return Some(set.into_iter().last().into_iter().collect());
        }
        let _ = len;
        let (lhs, rhs) = match pred.split_once("!=") {
            Some((l, r)) => (l.trim(), Some((r.trim(), false))),
            None => match pred.split_once('=') {
                Some((l, r)) => (l.trim(), Some((r.trim(), true))),
                None => (pred, None),
            },
        };
        let wanted = match rhs {
            Some((r, eq)) => {
                let lit = r
                    .strip_prefix('\'')
                    .and_then(|x| x.strip_suffix('\''))
                    .or_else(|| r.strip_prefix('"').and_then(|x| x.strip_suffix('"')))
                    .map(str::to_string)
                    .or_else(|| r.parse::<f64>().ok().map(|_| r.to_string()))?;
                Some((lit, eq))
            }
            None => None,
        };
        let mut out = Vec::new();
        for n in set {
            let values: Vec<String> = if lhs == "text()" || lhs == "." {
                if lhs == "." {
                    vec![self.text(n)]
                } else {
                    self.nodes[n]
                        .children
                        .iter()
                        .filter(|&&c| matches!(self.nodes[c].kind, Kind::Text | Kind::CData))
                        .map(|&c| self.nodes[c].value.clone())
                        .collect()
                }
            } else {
                self.select_path(n, lhs)?
                    .into_iter()
                    .map(|m| self.text(m))
                    .collect()
            };
            let keep = match &wanted {
                None => !values.is_empty(),
                Some((lit, true)) => values.iter().any(|v| v == lit),
                Some((lit, false)) => values.iter().any(|v| v != lit),
            };
            if keep {
                out.push(n);
            }
        }
        Some(out)
    }
}

/// Découpe hors crochets et guillemets.
fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, '[') => depth += 1,
            (None, ']') => depth -= 1,
            (None, c) if c == sep && depth == 0 => {
                parts.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Étapes d'un chemin relatif : (précédée de `//`, texte de l'étape).
fn split_steps(s: &str) -> Option<Vec<(bool, &str)>> {
    let mut out = Vec::new();
    let mut descend = false;
    for part in split_top(s, '/') {
        if part.is_empty() {
            if descend {
                return None;
            }
            descend = true;
            continue;
        }
        out.push((descend, part.trim()));
        descend = false;
    }
    if descend {
        return None;
    }
    Some(out)
}

fn split_predicates(step: &str) -> Option<(&str, Vec<&str>)> {
    let Some(open) = step.find('[') else {
        return Some((step.trim(), Vec::new()));
    };
    let test = step[..open].trim();
    let mut preds = Vec::new();
    let mut rest = &step[open..];
    while !rest.is_empty() {
        if !rest.starts_with('[') {
            return None;
        }
        let mut depth = 0;
        let mut quote: Option<char> = None;
        let mut end = None;
        for (i, c) in rest.char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '\'' | '"') => quote = Some(c),
                (None, '[') => depth += 1,
                (None, ']') => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let e = end?;
        preds.push(&rest[1..e]);
        rest = rest[e + 1..].trim_start();
    }
    Some((test, preds))
}

pub fn escape(s: &str, attribute: bool, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' if !attribute => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// Ligne, colonne (à partir de 1) et texte de la ligne de l'octet `at`.
fn position(text: &str, at: usize) -> (i32, i32, String) {
    let at = at.min(text.len());
    let before = &text[..at];
    let line = before.matches('\n').count() as i32 + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    let col = text[start..at].chars().count() as i32 + 1;
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    (
        line,
        col,
        text[start..end].trim_end_matches('\r').to_string(),
    )
}

/// Décode un fichier XML : BOM UTF-8/UTF-16, sinon UTF-8, sinon Latin-1.
pub fn decode(bytes: &[u8]) -> String {
    if let Some(b) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(b).into_owned();
    }
    if let Some(b) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let u: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&u);
    }
    if let Some(b) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let u: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&u);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

// ── Analyseur ──────────────────────────────────────────────────────

type Fault = (i32, String, usize);

struct Parser<'a> {
    s: &'a str,
    i: usize,
    doc: &'a mut Doc,
}

fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == ':'
}
fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_numeric() || c == '-' || c == '.' || c == '\u{B7}'
}

impl Parser<'_> {
    fn rest(&self) -> &str {
        &self.s[self.i..]
    }
    fn fail<T>(&self, code: i32, reason: &str) -> Result<T, Fault> {
        Err((code, reason.to_string(), self.i))
    }
    fn skip_ws(&mut self) {
        let r = self.rest();
        self.i += r.len() - r.trim_start_matches([' ', '\t', '\r', '\n']).len();
    }
    fn name(&mut self) -> Result<String, Fault> {
        let r = self.rest();
        let mut end = 0;
        for (k, c) in r.char_indices() {
            let ok = if k == 0 {
                is_name_start(c)
            } else {
                is_name_char(c)
            };
            if !ok {
                break;
            }
            end = k + c.len_utf8();
        }
        if end == 0 {
            return self.fail(
                XML_E_BADNAMECHAR,
                "Un nom contient un caractère non valide.",
            );
        }
        let name = r[..end].to_string();
        self.i += end;
        Ok(name)
    }
    fn until(&mut self, end: &str, what: &str) -> Result<String, Fault> {
        match self.rest().find(end) {
            Some(k) => {
                let v = self.rest()[..k].to_string();
                self.i += k + end.len();
                Ok(v)
            }
            None => {
                self.i = self.s.len();
                self.fail(
                    XML_E_UNEXPECTED_EOF,
                    &format!("Fin de fichier inattendue dans {what}."),
                )
            }
        }
    }
    fn unescape(&self, raw: &str, at: usize) -> Result<String, Fault> {
        if !raw.contains('&') {
            return Ok(raw.replace("\r\n", "\n"));
        }
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(k) = rest.find('&') {
            out.push_str(&rest[..k]);
            rest = &rest[k..];
            let Some(j) = rest.find(';') else {
                return Err((XML_E_BADENTITY, "Référence d'entité mal formée.".into(), at));
            };
            let ent = &rest[1..j];
            let c = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16)
                    .ok()
                    .and_then(char::from_u32),
                _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            let Some(c) = c else {
                return Err((
                    XML_E_BADENTITY,
                    format!("Référence à l'entité non définie « {ent} »."),
                    at,
                ));
            };
            out.push(c);
            rest = &rest[j + 1..];
        }
        out.push_str(rest);
        Ok(out.replace("\r\n", "\n"))
    }

    fn document(&mut self) -> Result<(), Fault> {
        if self.rest().starts_with('\u{FEFF}') {
            self.i += 3;
        }
        loop {
            self.skip_ws();
            if self.i >= self.s.len() {
                break;
            }
            if !self.rest().starts_with('<') {
                return self.fail(
                    XML_E_INVALIDATROOTLEVEL,
                    "Données non valides au niveau racine du document.",
                );
            }
            if self.rest().starts_with("<?") {
                self.pi(DOC)?;
            } else if self.rest().starts_with("<!--") {
                self.comment(DOC)?;
            } else if self.rest().starts_with("<!DOCTYPE") {
                self.doctype()?;
            } else {
                if self.doc.document_element().is_some() {
                    return self.fail(
                        XML_E_MULTIPLEROOTS,
                        "Un document XML ne peut contenir qu'un seul élément racine.",
                    );
                }
                self.element(DOC)?;
            }
        }
        if self.doc.document_element().is_none() {
            return self.fail(XML_E_MISSINGROOT, "L'élément racine est manquant.");
        }
        Ok(())
    }

    fn attach(&mut self, parent: usize, id: usize) {
        self.doc.nodes[id].parent = Some(parent);
        self.doc.nodes[parent].children.push(id);
    }

    fn pi(&mut self, parent: usize) -> Result<(), Fault> {
        self.i += 2;
        let target = self.name()?;
        self.skip_ws();
        let data = self.until("?>", "une instruction de traitement")?;
        let id = self.doc.create(Kind::Pi, &target, data.trim_end());
        self.attach(parent, id);
        Ok(())
    }

    fn comment(&mut self, parent: usize) -> Result<(), Fault> {
        self.i += 4;
        let text = self.until("-->", "un commentaire")?;
        let id = self.doc.create(Kind::Comment, "", &text);
        self.attach(parent, id);
        Ok(())
    }

    fn doctype(&mut self) -> Result<(), Fault> {
        // Ignorée (pas de DTD) : on saute jusqu'au '>' de fin.
        let mut depth = 0i32;
        let bytes = self.s.as_bytes();
        while self.i < bytes.len() {
            match bytes[self.i] {
                b'[' => depth += 1,
                b']' => depth -= 1,
                b'>' if depth <= 0 => {
                    self.i += 1;
                    return Ok(());
                }
                _ => {}
            }
            self.i += 1;
        }
        self.fail(
            XML_E_UNEXPECTED_EOF,
            "Fin de fichier inattendue dans la déclaration DOCTYPE.",
        )
    }

    fn element(&mut self, parent: usize) -> Result<(), Fault> {
        let start = self.i;
        self.i += 1;
        let name = self.name()?;
        let id = self.doc.create(Kind::Element, &name, "");
        self.attach(parent, id);
        loop {
            let before = self.i;
            self.skip_ws();
            if self.rest().starts_with("/>") {
                self.i += 2;
                return Ok(());
            }
            if self.rest().starts_with('>') {
                self.i += 1;
                break;
            }
            if self.i >= self.s.len() {
                self.i = start;
                return self.fail(
                    XML_E_UNEXPECTED_EOF,
                    "Fin de fichier inattendue dans une balise.",
                );
            }
            if before == self.i {
                return self.fail(
                    XML_E_BADNAMECHAR,
                    "Un espace est requis entre les attributs.",
                );
            }
            let an = self.name()?;
            self.skip_ws();
            if !self.rest().starts_with('=') {
                return self.fail(
                    XML_E_BADNAMECHAR,
                    "Signe « = » attendu après le nom d'attribut.",
                );
            }
            self.i += 1;
            self.skip_ws();
            let q = self.rest().chars().next();
            if q != Some('"') && q != Some('\'') {
                return self.fail(XML_E_BADNAMECHAR, "Valeur d'attribut sans guillemets.");
            }
            self.i += 1;
            let at = self.i;
            let raw = self.until(
                if q == Some('"') { "\"" } else { "'" },
                "une valeur d'attribut",
            )?;
            if raw.contains('<') {
                return Err((
                    XML_E_BADNAMECHAR,
                    "Caractère « < » dans une valeur d'attribut.".into(),
                    at,
                ));
            }
            let value = self.unescape(&raw, at)?.replace(['\n', '\t'], " ");
            if self.doc.attribute(id, &an).is_some() {
                return Err((
                    XML_E_BADNAMECHAR,
                    format!("Attribut « {an} » en double."),
                    at,
                ));
            }
            self.doc.set_attribute(id, &an, &value);
        }
        // Contenu.
        loop {
            if self.i >= self.s.len() {
                return Err((
                    XML_E_UNCLOSEDTAG,
                    format!("La balise « {name} » n'est pas fermée."),
                    start,
                ));
            }
            if self.rest().starts_with("</") {
                self.i += 2;
                let end = self.name()?;
                self.skip_ws();
                if end != name {
                    return self.fail(
                        XML_E_ENDTAGMISMATCH,
                        &format!("La balise de fin « {end} » ne correspond pas à la balise de début « {name} »."),
                    );
                }
                if !self.rest().starts_with('>') {
                    return self.fail(XML_E_UNEXPECTED_EOF, "« > » attendu.");
                }
                self.i += 1;
                return Ok(());
            }
            if self.rest().starts_with("<!--") {
                self.comment(id)?;
            } else if self.rest().starts_with("<![CDATA[") {
                self.i += 9;
                let text = self.until("]]>", "une section CDATA")?;
                let c = self.doc.create(Kind::CData, "", &text);
                self.attach(id, c);
            } else if self.rest().starts_with("<?") {
                self.pi(id)?;
            } else if self.rest().starts_with('<') {
                self.element(id)?;
            } else {
                let at = self.i;
                let end = self.rest().find('<').map_or(self.s.len(), |k| self.i + k);
                let raw = &self.s[self.i..end];
                self.i = end;
                let text = self.unescape(raw, at)?;
                if self.doc.preserve_ws || !text.trim_matches([' ', '\t', '\r', '\n']).is_empty() {
                    let t = self.doc.create(Kind::Text, "", &text);
                    self.attach(id, t);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(s: &str) -> Doc {
        let mut d = Doc::new();
        assert!(d.load_str(s), "{:?}", d.error);
        d
    }

    #[test]
    fn analyse_et_reecrit() {
        let d = load("<?xml version=\"1.0\"?>\n<!-- note -->\n<a x=\"1 &amp; 2\"><b>t&lt;</b><c/><![CDATA[<z>]]></a>");
        assert_eq!(
            d.xml(DOC),
            "<?xml version=\"1.0\"?>\r\n<!-- note -->\r\n<a x=\"1 &amp; 2\"><b>t&lt;</b><c/><![CDATA[<z>]]></a>\r\n"
        );
        let a = d.document_element().unwrap();
        assert_eq!(d.text(a), "t<<z>");
    }

    #[test]
    fn espaces_ignores_sauf_preserve() {
        let d = load("<a>\n  <b> x </b>\n</a>");
        let a = d.document_element().unwrap();
        assert_eq!(d.node(a).children.len(), 1);
        let mut p = Doc::new();
        p.preserve_ws = true;
        assert!(p.load_str("<a>\n  <b/>\n</a>"));
        assert_eq!(p.node(p.document_element().unwrap()).children.len(), 3);
    }

    #[test]
    fn erreurs_avec_position() {
        let mut d = Doc::new();
        assert!(!d.load_str("<a>\n  <b></c>\n</a>"));
        assert_eq!(d.error.code, XML_E_ENDTAGMISMATCH);
        assert_eq!(d.error.line, 2);
        assert_eq!(d.error.src_text, "  <b></c>");
        assert!(d.document_element().is_none());
        assert!(!d.load_str(""));
        assert_eq!(d.error.code, XML_E_MISSINGROOT);
        assert!(!d.load_str("<a/><b/>"));
        assert_eq!(d.error.code, XML_E_MULTIPLEROOTS);
        assert!(!d.load_str("<a>&nope;</a>"));
        assert_eq!(d.error.code, XML_E_BADENTITY);
        assert!(!d.load_str("<a>"));
        assert_eq!(d.error.code, XML_E_UNCLOSEDTAG);
    }

    #[test]
    fn xpath_sous_ensemble() {
        let d =
            load("<cfg><s name=\"a\"><v>1</v></s><s name=\"b\"><v>2</v><v>3</v></s><t>x</t></cfg>");
        let names = |ids: Vec<usize>| ids.iter().map(|&i| d.text(i)).collect::<Vec<_>>();
        assert_eq!(names(d.select(DOC, "/cfg/s/v").unwrap()), ["1", "2", "3"]);
        assert_eq!(names(d.select(DOC, "//v").unwrap()), ["1", "2", "3"]);
        assert_eq!(
            names(d.select(DOC, "cfg/s[@name='b']/v[2]").unwrap()),
            ["3"]
        );
        assert_eq!(names(d.select(DOC, "/cfg/s[v='1']/@name").unwrap()), ["a"]);
        assert_eq!(names(d.select(DOC, "/cfg/*[last()]").unwrap()), ["x"]);
        assert_eq!(
            names(d.select(DOC, "//s[@name]/v[1] | /cfg/t").unwrap()),
            ["1", "2", "x"]
        );
        let s = d.select(DOC, "//s[2]").unwrap()[0];
        assert_eq!(names(d.select(s, "../t/text()").unwrap()), ["x"]);
        assert!(
            d.select(DOC, "count(//v)").is_none()
                || d.select(DOC, "count(//v)").unwrap().is_empty()
        );
    }

    #[test]
    fn modifications_de_l_arbre() {
        let mut d = load("<a><b/></a>");
        let a = d.document_element().unwrap();
        let c = d.create(Kind::Element, "c", "");
        assert!(d.insert(a, c, None));
        let b = d.node(a).children[0];
        let t = d.create(Kind::Text, "", "é");
        assert!(d.insert(a, t, Some(b)));
        assert_eq!(d.xml(a), "<a>é<b/><c/></a>");
        // Un ancêtre ne peut devenir son propre enfant ; deux racines non plus.
        assert!(!d.insert(c, a, None));
        let r = d.create(Kind::Element, "r", "");
        assert!(!d.insert(DOC, r, None));
        let copy = d.clone_node(a, true);
        d.set_attribute(copy, "k", "<\"v\">");
        assert_eq!(d.xml(copy), "<a k=\"&lt;&quot;v&quot;>\">é<b/><c/></a>");
        d.set_text(a, "seul");
        assert_eq!(d.xml(a), "<a>seul</a>");
    }

    #[test]
    fn espaces_de_noms() {
        let d = load("<r xmlns=\"urn:x\" xmlns:p=\"urn:p\"><p:e p:a=\"1\"/></r>");
        let r = d.document_element().unwrap();
        let e = d.node(r).children[0];
        assert_eq!(d.namespace_uri(r), "urn:x");
        assert_eq!(d.namespace_uri(e), "urn:p");
        assert_eq!(d.prefix_and_base(e), ("p".to_string(), "e".to_string()));
    }
}

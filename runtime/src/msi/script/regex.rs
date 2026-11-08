//! Expressions régulières des scripts (`/…/g` de JScript, objet `RegExp`
//! de VBScript), d'après la syntaxe publique d'ECMAScript 3 : classes,
//! quantificateurs gourmands ou non, groupes capturants ou non,
//! alternatives, ancres, `\b`, références arrière. Recherche par retour
//! arrière, sur les caractères.

#[derive(Debug, Clone)]
enum Re {
    Char(char),
    Any,
    /// Intervalles, négation.
    Class(Vec<(char, char)>, bool),
    Start,
    End,
    WordB(bool),
    Group(Box<Re>, Option<usize>),
    /// Assertion `(?=…)` ou `(?!…)`.
    Look(Box<Re>, bool),
    Cat(Vec<Re>),
    Alt(Vec<Re>),
    Rep(Box<Re>, u32, u32, bool),
    BackRef(usize),
}

#[derive(Debug, Clone)]
pub struct Regex {
    re: Re,
    groups: usize,
    pub ignore_case: bool,
    pub multiline: bool,
}

pub type Caps = Vec<Option<(usize, usize)>>;

struct Parser<'a> {
    s: &'a [char],
    i: usize,
    groups: usize,
}

fn class_escape(c: char) -> Option<(Vec<(char, char)>, bool)> {
    let digits = vec![('0', '9')];
    let word = vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')];
    let space = vec![
        (' ', ' '),
        ('\t', '\r'),
        ('\u{a0}', '\u{a0}'),
        ('\u{2028}', '\u{2029}'),
        ('\u{feff}', '\u{feff}'),
    ];
    Some(match c {
        'd' => (digits, false),
        'D' => (digits, true),
        'w' => (word, false),
        'W' => (word, true),
        's' => (space, false),
        'S' => (space, true),
        _ => return None,
    })
}

fn plain_escape(c: char) -> char {
    match c {
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        'f' => '\u{c}',
        'v' => '\u{b}',
        '0' => '\0',
        o => o,
    }
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn alt(&mut self) -> Result<Re, String> {
        let mut branches = vec![self.cat()?];
        while self.peek() == Some('|') {
            self.i += 1;
            branches.push(self.cat()?);
        }
        Ok(if branches.len() == 1 {
            branches.pop().unwrap()
        } else {
            Re::Alt(branches)
        })
    }

    fn cat(&mut self) -> Result<Re, String> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            let atom = self.atom()?;
            items.push(self.quant(atom)?);
        }
        Ok(Re::Cat(items))
    }

    fn number(&mut self) -> Option<u32> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        self.s[start..self.i]
            .iter()
            .collect::<String>()
            .parse()
            .ok()
    }

    fn quant(&mut self, atom: Re) -> Result<Re, String> {
        let (min, max) = match self.peek() {
            Some('*') => (0, u32::MAX),
            Some('+') => (1, u32::MAX),
            Some('?') => (0, 1),
            Some('{') => {
                let save = self.i;
                self.i += 1;
                let Some(min) = self.number() else {
                    self.i = save;
                    return Ok(atom);
                };
                let max = if self.peek() == Some(',') {
                    self.i += 1;
                    self.number().unwrap_or(u32::MAX)
                } else {
                    min
                };
                if self.peek() != Some('}') {
                    self.i = save;
                    return Ok(atom);
                }
                (min, max) // le `}` est consommé plus bas
            }
            _ => return Ok(atom),
        };
        self.i += 1;
        let greedy = if self.peek() == Some('?') {
            self.i += 1;
            false
        } else {
            true
        };
        Ok(Re::Rep(Box::new(atom), min, max, greedy))
    }

    fn atom(&mut self) -> Result<Re, String> {
        let c = self.peek().ok_or("motif incomplet")?;
        self.i += 1;
        Ok(match c {
            '.' => Re::Any,
            '^' => Re::Start,
            '$' => Re::End,
            '(' => {
                let mut cap = true;
                let mut look = None;
                if self.peek() == Some('?') {
                    match self.s.get(self.i + 1) {
                        Some(':') => cap = false,
                        Some('=') => look = Some(true),
                        Some('!') => look = Some(false),
                        _ => return Err("groupe non reconnu".into()),
                    }
                    self.i += 2;
                }
                let idx = if cap && look.is_none() {
                    self.groups += 1;
                    Some(self.groups)
                } else {
                    None
                };
                let inner = self.alt()?;
                if self.peek() != Some(')') {
                    return Err("parenthèse non fermée".into());
                }
                self.i += 1;
                match look {
                    Some(pos) => Re::Look(Box::new(inner), pos),
                    None => Re::Group(Box::new(inner), idx),
                }
            }
            '[' => self.class()?,
            '\\' => {
                let e = self.peek().ok_or("\\ final")?;
                self.i += 1;
                if let Some((r, neg)) = class_escape(e) {
                    Re::Class(r, neg)
                } else if e == 'b' {
                    Re::WordB(true)
                } else if e == 'B' {
                    Re::WordB(false)
                } else if e.is_ascii_digit() && e != '0' {
                    self.i -= 1;
                    Re::BackRef(self.number().unwrap_or(0) as usize)
                } else if e == 'x' || e == 'u' {
                    Re::Char(self.hex(if e == 'x' { 2 } else { 4 }))
                } else {
                    Re::Char(plain_escape(e))
                }
            }
            o => Re::Char(o),
        })
    }

    fn hex(&mut self, n: usize) -> char {
        let digits: String = self.s[self.i..(self.i + n).min(self.s.len())]
            .iter()
            .collect();
        match u32::from_str_radix(&digits, 16)
            .ok()
            .and_then(char::from_u32)
        {
            Some(c) if digits.len() == n => {
                self.i += n;
                c
            }
            _ => 'x',
        }
    }

    fn class(&mut self) -> Result<Re, String> {
        let mut neg = false;
        if self.peek() == Some('^') {
            neg = true;
            self.i += 1;
        }
        let mut ranges = Vec::new();
        let mut first = true;
        loop {
            let c = self.peek().ok_or("crochet non fermé")?;
            self.i += 1;
            if c == ']' && !first {
                break;
            }
            first = false;
            let lo = if c == '\\' {
                let e = self.peek().ok_or("\\ final")?;
                self.i += 1;
                if let Some((r, n)) = class_escape(e) {
                    if n {
                        // \D, \W, \S dans une classe : complément approché.
                        let mut prev = '\0';
                        for (a, b) in {
                            let mut r = r.clone();
                            r.sort();
                            r
                        } {
                            if a > prev {
                                ranges.push((prev, char::from_u32(a as u32 - 1).unwrap_or(prev)));
                            }
                            prev = char::from_u32(b as u32 + 1).unwrap_or(b);
                        }
                        ranges.push((prev, char::MAX));
                    } else {
                        ranges.extend(r);
                    }
                    continue;
                }
                if e == 'b' {
                    '\u{8}'
                } else if e == 'x' || e == 'u' {
                    self.hex(if e == 'x' { 2 } else { 4 })
                } else {
                    plain_escape(e)
                }
            } else {
                c
            };
            if self.peek() == Some('-') && self.s.get(self.i + 1).is_some_and(|&n| n != ']') {
                self.i += 1;
                let mut hi = self.peek().unwrap();
                self.i += 1;
                if hi == '\\' {
                    hi = plain_escape(self.peek().unwrap_or('\\'));
                    self.i += 1;
                }
                ranges.push((lo, hi));
            } else {
                ranges.push((lo, lo));
            }
        }
        Ok(Re::Class(ranges, neg))
    }
}

fn is_word(c: Option<&char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || *c == '_')
}

impl Regex {
    pub fn new(pattern: &str, flags: &str) -> Result<Regex, String> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut p = Parser {
            s: &chars,
            i: 0,
            groups: 0,
        };
        let re = p.alt()?;
        if p.i != chars.len() {
            return Err(format!("expression régulière incorrecte : {pattern}"));
        }
        Ok(Regex {
            re,
            groups: p.groups,
            ignore_case: flags.contains('i'),
            multiline: flags.contains('m'),
        })
    }

    fn same(&self, a: char, b: char) -> bool {
        a == b || self.ignore_case && a.to_lowercase().eq(b.to_lowercase())
    }

    fn in_class(&self, c: char, ranges: &[(char, char)]) -> bool {
        let test = |c: char| ranges.iter().any(|&(a, b)| a <= c && c <= b);
        test(c) || self.ignore_case && (c.to_lowercase().any(test) || c.to_uppercase().any(test))
    }

    fn m(
        &self,
        re: &Re,
        s: &[char],
        i: usize,
        caps: &mut Caps,
        k: &mut dyn FnMut(usize, &mut Caps) -> bool,
    ) -> bool {
        match re {
            Re::Char(c) => s.get(i).is_some_and(|&x| self.same(x, *c)) && k(i + 1, caps),
            Re::Any => s.get(i).is_some_and(|&x| x != '\n' && x != '\r') && k(i + 1, caps),
            Re::Class(r, neg) => {
                s.get(i).is_some_and(|&x| self.in_class(x, r) != *neg) && k(i + 1, caps)
            }
            Re::Start => {
                (i == 0 || self.multiline && matches!(s[i - 1], '\n' | '\r')) && k(i, caps)
            }
            Re::End => {
                (i == s.len() || self.multiline && matches!(s[i], '\n' | '\r')) && k(i, caps)
            }
            Re::WordB(want) => {
                let at = is_word(i.checked_sub(1).and_then(|p| s.get(p))) != is_word(s.get(i));
                at == *want && k(i, caps)
            }
            Re::Group(inner, idx) => {
                let start = i;
                self.m(inner, s, i, caps, &mut |j, caps: &mut Caps| {
                    let Some(n) = idx else { return k(j, caps) };
                    let old = caps[*n];
                    caps[*n] = Some((start, j));
                    if k(j, caps) {
                        return true;
                    }
                    caps[*n] = old;
                    false
                })
            }
            Re::Look(inner, positive) => {
                let mut probe = caps.clone();
                let found = self.m(inner, s, i, &mut probe, &mut |_, _| true);
                if found == *positive {
                    if found {
                        *caps = probe;
                    }
                    k(i, caps)
                } else {
                    false
                }
            }
            Re::Cat(items) => self.seq(items, s, i, caps, k),
            Re::Alt(branches) => branches.iter().any(|b| self.m(b, s, i, caps, k)),
            Re::Rep(inner, min, max, greedy) => {
                self.rep(inner, *min, *max, *greedy, 0, s, i, caps, k)
            }
            Re::BackRef(n) => {
                let Some(Some((a, b))) = caps.get(*n) else {
                    return k(i, caps);
                };
                let len = b - a;
                if i + len > s.len() {
                    return false;
                }
                (0..len).all(|d| self.same(s[a + d], s[i + d])) && k(i + len, caps)
            }
        }
    }

    fn seq(
        &self,
        items: &[Re],
        s: &[char],
        i: usize,
        caps: &mut Caps,
        k: &mut dyn FnMut(usize, &mut Caps) -> bool,
    ) -> bool {
        match items.split_first() {
            None => k(i, caps),
            Some((first, rest)) => self.m(first, s, i, caps, &mut |j, caps: &mut Caps| {
                self.seq(rest, s, j, caps, k)
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rep(
        &self,
        r: &Re,
        min: u32,
        max: u32,
        greedy: bool,
        count: u32,
        s: &[char],
        i: usize,
        caps: &mut Caps,
        k: &mut dyn FnMut(usize, &mut Caps) -> bool,
    ) -> bool {
        if count < min {
            return self.m(r, s, i, caps, &mut |j, caps: &mut Caps| {
                self.rep(r, min, max, greedy, count + 1, s, j, caps, k)
            });
        }
        if greedy {
            if count < max
                && self.m(r, s, i, caps, &mut |j, caps: &mut Caps| {
                    j != i && self.rep(r, min, max, greedy, count + 1, s, j, caps, k)
                })
            {
                return true;
            }
            k(i, caps)
        } else {
            k(i, caps)
                || count < max
                    && self.m(r, s, i, caps, &mut |j, caps: &mut Caps| {
                        j != i && self.rep(r, min, max, greedy, count + 1, s, j, caps, k)
                    })
        }
    }

    /// Première correspondance à partir de `from` (indices en caractères) :
    /// groupe 0 = toute la correspondance.
    pub fn find_at(&self, s: &[char], from: usize) -> Option<Caps> {
        for start in from..=s.len() {
            let mut caps: Caps = vec![None; self.groups + 1];
            let mut end = None;
            if self.m(&self.re, s, start, &mut caps, &mut |j, _| {
                end = Some(j);
                true
            }) {
                caps[0] = Some((start, end.unwrap_or(start)));
                return Some(caps);
            }
        }
        None
    }

    pub fn is_match(&self, text: &str) -> bool {
        let s: Vec<char> = text.chars().collect();
        self.find_at(&s, 0).is_some()
    }

    /// Toutes les correspondances (ou la première).
    pub fn find_all(&self, s: &[char], global: bool) -> Vec<Caps> {
        let mut out = Vec::new();
        let mut at = 0;
        while at <= s.len() {
            let Some(caps) = self.find_at(s, at) else {
                break;
            };
            let (a, b) = caps[0].unwrap();
            at = if b == a { b + 1 } else { b };
            out.push(caps);
            if !global {
                break;
            }
        }
        out
    }

    /// Remplacement avec `$1`…`$99`, `$&`, `` $` ``, `$'` et `$$`.
    pub fn replace(&self, text: &str, with: &str, global: bool) -> String {
        let s: Vec<char> = text.chars().collect();
        let w: Vec<char> = with.chars().collect();
        let mut out = String::new();
        let mut last = 0;
        for caps in self.find_all(&s, global) {
            let (a, b) = caps[0].unwrap();
            out.extend(&s[last..a]);
            let mut i = 0;
            while i < w.len() {
                if w[i] == '$' && i + 1 < w.len() {
                    let n = w[i + 1];
                    match n {
                        '$' => {
                            out.push('$');
                            i += 2;
                            continue;
                        }
                        '&' => {
                            out.extend(&s[a..b]);
                            i += 2;
                            continue;
                        }
                        '`' => {
                            out.extend(&s[..a]);
                            i += 2;
                            continue;
                        }
                        '\'' => {
                            out.extend(&s[b..]);
                            i += 2;
                            continue;
                        }
                        d if d.is_ascii_digit() => {
                            let mut num = d.to_digit(10).unwrap() as usize;
                            let mut used = 2;
                            if let Some(d2) = w.get(i + 2).and_then(|c| c.to_digit(10)) {
                                if num * 10 + (d2 as usize) <= self.groups {
                                    num = num * 10 + d2 as usize;
                                    used = 3;
                                }
                            }
                            if num >= 1 && num <= self.groups {
                                if let Some((x, y)) = caps[num] {
                                    out.extend(&s[x..y]);
                                }
                                i += used;
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
                out.push(w[i]);
                i += 1;
            }
            last = b;
        }
        out.extend(&s[last..]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::Regex;

    #[test]
    fn regex_basics() {
        let r = Regex::new(r"^(\w+)-(\d{2,3})$", "").unwrap();
        assert!(r.is_match("abc-12"));
        assert!(!r.is_match("abc-1"));
        assert_eq!(r.replace("abc-123", "$2:$1", false), "123:abc");
        let r = Regex::new(r"\\", "g").unwrap();
        assert_eq!(r.replace(r"C:\a\b", "/", true), "C:/a/b");
        let r = Regex::new("a.*?b", "i").unwrap();
        assert_eq!(r.replace("xAzzbzzb", "-", false), "x-zzb");
        let r = Regex::new("[^0-9.]", "").unwrap();
        assert_eq!(
            r.replace("v1.2b", "", true),
            "v1.2b".replacen('v', "", 1).replacen('b', "", 1)
        );
        let r = Regex::new("(a|bc)+(?=d)", "").unwrap();
        assert!(r.is_match("abcad"));
        let r = Regex::new(r"(\w)\1", "").unwrap();
        assert!(r.is_match("hello"));
    }
}

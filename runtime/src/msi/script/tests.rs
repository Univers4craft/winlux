use super::*;

fn js(src: &str) -> String {
    let (r, it) = run_source(Lang::JScript, src, "", vec![]);
    match r {
        Ok(v) => it.to_str(&v),
        Err(e) => format!("ERREUR {e}"),
    }
}

fn vb(src: &str, function: &str) -> String {
    let (r, it) = run_source(Lang::VBScript, src, function, vec![]);
    match r {
        Ok(v) => it.to_str(&v),
        Err(e) => format!("ERREUR {e}"),
    }
}

/// Valeur d'une variable globale après le script VBScript.
fn vb_var(src: &str, name: &str) -> String {
    let (r, it) = run_source(Lang::VBScript, src, "", vec![]);
    if let Err(e) = r {
        return format!("ERREUR {e}");
    }
    it.global_value(name)
        .map(|v| it.to_str(&v))
        .unwrap_or_default()
}

#[test]
fn jscript_language() {
    assert_eq!(
        js("function f(a, b) { return a + b; }\nreturn f(2, 3) + '!';"),
        "5!"
    );
    assert_eq!(
        js("var s = 0; for (var i = 0; i < 10; i++) { if (i % 2) continue; s += i; } return s"),
        "20"
    );
    assert_eq!(
        js("var o = {a: 1, 'b': [1, 2, 3]}; return o.b.length + o.a"),
        "4"
    );
    assert_eq!(
        js("var a = []; a.push('x'); a.push('y'); return a.join('-')"),
        "x-y"
    );
    assert_eq!(js("return '1' + 2 + 3"), "123");
    assert_eq!(js("return 1 + 2 + '3'"), "33");
    assert_eq!(js("return '10' == 10 && '10' !== 10"), "true");
    assert_eq!(
        js("return typeof x + typeof 1 + typeof 'a'"),
        "undefinednumberstring"
    );
    assert_eq!(js("var t; try { throw new Error('boum'); } catch (e) { t = e.message; } finally { t += '!'; } return t"), "boum!");
    assert_eq!(
        js("function mk(n) { return function () { return n * 2; }; } return mk(21)()"),
        "42"
    );
    assert_eq!(js("var r = ''; switch (2) { case 1: r += 'a'; case 2: r += 'b'; case 3: r += 'c'; break; default: r += 'd'; } return r"), "bc");
    assert_eq!(
        js("var k = []; for (var p in {x: 1, y: 2}) k.push(p); return k.join()"),
        "x,y"
    );
    assert_eq!(js("var i = 0; do { i++; } while (i < 5); return i"), "5");
    assert_eq!(
        js("return 'C:\\\\Prog\\\\App'.replace(/\\\\/g, '/').toUpperCase()"),
        "C:/PROG/APP"
    );
    assert_eq!(
        js("var m = 'v1.25'.match(/(\\d+)\\.(\\d+)/); return m[2]"),
        "25"
    );
    assert_eq!(
        js("return 'a,b,,c'.split(',').length + parseInt('0x1F') + parseFloat('2.5kg')"),
        "37.5"
    );
    assert_eq!(
        js("return x"),
        "ERREUR erreur 5009 : « x » n'est pas défini"
    );
    assert_eq!(js("var n = 7; n <<= 2; return n.toString(2)"), "11100");
    assert_eq!(
        js("return [3, 1, 2].sort().concat([9]).slice(1).join('')"),
        "239"
    );
    assert_eq!(js("return Math.max(1, 5, 3) + Math.floor(2.7)"), "7");
    assert_eq!(
        js("var s = 'abcdef'; return s.substring(4, 1) + s.substr(-2) + s.charAt(0)"),
        "bcdefa"
    );
    assert_eq!(
        js("var c = 0; while (true) { if (++c > 3) break; } return c"),
        "4"
    );
}

#[test]
fn vbscript_language() {
    let src = "Function Add(a, b)\n  Add = a + b\nEnd Function\nFunction Main()\n  Main = Add(2, 3) & \"!\"\nEnd Function";
    assert_eq!(vb(src, "Main"), "5!");
    assert_eq!(
        vb_var(
            "Dim s, i\ns = 0\nFor i = 1 To 10 Step 3 : s = s + i : Next",
            "s"
        ),
        "22"
    );
    assert_eq!(
        vb_var(
            "Dim a(2)\na(0) = \"x\" : a(2) = \"z\"\nr = Join(a, \"-\") & UBound(a)",
            "r"
        ),
        "x--z2"
    );
    assert_eq!(vb_var("If 1 = 2 Then r = \"a\" Else r = \"b\"", "r"), "b");
    assert_eq!(
        vb_var("x = 5\nIf x > 10 Then\n r = \"grand\"\nElseIf x > 3 Then\n r = \"moyen\"\nElse\n r = \"petit\"\nEnd If", "r"),
        "moyen"
    );
    assert_eq!(
        vb_var("Select Case 7\n Case 1, 2\n  r = \"a\"\n Case 5 To 8\n  r = \"b\"\n Case Else\n  r = \"c\"\nEnd Select", "r"),
        "b"
    );
    assert_eq!(vb_var("i = 0\nDo While i < 4\n i = i + 1\nLoop", "i"), "4");
    assert_eq!(vb_var("i = 0\nDo\n i = i + 2\nLoop Until i >= 7", "i"), "8");
    assert_eq!(vb_var("i = 0\nWhile i < 3 : i = i + 1 : Wend", "i"), "3");
    assert_eq!(vb_var("r = Mid(\"Bonjour\", 2, 3) & Left(\"abc\", 1) & Right(\"abc\", 2) & InStr(\"abcabc\", \"ca\")", "r"), "onjabc3");
    assert_eq!(
        vb_var(
            "r = UCase(Replace(\"a.b.c\", \".\", \"/\")) & Len(\"été\")",
            "r"
        ),
        "A/B/C3"
    );
    assert_eq!(
        vb_var("p = Split(\"1;2;3\", \";\")\nr = p(1) + p(2)", "r"),
        "23"
    );
    assert_eq!(vb_var("r = CInt(\"12\") + 1", "r"), "13");
    assert_eq!(
        vb_var("r = 7 \\ 2 & \"/\" & 7 Mod 3 & \"/\" & 2 ^ 3", "r"),
        "3/1/8"
    );
    assert_eq!(vb_var("r = (1 = 1) And Not (2 = 3)", "r"), "True");
    assert_eq!(
        vb_var(
            "On Error Resume Next\nx = 1 / 0\nr = Err.Number\nErr.Clear\nq = Err.Number",
            "r"
        ),
        "11"
    );
    assert_eq!(
        vb_var("x = 1 / 0\nr = 1", "r"),
        "ERREUR erreur 11 : division par zéro"
    );
    assert_eq!(vb_var("Set d = CreateObject(\"Scripting.Dictionary\")\nd.Add \"a\", 1\nd(\"b\") = 2\nFor Each k In d.Keys : r = r & k & d(k) : Next", "r"), "a1b2");
    assert_eq!(
        vb_var(
            "Sub P(x)\n  r = r & x\nEnd Sub\nr = \"\"\nP \"un\"\nCall P(\"deux\")",
            "r"
        ),
        "undeux"
    );
    assert_eq!(vb_var("Set re = New RegExp\nre.Pattern = \"(\\d+)\"\nre.Global = True\nr = re.Replace(\"a1b22\", \"<$1>\")", "r"), "a<1>b<22>");
    assert_eq!(
        vb_var(
            "r = TypeName(Nothing) & TypeName(1) & TypeName(\"\") & IsEmpty(z)",
            "r"
        ),
        "NothingIntegerStringTrue"
    );
    assert_eq!(vb_var("r = &HFF + &H10", "r"), "271");
    assert_eq!(
        vb_var("Const A = 3 ' commentaire\nr = A * _\n  2", "r"),
        "6"
    );
    assert_eq!(vb_var("Set d = CreateObject(\"Scripting.Dictionary\")\nWith d\n .Add \"k\", \"v\"\n r = .Item(\"k\") & .Count\nEnd With", "r"), "v1");
    assert_eq!(
        vb_var("Dim m(1, 2)\nm(1, 2) = 5\nr = m(1, 2) + UBound(m, 2)", "r"),
        "7"
    );
    assert_eq!(
        vb_var(
            "ReDim a(1)\na(1) = \"x\"\nReDim Preserve a(3)\nr = a(1) & UBound(a)",
            "r"
        ),
        "x3"
    );
}

#[test]
fn status_codes_follow_installer_rules() {
    let it = Interp::new(Lang::VBScript);
    assert_eq!(status_code(&it, &V::Empty), 0);
    assert_eq!(status_code(&it, &V::Num(1.0)), 0);
    assert_eq!(status_code(&it, &V::Num(3.0)), 1603);
    assert_eq!(status_code(&it, &V::Num(2.0)), 1602);
}

#[test]
fn syntax_errors_name_the_line() {
    let (r, _) = run_source(Lang::VBScript, "x = 1\nIf x Then\n y = 2\n", "", vec![]);
    assert!(r.unwrap_err().contains("End if"));
    let (r, _) = run_source(Lang::JScript, "var a = 1;\nvar b = (2;", "", vec![]);
    assert!(r.unwrap_err().contains("ligne 2"));
}

#[test]
fn endless_loop_is_stopped() {
    let (r, _) = run_source(Lang::JScript, "while (true) {}", "", vec![]);
    assert!(r.unwrap_err().contains("trop d'instructions"));
}

#[test]
fn decodes_utf16_scripts() {
    let mut data = vec![0xFF, 0xFE];
    data.extend("x = \"é\"".encode_utf16().flat_map(u16::to_le_bytes));
    assert_eq!(decode_source(&data), "x = \"é\"");
}

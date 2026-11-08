//! Témoins du CLR intégré : assemblies écrites par `emit`, exécutées de bout
//! en bout par l'interpréteur.

use super::emit::*;
use std::path::PathBuf;
use std::sync::Mutex;

/// Le registre des types et le tas sont globaux : un témoin à la fois.
static SERIE: Mutex<()> = Mutex::new(());

const ADD: u8 = 0x58;
const MUL: u8 = 0x5A;
const DUP: u8 = 0x25;

fn main_sig() -> Vec<u8> {
    method_sig(false, &[T_I4], &[])
}

fn dossier(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("winlux-clr-{nom}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn lancer(pe: &[u8], dir: Option<&std::path::Path>) -> i32 {
    let _verrou = SERIE.lock().unwrap_or_else(|e| e.into_inner());
    super::execute_assembly_in(pe, dir)
}

#[test]
fn un_programme_minimal_rend_son_code_de_sortie() {
    let mut b = Builder::new("Temoin");
    b.type_def(PUBLIC, "Temoin", "Programme", 0);
    let main = b.method(
        M_PUBLIC | M_STATIC,
        "Main",
        &main_sig(),
        Some(Il::default().i4(42).ret().body()),
    );
    assert_eq!(lancer(&b.build(main), None), 42);
}

/// Bibliothèque « Outils » livrée à côté du programme :
/// `Outils.Base` (champ `Valeur`, méthode virtuelle `Nom` → 1),
/// `Outils.Calcul` (`Double(x)`, statique `Compteur` posé à 7 par `.cctor`).
fn bibliotheque_outils() -> Vec<u8> {
    let mut b = Builder::new("Outils");
    let mscorlib = b.assembly_ref("mscorlib");
    let object = b.type_ref(Scope::Assembly(mscorlib), "System", "Object");
    let object_ctor = b.member_ref(object, ".ctor", &method_sig(true, &[T_VOID], &[]));

    let base = b.type_def(PUBLIC, "Outils", "Base", object);
    b.field(F_PUBLIC, "Valeur", &[T_I4]);
    b.method(
        M_CTOR,
        ".ctor",
        &method_sig(true, &[T_VOID], &[]),
        Some(Il::default().ldarg0().call(object_ctor).ret().body()),
    );
    b.method(
        M_PUBLIC | M_VIRTUAL | M_HIDEBYSIG | M_NEWSLOT,
        "Nom",
        &method_sig(true, &[T_I4], &[]),
        Some(Il::default().i4(1).ret().body()),
    );

    b.type_def(PUBLIC | ABSTRACT, "Outils", "Calcul", object);
    let compteur = b.field(F_PUBLIC | F_STATIC, "Compteur", &[T_I4]);
    b.method(
        M_PUBLIC | M_STATIC | M_HIDEBYSIG,
        "Double",
        &method_sig(false, &[T_I4], &[&[T_I4]]),
        Some(Il::default().op(0x02).i4(2).op(MUL).ret().body()),
    );
    b.method(
        M_CCTOR,
        ".cctor",
        &method_sig(false, &[T_VOID], &[]),
        Some(Il::default().i4(7).stsfld(compteur).ret().body()),
    );
    let _ = base;
    b.build(0)
}

/// Programme qui utilise « Outils » : appel statique, classe dérivée qui
/// redéfinit une méthode virtuelle, champ statique initialisé par le
/// constructeur statique, champ d'instance d'une classe de la bibliothèque.
fn programme_outils() -> Vec<u8> {
    let mut b = Builder::new("Programme");
    let outils = b.assembly_ref("Outils");
    let base = b.type_ref(Scope::Assembly(outils), "Outils", "Base");
    let calcul = b.type_ref(Scope::Assembly(outils), "Outils", "Calcul");
    let void_this = method_sig(true, &[T_VOID], &[]);
    let base_ctor = b.member_ref(base, ".ctor", &void_this);
    let base_nom = b.member_ref(base, "Nom", &method_sig(true, &[T_I4], &[]));
    let valeur = b.member_ref(base, "Valeur", &[0x06, T_I4]);
    let double = b.member_ref(calcul, "Double", &method_sig(false, &[T_I4], &[&[T_I4]]));
    let compteur = b.member_ref(calcul, "Compteur", &[0x06, T_I4]);

    b.type_def(PUBLIC, "Programme", "Derive", base);
    let derive_ctor = b.method(
        M_CTOR,
        ".ctor",
        &void_this,
        Some(Il::default().ldarg0().call(base_ctor).ret().body()),
    );
    b.method(
        M_PUBLIC | M_VIRTUAL | M_HIDEBYSIG,
        "Nom",
        &method_sig(true, &[T_I4], &[]),
        Some(Il::default().i4(5).ret().body()),
    );

    b.type_def(PUBLIC, "Programme", "Point", 0);
    let code = Il::default()
        // Double(21) = 42
        .i4(21)
        .call(double)
        // + new Derive().Nom() * 100 = 500
        .newobj(derive_ctor)
        .callvirt(base_nom)
        .i4(100)
        .op(MUL)
        .op(ADD)
        // + Calcul.Compteur * 1000 = 7000
        .ldsfld(compteur)
        .i4(1000)
        .op(MUL)
        .op(ADD)
        // + (new Base { Valeur = 3 }).Valeur * 10000 = 30000
        .newobj(base_ctor)
        .op(DUP)
        .i4(3)
        .stfld(valeur)
        .ldfld(valeur)
        .i4(10000)
        .op(MUL)
        .op(ADD)
        .ret();
    let main = b.method(M_PUBLIC | M_STATIC, "Main", &main_sig(), Some(code.body()));
    b.build(main)
}

#[test]
fn une_bibliotheque_livree_avec_le_programme_est_chargee() {
    let dir = dossier("outils");
    std::fs::write(dir.join("Outils.dll"), bibliotheque_outils()).unwrap();
    let code = lancer(&programme_outils(), Some(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 42 + 500 + 7000 + 30000);
}

#[test]
fn le_nom_de_la_dll_est_cherche_sans_tenir_compte_de_la_casse() {
    let dir = dossier("casse");
    std::fs::write(dir.join("OUTILS.DLL"), bibliotheque_outils()).unwrap();
    let code = lancer(&programme_outils(), Some(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 37542);
}

#[test]
fn sans_la_bibliotheque_l_appel_echoue_proprement() {
    let dir = dossier("absente");
    let code = lancer(&programme_outils(), Some(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    // Exception non gérée (MissingMethodException), pas de plantage.
    assert_eq!(code, 0xE0434352u32 as i32);
}

#[test]
fn le_constructeur_statique_ne_tourne_qu_une_fois() {
    let mut b = Builder::new("Statique");
    b.type_def(PUBLIC, "Statique", "Programme", 0);
    let n = b.field(F_PUBLIC | F_STATIC, "N", &[T_I4]);
    let incr = b.method(
        M_PUBLIC | M_STATIC,
        "Incr",
        &method_sig(false, &[T_VOID], &[]),
        Some(Il::default().ldsfld(n).i4(1).op(ADD).stsfld(n).ret().body()),
    );
    b.method(
        M_CCTOR,
        ".cctor",
        &method_sig(false, &[T_VOID], &[]),
        Some(Il::default().i4(10).stsfld(n).ret().body()),
    );
    let main = b.method(
        M_PUBLIC | M_STATIC,
        "Main",
        &main_sig(),
        Some(Il::default().call(incr).call(incr).ldsfld(n).ret().body()),
    );
    assert_eq!(lancer(&b.build(main), None), 12);
}

// ── Étape 2 : nouveau cœur d'exécution (#945, #946) ──

const STLOC0: u8 = 0x0A;
const STLOC1: u8 = 0x0B;
const LDLOC0: u8 = 0x06;
const LDLOC1: u8 = 0x07;
const LDARG1: u8 = 0x03;

impl Il {
    fn ldloca(mut self, i: u8) -> Il {
        self.0.extend_from_slice(&[0x12, i]);
        self
    }
    fn ldarga(mut self, i: u8) -> Il {
        self.0.extend_from_slice(&[0x0F, i]);
        self
    }
    fn prefix(mut self, op: u8, token: u32) -> Il {
        self.0.extend_from_slice(&[0xFE, op]);
        self.0.extend_from_slice(&token.to_le_bytes());
        self
    }
}

fn avec_locaux(il: Il, locaux: Vec<Vec<u8>>) -> Body {
    let mut body = il.body();
    body.locals = locaux;
    body
}

/// `ref`/`out` et largeurs des accès indirects : `Fixe(out int x)` puis un
/// octet lu par `ldelema` + `ldind.u1`.
#[test]
fn ref_out_et_acces_indirects() {
    let mut b = Builder::new("RefOut");
    let mscorlib = b.assembly_ref("mscorlib");
    let byte = b.type_ref(Scope::Assembly(mscorlib), "System", "Byte");
    b.type_def(PUBLIC, "RefOut", "P", 0);
    let fixe = b.method(
        M_PUBLIC | M_STATIC,
        "Fixe",
        &method_sig(false, &[T_VOID], &[&[0x10, T_I4]]),
        Some(Il::default().ldarg0().i4(7).op(0x54).ret().body()),
    );
    let code = Il::default()
        .ldloca(0)
        .call(fixe)
        .i4(3)
        .tok(0x8D, byte)
        .op(STLOC1)
        .op(LDLOC1)
        .i4(1)
        .i4(200)
        .op(0x9C) // stelem.i1
        .op(LDLOC1)
        .i4(1)
        .tok(0x8F, byte) // ldelema
        .op(0x47) // ldind.u1
        .op(LDLOC0)
        .op(ADD)
        .ret();
    // byte[] : SZARRAY U1.
    let byte_array = vec![0x1D, 0x05];
    let main = b.method(
        M_PUBLIC | M_STATIC,
        "Main",
        &main_sig(),
        Some(avec_locaux(code, vec![vec![T_I4], byte_array])),
    );
    assert_eq!(lancer(&b.build(main), None), 207);
}

/// Un type valeur se copie : `b = a` puis modifier `b` ne touche pas `a`,
/// une méthode qui modifie `this` agit sur la variable, le boxing copie.
#[test]
fn les_structures_se_copient() {
    let mut b = Builder::new("Structs");
    let mscorlib = b.assembly_ref("mscorlib");
    let value_type_ref = b.type_ref(Scope::Assembly(mscorlib), "System", "ValueType");
    let point = b.type_def(PUBLIC, "Structs", "Point", value_type_ref);
    let x = b.field(F_PUBLIC, "X", &[T_I4]);
    b.field(F_PUBLIC, "Y", &[T_I4]);
    let decale = b.method(
        M_PUBLIC | M_HIDEBYSIG,
        "Decale",
        &method_sig(true, &[T_VOID], &[&[T_I4]]),
        Some(
            Il::default()
                .ldarg0()
                .ldarg0()
                .ldfld(x)
                .op(LDARG1)
                .op(ADD)
                .stfld(x)
                .ret()
                .body(),
        ),
    );
    b.type_def(PUBLIC, "Structs", "P", 0);
    let code = Il::default()
        .ldloca(0)
        .i4(1)
        .stfld(x)
        .op(LDLOC0)
        .op(STLOC1) // b = a
        .ldloca(1)
        .i4(5)
        .stfld(x) // b.X = 5
        .ldloca(1)
        .i4(100)
        .call(decale) // b.X = 105
        .op(LDLOC0)
        .ldfld(x) // a.X = 1
        .i4(1000)
        .op(MUL)
        .ldloca(1)
        .ldfld(x)
        .op(ADD) // 1105
        .op(LDLOC0)
        .tok(0x8C, point) // box
        .tok(0xA5, point) // unbox.any
        .op(STLOC1)
        .ldloca(1)
        .ldfld(x)
        .op(ADD) // 1106
        .ret();
    let main = b.method(
        M_PUBLIC | M_STATIC,
        "Main",
        &main_sig(),
        Some(avec_locaux(
            code,
            vec![value_type(point), value_type(point)],
        )),
    );
    assert_eq!(lancer(&b.build(main), None), 1106);
}

/// Génériques : `Boite<T>` (champ T, statique par instanciation, box/unbox
/// de T, `constrained.`), méthodes génériques et `ldelem !!0`.
#[test]
fn les_generiques_sont_instancies() {
    let mut b = Builder::new("Generiques");
    let mscorlib = b.assembly_ref("mscorlib");
    let object = b.type_ref(Scope::Assembly(mscorlib), "System", "Object");
    let string = b.type_ref(Scope::Assembly(mscorlib), "System", "String");
    let int32 = b.type_ref(Scope::Assembly(mscorlib), "System", "Int32");
    let object_ctor = b.member_ref(object, ".ctor", &method_sig(true, &[T_VOID], &[]));
    let to_string = b.member_ref(object, "ToString", &method_sig(true, &[T_STRING], &[]));
    let length = b.member_ref(string, "get_Length", &method_sig(true, &[T_I4], &[]));
    let t = b.type_spec(&var(0));
    let mt = b.type_spec(&mvar(0));

    let boite = b.type_def(PUBLIC, "Generiques", "Boite`1", object);
    b.generic_param(boite, 0, "T");
    b.field(F_PUBLIC, "Valeur", &var(0));
    b.field(F_PUBLIC | F_STATIC, "Compte", &[T_I4]);
    b.method(
        M_CTOR,
        ".ctor",
        &method_sig(true, &[T_VOID], &[]),
        Some(Il::default().ldarg0().call(object_ctor).ret().body()),
    );
    b.method(
        M_PUBLIC | M_HIDEBYSIG,
        "Emballer",
        &method_sig(true, &[T_OBJECT], &[&var(0)]),
        Some(Il::default().op(LDARG1).tok(0x8C, t).ret().body()),
    );
    b.method(
        M_PUBLIC | M_HIDEBYSIG,
        "Texte",
        &method_sig(true, &[T_STRING], &[&var(0)]),
        Some(
            Il::default()
                .ldarga(1)
                .prefix(0x16, t)
                .callvirt(to_string)
                .ret()
                .body(),
        ),
    );

    b.type_def(PUBLIC, "Generiques", "P", object);
    let identite = b.method(
        M_PUBLIC | M_STATIC,
        "Identite",
        &generic_method_sig(1, false, &mvar(0), &[&mvar(0)]),
        Some(Il::default().ldarg0().ret().body()),
    );
    b.generic_param(identite, 0, "T");
    let mut tableau_t = vec![0x1D];
    tableau_t.extend_from_slice(&mvar(0));
    let lire = b.method(
        M_PUBLIC | M_STATIC,
        "Lire",
        &generic_method_sig(1, false, &mvar(0), &[&tableau_t, &[T_I4]]),
        Some(Il::default().ldarg0().op(LDARG1).tok(0xA3, mt).ret().body()),
    );
    b.generic_param(lire, 0, "T");

    let boite_int_sig = generic_inst(false, boite, &[&[T_I4]]);
    let boite_int = b.type_spec(&boite_int_sig);
    let boite_str = b.type_spec(&generic_inst(false, boite, &[&[T_STRING]]));
    let ctor = b.member_ref(boite_int, ".ctor", &method_sig(true, &[T_VOID], &[]));
    let emballer = b.member_ref(
        boite_int,
        "Emballer",
        &method_sig(true, &[T_OBJECT], &[&var(0)]),
    );
    let texte = b.member_ref(
        boite_int,
        "Texte",
        &method_sig(true, &[T_STRING], &[&var(0)]),
    );
    let valeur = b.member_ref(boite_int, "Valeur", &[&[0x06u8][..], &var(0)].concat());
    let compte_int = b.member_ref(boite_int, "Compte", &[0x06, T_I4]);
    let compte_str = b.member_ref(boite_str, "Compte", &[0x06, T_I4]);
    let identite_int = b.method_spec(identite, &[&[T_I4]]);
    let lire_int = b.method_spec(lire, &[&[T_I4]]);

    let code = Il::default()
        .newobj(ctor)
        .op(STLOC0)
        .op(LDLOC0)
        .i4(41)
        .call(emballer)
        .tok(0xA5, int32) // 41
        .op(LDLOC0)
        .i4(2)
        .stfld(valeur)
        .op(LDLOC0)
        .ldfld(valeur)
        .op(ADD) // 43
        .i4(1)
        .stsfld(compte_int)
        .i4(50)
        .stsfld(compte_str)
        .ldsfld(compte_int)
        .op(ADD) // 44
        .op(LDLOC0)
        .i4(12345)
        .callvirt(texte)
        .callvirt(length)
        .i4(100)
        .op(MUL)
        .op(ADD) // 544
        .i4(9)
        .call(identite_int)
        .i4(1000)
        .op(MUL)
        .op(ADD) // 9544
        .i4(3)
        .tok(0x8D, int32)
        .op(DUP)
        .i4(2)
        .i4(7)
        .op(0x9E) // stelem.i4
        .i4(2)
        .call(lire_int)
        .i4(10000)
        .op(MUL)
        .op(ADD) // 79544
        .ret();
    let main = b.method(
        M_PUBLIC | M_STATIC,
        "Main",
        &main_sig(),
        Some(avec_locaux(code, vec![boite_int_sig.clone()])),
    );
    assert_eq!(lancer(&b.build(main), None), 79544);
}

/// Arithmétique : comparaisons de flottants, NaN, décalages 64 bits,
/// conversions non signées et débordement contrôlé.
#[test]
fn arithmetique_conforme() {
    let mut b = Builder::new("Arith");
    b.type_def(PUBLIC, "Arith", "P", 0);
    let mut il = Il::default();
    // 2.5 > 2.25 (comparaison flottante, pas entière) → 1
    il.0.push(0x23);
    il.0.extend_from_slice(&2.5f64.to_le_bytes());
    il.0.push(0x23);
    il.0.extend_from_slice(&2.25f64.to_le_bytes());
    il.0.extend_from_slice(&[0xFE, 0x02]); // cgt
                                           // NaN == NaN → 0 ; (NaN < 1).un → 1
    il.0.push(0x23);
    il.0.extend_from_slice(&f64::NAN.to_le_bytes());
    il.0.push(0x23);
    il.0.extend_from_slice(&1.0f64.to_le_bytes());
    il.0.extend_from_slice(&[0xFE, 0x05]); // clt.un → 1
    il.0.push(0x58); // 2
                     // (1L << 40) >> 38 = 4
    il.0.push(0x21);
    il.0.extend_from_slice(&1i64.to_le_bytes());
    il = il.i4(40).op(0x62); // shl
    il = il.i4(38).op(0x63); // shr
    il = il.op(0x69); // conv.i4
    il = il.i4(10).op(MUL).op(ADD); // 42
                                    // conv.u8 de -1 (int32) = 0xFFFFFFFF, >> 31 = 1
    il = il
        .i4(-1)
        .op(0x6E)
        .i4(31)
        .op(0x64)
        .op(0x69)
        .i4(100)
        .op(MUL)
        .op(ADD); // 142
    il = il.ret();
    let main = b.method(M_PUBLIC | M_STATIC, "Main", &main_sig(), Some(il.body()));
    assert_eq!(lancer(&b.build(main), None), 142);
}

#[test]
fn conv_ovf_leve_overflow_exception() {
    let mut b = Builder::new("Ovf");
    b.type_def(PUBLIC, "Ovf", "P", 0);
    let il = Il::default().i4(300).op(0x86).ret(); // conv.ovf.u1.un
    let main = b.method(M_PUBLIC | M_STATIC, "Main", &main_sig(), Some(il.body()));
    assert_eq!(lancer(&b.build(main), None), 0xE0434352u32 as i32);
}

#[test]
fn ldloca_puis_ldind_relit_la_variable() {
    let mut b = Builder::new("Sonde");
    b.type_def(PUBLIC, "Sonde", "P", 0);
    let mut body = Il::default()
        .i4(5)
        .op(0x0A) // stloc.0
        .op(0x12)
        .op(0x00) // ldloca.s 0
        .op(0x4A) // ldind.i4
        .ret()
        .body();
    body.locals = vec![vec![T_I4]];
    let main = b.method(M_PUBLIC | M_STATIC, "Main", &main_sig(), Some(body));
    assert_eq!(lancer(&b.build(main), None), 5);
}

// ── Étape 3 : délégués, événements, lambdas (#971) ──

const LDNULL: u8 = 0x14;
const POP: u8 = 0x26;
const CASTCLASS: u8 = 0x74;
const LDSFLDA: u8 = 0x7F;
const T_NATIVE_INT: u8 = 0x18;

impl Il {
    fn ldftn(self, t: u32) -> Il {
        self.prefix(0x06, t)
    }
    fn ldvirtftn(self, t: u32) -> Il {
        self.prefix(0x07, t)
    }
}

fn delegue_ctor_sig() -> Vec<u8> {
    method_sig(true, &[T_VOID], &[&[T_OBJECT], &[T_NATIVE_INT]])
}

/// Délégué simple sur une méthode statique, délégué fermé sur une
/// instance, `Delegate.Combine` puis `Delegate.Remove`.
#[test]
fn les_delegues_appellent_leur_methode() {
    let mut b = Builder::new("Delegues");
    let mscorlib = b.assembly_ref("mscorlib");
    let object = b.type_ref(Scope::Assembly(mscorlib), "System", "Object");
    let multicast = b.type_ref(Scope::Assembly(mscorlib), "System", "MulticastDelegate");
    let delegate = b.type_ref(Scope::Assembly(mscorlib), "System", "Delegate");
    let object_ctor = b.member_ref(object, ".ctor", &method_sig(true, &[T_VOID], &[]));
    let deux_delegues = method_sig(
        false,
        &class(delegate),
        &[&class(delegate), &class(delegate)],
    );
    let combine = b.member_ref(delegate, "Combine", &deux_delegues);
    let remove = b.member_ref(delegate, "Remove", &deux_delegues);

    let op = b.type_def(PUBLIC, "Delegues", "Op", multicast);
    let op_ctor = b.method(M_CTOR, ".ctor", &delegue_ctor_sig(), None);
    let op_invoke = b.method(
        M_PUBLIC | M_VIRTUAL | M_HIDEBYSIG | M_NEWSLOT,
        "Invoke",
        &method_sig(true, &[T_I4], &[&[T_I4]]),
        None,
    );

    let compteur = b.type_def(PUBLIC, "Delegues", "Compteur", object);
    let total = b.field(F_PUBLIC, "Total", &[T_I4]);
    let compteur_ctor = b.method(
        M_CTOR,
        ".ctor",
        &method_sig(true, &[T_VOID], &[]),
        Some(Il::default().ldarg0().call(object_ctor).ret().body()),
    );
    // Total += x ; return Total
    let ajouter = b.method(
        M_PUBLIC | M_HIDEBYSIG,
        "Ajouter",
        &method_sig(true, &[T_I4], &[&[T_I4]]),
        Some(
            Il::default()
                .ldarg0()
                .ldarg0()
                .ldfld(total)
                .op(LDARG1)
                .op(ADD)
                .stfld(total)
                .ldarg0()
                .ldfld(total)
                .ret()
                .body(),
        ),
    );

    b.type_def(PUBLIC, "Delegues", "P", object);
    let carre = b.method(
        M_PUBLIC | M_STATIC,
        "Carre",
        &method_sig(false, &[T_I4], &[&[T_I4]]),
        Some(Il::default().ldarg0().ldarg0().op(MUL).ret().body()),
    );
    // locaux : 0 = Compteur, 1 = Op d1 (Carre), 2 = Op d2 (Ajouter), 3 = Op d
    let code = Il::default()
        .newobj(compteur_ctor)
        .op(STLOC0)
        .op(LDNULL)
        .ldftn(carre)
        .newobj(op_ctor)
        .op(STLOC1)
        .op(LDLOC0)
        .ldftn(ajouter)
        .newobj(op_ctor)
        .op(0x0C) // stloc.2
        // d = Combine(d1, d2) ; d(5) ; résultat = d(10) → Total = 15
        .op(LDLOC1)
        .op(0x08) // ldloc.2
        .call(combine)
        .tok(CASTCLASS, op)
        .op(0x0D) // stloc.3
        .op(0x09) // ldloc.3
        .i4(5)
        .callvirt(op_invoke)
        .op(POP)
        .op(0x09)
        .i4(10)
        .callvirt(op_invoke)
        .i4(100)
        .op(MUL)
        // + d1(7) = 49
        .op(LDLOC1)
        .i4(7)
        .callvirt(op_invoke)
        .op(ADD)
        // + Remove(d, d2)(3) * 10000 = 90000
        .op(0x09)
        .op(0x08)
        .call(remove)
        .tok(CASTCLASS, op)
        .i4(3)
        .callvirt(op_invoke)
        .i4(10000)
        .op(MUL)
        .op(ADD)
        // + Total * 1000000 : Remove n'a plus appelé Ajouter
        .op(LDLOC0)
        .ldfld(total)
        .i4(1_000_000)
        .op(MUL)
        .op(ADD)
        .ret();
    let main = b.method(
        M_PUBLIC | M_STATIC,
        "Main",
        &main_sig(),
        Some(avec_locaux(
            code,
            vec![class(compteur), class(op), class(op), class(op)],
        )),
    );
    assert_eq!(lancer(&b.build(main), None), 15_091_549);
}

/// `ldvirtftn` choisit la redéfinition, délégué générique de la
/// bibliothèque (`Func<int>`), et accesseur d'événement tel que l'écrit un
/// compilateur : `Delegate.Combine` puis `Interlocked.CompareExchange<T>`.
#[test]
fn ldvirtftn_func_et_evenements() {
    let mut b = Builder::new("Evenements");
    let mscorlib = b.assembly_ref("mscorlib");
    let object = b.type_ref(Scope::Assembly(mscorlib), "System", "Object");
    let multicast = b.type_ref(Scope::Assembly(mscorlib), "System", "MulticastDelegate");
    let delegate = b.type_ref(Scope::Assembly(mscorlib), "System", "Delegate");
    let func = b.type_ref(Scope::Assembly(mscorlib), "System", "Func`1");
    let interlocked = b.type_ref(Scope::Assembly(mscorlib), "System.Threading", "Interlocked");
    let object_ctor = b.member_ref(object, ".ctor", &method_sig(true, &[T_VOID], &[]));
    let combine = b.member_ref(
        delegate,
        "Combine",
        &method_sig(
            false,
            &class(delegate),
            &[&class(delegate), &class(delegate)],
        ),
    );
    let func_int = b.type_spec(&generic_inst(false, func, &[&[T_I4]]));
    let func_ctor = b.member_ref(func_int, ".ctor", &delegue_ctor_sig());
    let func_invoke = b.member_ref(func_int, "Invoke", &method_sig(true, &var(0), &[]));
    let mut ref_t = vec![0x10];
    ref_t.extend_from_slice(&mvar(0));
    let cmpxchg = b.member_ref(
        interlocked,
        "CompareExchange",
        &generic_method_sig(1, false, &mvar(0), &[&ref_t, &mvar(0), &mvar(0)]),
    );

    let rappel = b.type_def(PUBLIC, "Evenements", "Rappel", multicast);
    let rappel_ctor = b.method(M_CTOR, ".ctor", &delegue_ctor_sig(), None);
    let rappel_invoke = b.method(
        M_PUBLIC | M_VIRTUAL | M_HIDEBYSIG | M_NEWSLOT,
        "Invoke",
        &method_sig(true, &[T_I4], &[]),
        None,
    );
    let cmpxchg_rappel = b.method_spec(cmpxchg, &[&class(rappel)]);

    let base = b.type_def(PUBLIC, "Evenements", "Base", object);
    let base_ctor = b.method(
        M_CTOR,
        ".ctor",
        &method_sig(true, &[T_VOID], &[]),
        Some(Il::default().ldarg0().call(object_ctor).ret().body()),
    );
    let base_valeur = b.method(
        M_PUBLIC | M_VIRTUAL | M_HIDEBYSIG | M_NEWSLOT,
        "Valeur",
        &method_sig(true, &[T_I4], &[]),
        Some(Il::default().i4(1).ret().body()),
    );
    b.type_def(PUBLIC, "Evenements", "Derive", base);
    let derive_ctor = b.method(
        M_CTOR,
        ".ctor",
        &method_sig(true, &[T_VOID], &[]),
        Some(Il::default().ldarg0().call(base_ctor).ret().body()),
    );
    b.method(
        M_PUBLIC | M_VIRTUAL | M_HIDEBYSIG,
        "Valeur",
        &method_sig(true, &[T_I4], &[]),
        Some(Il::default().i4(2).ret().body()),
    );

    b.type_def(PUBLIC, "Evenements", "P", object);
    let evt = b.field(F_PUBLIC | F_STATIC, "Clic", &class(rappel));
    let appels = b.field(F_PUBLIC | F_STATIC, "Appels", &[T_I4]);
    let un = b.method(
        M_PUBLIC | M_STATIC,
        "Un",
        &method_sig(false, &[T_I4], &[]),
        Some(
            Il::default()
                .ldsfld(appels)
                .i4(1)
                .op(ADD)
                .stsfld(appels)
                .i4(1)
                .ret()
                .body(),
        ),
    );
    // add_Clic(Rappel r) : CompareExchange(ref Clic, Combine(Clic, r), Clic)
    let ajouter = b.method(
        M_PUBLIC | M_STATIC,
        "add_Clic",
        &method_sig(false, &[T_VOID], &[&class(rappel)]),
        Some(
            Il::default()
                .tok(LDSFLDA, evt)
                .ldsfld(evt)
                .ldarg0()
                .call(combine)
                .tok(CASTCLASS, rappel)
                .ldsfld(evt)
                .call(cmpxchg_rappel)
                .op(POP)
                .ret()
                .body(),
        ),
    );
    let code = Il::default()
        // new Func<int>(derive, ldvirtftn Base::Valeur)() = 2
        .newobj(derive_ctor)
        .op(DUP)
        .ldvirtftn(base_valeur)
        .newobj(func_ctor)
        .callvirt(func_invoke)
        // Clic += Un ; Clic += Un ; Clic() * 10 = 10
        .op(LDNULL)
        .ldftn(un)
        .newobj(rappel_ctor)
        .call(ajouter)
        .op(LDNULL)
        .ldftn(un)
        .newobj(rappel_ctor)
        .call(ajouter)
        .ldsfld(evt)
        .callvirt(rappel_invoke)
        .i4(10)
        .op(MUL)
        .op(ADD)
        // + Appels * 100 = 200
        .ldsfld(appels)
        .i4(100)
        .op(MUL)
        .op(ADD)
        .ret();
    let main = b.method(M_PUBLIC | M_STATIC, "Main", &main_sig(), Some(code.body()));
    assert_eq!(lancer(&b.build(main), None), 212);
}

// ── Étape 4 : exceptions complètes (#972) ──

const THROW: u8 = 0x7A;
const ENDFINALLY: u8 = 0xDC;
const CL_CATCH: u32 = 0;
const CL_FILTER: u32 = 1;
const CL_FINALLY: u32 = 2;

/// Code IL avec étiquettes, pour placer les `leave` et les clauses.
#[derive(Default)]
struct Prog {
    code: Vec<u8>,
    labels: std::collections::HashMap<&'static str, u32>,
    fixups: Vec<(usize, &'static str)>,
}

impl Il {
    fn op2(mut self, b: u8) -> Il {
        self.0.extend_from_slice(&[0xFE, b]);
        self
    }
}

impl Prog {
    fn il(&mut self, il: Il) -> &mut Prog {
        self.code.extend_from_slice(&il.0);
        self
    }
    fn label(&mut self, name: &'static str) -> &mut Prog {
        self.labels.insert(name, self.code.len() as u32);
        self
    }
    fn leave(&mut self, name: &'static str) -> &mut Prog {
        self.code.push(0xDD);
        self.fixups.push((self.code.len(), name));
        self.code.extend_from_slice(&[0; 4]);
        self
    }
    fn at(&self, name: &str) -> u32 {
        self.labels[name]
    }
    /// Clause de `try` à `try_end`, gestionnaire de `h` à `h_end`.
    fn clause(
        &self,
        flags: u32,
        t: &str,
        t_end: &str,
        h: &str,
        h_end: &str,
        extra: u32,
    ) -> [u32; 6] {
        let (t, h) = (self.at(t), self.at(h));
        [flags, t, self.at(t_end) - t, h, self.at(h_end) - h, extra]
    }
    fn finish(mut self) -> Vec<u8> {
        for (pos, name) in std::mem::take(&mut self.fixups) {
            let rel = self.labels[name] as i32 - (pos as i32 + 4);
            self.code[pos..pos + 4].copy_from_slice(&rel.to_le_bytes());
        }
        self.code
    }
}

/// Filtres `when`, finally imbriqués traversés par un `leave` et par une
/// exception relancée, `rethrow`, `InnerException`. Chaque étape ajoute un
/// chiffre à `Trace` : l'ordre attendu est 1 2 3 4 5 6 7 8, puis la longueur
/// du message interne (4).
#[test]
fn filtres_finally_et_rethrow() {
    let mut b = Builder::new("Exceptions");
    let mscorlib = b.assembly_ref("mscorlib");
    let object = b.type_ref(Scope::Assembly(mscorlib), "System", "Object");
    let exc = b.type_ref(Scope::Assembly(mscorlib), "System", "Exception");
    let string = b.type_ref(Scope::Assembly(mscorlib), "System", "String");
    let exc_ctor = b.member_ref(exc, ".ctor", &method_sig(true, &[T_VOID], &[&[T_STRING]]));
    let exc_ctor2 = b.member_ref(
        exc,
        ".ctor",
        &method_sig(true, &[T_VOID], &[&[T_STRING], &class(exc)]),
    );
    let message = b.member_ref(exc, "get_Message", &method_sig(true, &[T_STRING], &[]));
    let inner = b.member_ref(
        exc,
        "get_InnerException",
        &method_sig(true, &class(exc), &[]),
    );
    let length = b.member_ref(string, "get_Length", &method_sig(true, &[T_I4], &[]));
    let abc = b.user_string("abc");
    let ab = b.user_string("ab");
    let abcd = b.user_string("abcd");

    b.type_def(PUBLIC, "Exceptions", "P", object);
    let trace = b.field(F_PUBLIC | F_STATIC, "Trace", &[T_I4]);
    let mark = |d: i32| {
        Il::default()
            .ldsfld(trace)
            .i4(10)
            .op(MUL)
            .i4(d)
            .op(ADD)
            .stsfld(trace)
    };
    let jeter = |s: u32| Il::default().ldstr(s).newobj(exc_ctor).op(THROW);
    // Filtre : longueur du message == n (ceq ; endfilter)
    let filtre = |n: i32| {
        Il::default()
            .tok(CASTCLASS, exc)
            .callvirt(message)
            .callvirt(length)
            .i4(n)
            .op2(0x01)
            .op2(0x11)
    };
    let mut p = Prog::default();

    // A : le filtre interne refuse (longueur 2), l'externe accepte (3).
    p.label("a_to")
        .label("a_ti")
        .il(jeter(abc))
        .label("a_ti_end");
    p.label("a_fi").il(filtre(2));
    p.label("a_hi")
        .il(Il::default().op(POP))
        .il(mark(9))
        .leave("a_fin")
        .label("a_hi_end");
    p.label("a_fo").il(filtre(3));
    p.label("a_ho")
        .il(Il::default().op(POP))
        .il(mark(1))
        .leave("a_fin")
        .label("a_ho_end");
    p.label("a_fin");

    // B : catch qui relance, deux finally traversés par l'exception, catch externe.
    p.label("b_c").label("b_f2").label("b_f1").label("b_r");
    p.il(jeter(abc)).label("b_r_end");
    p.label("b_rh")
        .il(Il::default().op(POP))
        .il(mark(2))
        .il(Il::default().op2(0x1A));
    p.label("b_rh_end").label("b_f1_end");
    p.label("b_f1h")
        .il(mark(3))
        .il(Il::default().op(ENDFINALLY))
        .label("b_f1h_end");
    p.label("b_f2_end");
    p.label("b_f2h")
        .il(mark(4))
        .il(Il::default().op(ENDFINALLY))
        .label("b_f2h_end");
    p.label("b_c_end");
    p.label("b_ch")
        .il(Il::default().op(POP))
        .il(mark(5))
        .leave("b_fin")
        .label("b_ch_end");
    p.label("b_fin");

    // C : un `leave` traverse deux finally.
    p.label("c_g2")
        .label("c_g1")
        .il(mark(6))
        .leave("c_fin")
        .label("c_g1_end");
    p.label("c_g1h")
        .il(mark(7))
        .il(Il::default().op(ENDFINALLY))
        .label("c_g1h_end");
    p.label("c_g2_end");
    p.label("c_g2h")
        .il(mark(8))
        .il(Il::default().op(ENDFINALLY))
        .label("c_g2h_end");
    p.label("c_fin");

    // D : InnerException.
    p.il(Il::default()
        .ldsfld(trace)
        .i4(10)
        .op(MUL)
        .ldstr(ab)
        .ldstr(abcd)
        .newobj(exc_ctor)
        .newobj(exc_ctor2)
        .callvirt(inner)
        .callvirt(message)
        .callvirt(length)
        .op(ADD)
        .ret());

    let clauses = vec![
        p.clause(
            CL_FILTER,
            "a_ti",
            "a_ti_end",
            "a_hi",
            "a_hi_end",
            p.at("a_fi"),
        ),
        p.clause(
            CL_FILTER,
            "a_to",
            "a_hi_end",
            "a_ho",
            "a_ho_end",
            p.at("a_fo"),
        ),
        p.clause(CL_CATCH, "b_r", "b_r_end", "b_rh", "b_rh_end", exc),
        p.clause(CL_FINALLY, "b_f1", "b_f1_end", "b_f1h", "b_f1h_end", 0),
        p.clause(CL_FINALLY, "b_f2", "b_f2_end", "b_f2h", "b_f2h_end", 0),
        p.clause(CL_CATCH, "b_c", "b_c_end", "b_ch", "b_ch_end", exc),
        p.clause(CL_FINALLY, "c_g1", "c_g1_end", "c_g1h", "c_g1h_end", 0),
        p.clause(CL_FINALLY, "c_g2", "c_g2_end", "c_g2h", "c_g2h_end", 0),
    ];
    let mut body = Body::new(p.finish());
    body.clauses = clauses;
    let main = b.method(M_PUBLIC | M_STATIC, "Main", &main_sig(), Some(body));
    assert_eq!(lancer(&b.build(main), None), 123_456_784);
}

/// Une exception qui quitte une méthode exécute son finally, puis un filtre
/// qui lève lui-même une exception vaut un refus.
#[test]
fn le_finally_tourne_avant_l_appelant_et_un_filtre_qui_leve_refuse() {
    let mut b = Builder::new("Remontee");
    let mscorlib = b.assembly_ref("mscorlib");
    let object = b.type_ref(Scope::Assembly(mscorlib), "System", "Object");
    let exc = b.type_ref(Scope::Assembly(mscorlib), "System", "Exception");
    let exc_ctor = b.member_ref(exc, ".ctor", &method_sig(true, &[T_VOID], &[&[T_STRING]]));
    let abc = b.user_string("abc");
    b.type_def(PUBLIC, "Remontee", "P", object);
    let trace = b.field(F_PUBLIC | F_STATIC, "Trace", &[T_I4]);
    let mark = |d: i32| {
        Il::default()
            .ldsfld(trace)
            .i4(10)
            .op(MUL)
            .i4(d)
            .op(ADD)
            .stsfld(trace)
    };

    let mut f = Prog::default();
    f.label("t")
        .il(Il::default().ldstr(abc).newobj(exc_ctor).op(THROW))
        .label("t_end");
    f.label("h")
        .il(mark(1))
        .il(Il::default().op(ENDFINALLY))
        .label("h_end");
    f.il(Il::default().ret());
    let clause = f.clause(CL_FINALLY, "t", "t_end", "h", "h_end", 0);
    let mut body = Body::new(f.finish());
    body.clauses = vec![clause];
    let fuite = b.method(
        M_PUBLIC | M_STATIC,
        "Fuite",
        &method_sig(false, &[T_VOID], &[]),
        Some(body),
    );

    let mut p = Prog::default();
    p.label("c")
        .label("i")
        .il(Il::default().call(fuite))
        .leave("fin")
        .label("i_end");
    p.label("f").il(Il::default().op(POP).op(LDNULL).op(THROW));
    p.label("ih")
        .il(Il::default().op(POP))
        .il(mark(9))
        .leave("fin")
        .label("ih_end");
    p.label("c_end");
    p.label("ch")
        .il(Il::default().op(POP))
        .il(mark(2))
        .leave("fin")
        .label("ch_end");
    p.label("fin").il(Il::default().ldsfld(trace).ret());
    let clauses = vec![
        p.clause(CL_FILTER, "i", "i_end", "ih", "ih_end", p.at("f")),
        p.clause(CL_CATCH, "c", "c_end", "ch", "ch_end", exc),
    ];
    let mut body = Body::new(p.finish());
    body.clauses = clauses;
    let main = b.method(M_PUBLIC | M_STATIC, "Main", &main_sig(), Some(body));
    assert_eq!(lancer(&b.build(main), None), 12);
}

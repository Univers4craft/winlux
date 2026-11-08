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

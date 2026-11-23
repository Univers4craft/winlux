//! Suivi des constructeurs statiques des types intégrés au moteur (#2001).
//!
//! ECMA-335 §II.10.5.3.3 : un fil qui touche un type pendant qu'un autre
//! fil exécute son `.cctor` attend la fin de l'initialisation ; le fil qui
//! l'exécute, lui, peut y revenir (appel récursif) sans attendre ; une
//! attente qui fermerait un cycle entre fils (A attend B qui attend A) est
//! rompue plutôt que de bloquer. Si le `.cctor` lève, le premier accès lève
//! `TypeInitializationException` et chaque accès suivant relève la même.
//!
//! Le verrou global des fils managés (threads.rs) est rendu pendant
//! l'attente : le fil qui exécute le `.cctor` peut l'avoir rendu lui-même
//! (attente, appel natif), et il doit pouvoir le reprendre pour finir.

use std::collections::HashMap;
use std::sync::{Condvar, Mutex};
use std::thread::{self, ThreadId};

use super::exceptions::ClrException;
use super::threads;

enum Etat {
    /// `.cctor` en cours sur ce fil.
    EnCours(ThreadId),
    Fini,
    /// Échec : adresse de la `TypeInitializationException` à relever.
    Echec(usize),
}

/// Ce que l'appelant doit faire d'un accès à un type.
pub enum Acces {
    /// Le type est prêt, ou l'accès vient du fil qui l'initialise, ou
    /// l'attente fermerait un cycle : lire sans rien lancer.
    Pret,
    /// Premier accès : lancer le `.cctor` puis appeler [`Initialisations::terminer`].
    Lancer,
    /// Le `.cctor` a échoué : relever cette exception.
    Echec(ClrException),
}

#[derive(Default)]
struct Table {
    etats: HashMap<String, Etat>,
    /// Type attendu par chaque fil en attente (recherche de cycle).
    attentes: HashMap<ThreadId, String>,
}

#[derive(Default)]
pub struct Initialisations {
    table: Mutex<Table>,
    fin: Condvar,
}

impl Initialisations {
    /// Décide d'un accès au type `cle`, en attendant au besoin qu'un autre
    /// fil finisse de l'initialiser.
    pub fn entrer(&self, cle: &str) -> Acces {
        let moi = thread::current().id();
        let mut t = self.table.lock().unwrap();
        loop {
            match t.etats.get(cle) {
                None => {
                    t.etats.insert(cle.to_string(), Etat::EnCours(moi));
                    return Acces::Lancer;
                }
                Some(Etat::Fini) => return Acces::Pret,
                Some(Etat::Echec(p)) => {
                    return Acces::Echec(ClrException {
                        obj_ptr: *p as *mut u8,
                    })
                }
                Some(&Etat::EnCours(fil)) => {
                    if fil == moi || cycle(&t, fil, moi) {
                        return Acces::Pret;
                    }
                    t.attentes.insert(moi, cle.to_string());
                    drop(t);
                    // Verrou global rendu hors du verrou de la table : le
                    // fil qui le détient peut attendre la table.
                    let prise = threads::gil_release_all();
                    let mut u = self.table.lock().unwrap();
                    if matches!(u.etats.get(cle), Some(&Etat::EnCours(f)) if f == fil) {
                        u = self.fin.wait(u).unwrap();
                    }
                    drop(u);
                    threads::gil_restore(prise);
                    t = self.table.lock().unwrap();
                    t.attentes.remove(&moi);
                }
            }
        }
    }

    /// Fin du `.cctor` lancé après [`Acces::Lancer`] : `Err` porte la
    /// `TypeInitializationException` gardée pour les accès suivants.
    pub fn terminer(&self, cle: &str, r: Result<(), ClrException>) {
        let etat = match r {
            Ok(()) => Etat::Fini,
            Err(e) => Etat::Echec(e.obj_ptr as usize),
        };
        self.table
            .lock()
            .unwrap()
            .etats
            .insert(cle.to_string(), etat);
        self.fin.notify_all();
    }

    /// Exceptions gardées : racines du ramasse-miettes.
    pub fn racines(&self) -> Vec<usize> {
        let t = self.table.lock().unwrap();
        t.etats
            .values()
            .filter_map(|e| match e {
                Etat::Echec(p) => Some(*p),
                _ => None,
            })
            .collect()
    }
}

/// Attendre `fil` fermerait-il un cycle qui revient à `moi` ?
fn cycle(t: &Table, mut fil: ThreadId, moi: ThreadId) -> bool {
    for _ in 0..=t.attentes.len() {
        let Some(Etat::EnCours(suivant)) = t.attentes.get(&fil).and_then(|c| t.etats.get(c)) else {
            return false;
        };
        if *suivant == moi {
            return true;
        }
        fil = *suivant;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::Duration;

    fn lancer(a: Acces) -> bool {
        matches!(a, Acces::Lancer)
    }

    #[test]
    fn le_second_fil_attend_la_fin_du_constructeur_statique_2001() {
        let init = Arc::new(Initialisations::default());
        assert!(lancer(init.entrer("System.String")));
        let fini = Arc::new(AtomicBool::new(false));
        let (i2, f2) = (init.clone(), fini.clone());
        let autre = thread::spawn(move || {
            let pret = matches!(i2.entrer("System.String"), Acces::Pret);
            // Lu après le retour : le .cctor doit être fini.
            (pret, f2.load(Ordering::SeqCst))
        });
        thread::sleep(Duration::from_millis(100));
        fini.store(true, Ordering::SeqCst);
        init.terminer("System.String", Ok(()));
        let (pret, vu_fini) = autre.join().unwrap();
        println!("second fil : prêt={pret}, .cctor fini à son retour={vu_fini}");
        assert!(pret && vu_fini);
    }

    #[test]
    fn le_fil_qui_initialise_n_attend_pas_lui_meme_2001() {
        let init = Initialisations::default();
        assert!(lancer(init.entrer("System.Boolean")));
        assert!(matches!(init.entrer("System.Boolean"), Acces::Pret));
        init.terminer("System.Boolean", Ok(()));
        assert!(matches!(init.entrer("System.Boolean"), Acces::Pret));
    }

    #[test]
    fn l_echec_est_releve_a_chaque_acces_2001() {
        let init = Initialisations::default();
        assert!(lancer(init.entrer("System.Char")));
        let e = ClrException {
            obj_ptr: 0x1234 as *mut u8,
        };
        init.terminer("System.Char", Err(e));
        for _ in 0..3 {
            match init.entrer("System.Char") {
                Acces::Echec(r) => assert_eq!(r.obj_ptr, e.obj_ptr),
                _ => panic!("l'échec du .cctor doit être relevé"),
            }
        }
        assert_eq!(init.racines(), vec![0x1234]);
    }

    #[test]
    fn deux_initialisations_croisees_ne_se_bloquent_pas_2001() {
        // A initialise X puis touche Y ; B initialise Y puis touche X.
        let init = Arc::new(Initialisations::default());
        let barriere = Arc::new(Barrier::new(2));
        let fil = |moi: &'static str, autre: &'static str| {
            let (init, barriere) = (init.clone(), barriere.clone());
            thread::spawn(move || {
                assert!(lancer(init.entrer(moi)));
                barriere.wait();
                // L'un des deux attend l'autre ; le second ferme le cycle et
                // passe sans attendre, ce qui libère le premier.
                let a = init.entrer(autre);
                init.terminer(moi, Ok(()));
                matches!(a, Acces::Pret)
            })
        };
        let a = fil("X", "Y");
        let b = fil("Y", "X");
        let (ra, rb) = (a.join().unwrap(), b.join().unwrap());
        println!("cycle rompu : A prêt={ra}, B prêt={rb}");
        assert!(ra && rb);
    }
}

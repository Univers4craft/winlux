//! Fonctions privées de Winlux pour le Planificateur de tâches du bureau
//! (#2230). Le bureau lisait et écrivait lui-même `/etc/winlux/tasks`,
//! fermé en écriture aux comptes ordinaires : il passe maintenant par le
//! module des tâches, qui choisit le magasin du compte courant (celui du
//! système pour root, `~/.local/share/winlux/tasks` sinon, #1824).
//!
//! Les textes échangés sont en Latin-1, comme les fichiers `.task`.

use crate::admin;
use crate::tasks;

/// Copie `text` (Latin-1, terminé par un zéro) dans `out` ; rend la taille
/// nécessaire sans le zéro, comme les API « A » de Windows.
unsafe fn copy_out(text: &str, out: *mut u8, cap: u32) -> u32 {
    let bytes = admin::encode(text);
    if !out.is_null() && (bytes.len() as u64) < cap as u64 {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        *out.add(bytes.len()) = 0;
    }
    bytes.len() as u32
}

/// Liste des tâches visibles du compte courant : une ligne par tâche,
/// `id \t clé d'état \t fichier (chemin Windows) \t 1 si modifiable`.
/// Rend la taille nécessaire ; rien n'est écrit si `cap` est trop petit.
unsafe extern "win64" fn winlux_task_list(out: *mut u8, cap: u32) -> u32 {
    let mut text = String::new();
    for e in tasks::entries() {
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            e.id,
            e.key,
            crate::paths::to_windows(&e.file),
            u8::from(e.mine)
        ));
    }
    copy_out(&text, out, cap)
}

/// Enregistre une tâche composée par le bureau (`text`, format `.task`,
/// Latin-1) sous l'identifiant `id`. Rend 0, ou un code d'erreur Win32 :
/// 87 pour un identifiant invalide, 5 pour un refus d'écriture.
unsafe extern "win64" fn winlux_task_save(id: *const u8, text: *const u8, len: u32) -> u32 {
    if id.is_null() || (text.is_null() && len != 0) {
        return 87;
    }
    let id = crate::dlls::cstr(id);
    let bytes = if len == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(text, len as usize)
    };
    match tasks::save_text(&id, &admin::decode(bytes)) {
        Ok(()) => 0,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => 87,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => 5,
        Err(_) => 29, // ERROR_WRITE_FAULT
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinluxTaskList" => fn winlux_task_list,
        "WinluxTaskSave" => fn winlux_task_save,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enregistre_et_liste_par_le_bureau() {
        let _r = crate::admin::test_root::Root::new("taches-bureau");
        let text = "name=banc\nenabled=1\ntrigger=interval 1\naction=/bin/true\t\t\n";
        let r =
            unsafe { winlux_task_save(c"banc".as_ptr().cast(), text.as_ptr(), text.len() as u32) };
        assert_eq!(r, 0);
        // Un identifiant qui sortirait du magasin est refusé.
        let bad =
            unsafe { winlux_task_save(c"../x".as_ptr().cast(), text.as_ptr(), text.len() as u32) };
        assert_eq!(bad, 87);
        let mut buf = vec![0u8; 4096];
        let n = unsafe { winlux_task_list(buf.as_mut_ptr(), buf.len() as u32) } as usize;
        let listed = String::from_utf8_lossy(&buf[..n]).into_owned();
        let line = listed
            .lines()
            .find(|l| l.starts_with("banc\t"))
            .expect(&listed);
        let cols: Vec<&str> = line.split('\t').collect();
        assert_eq!(cols.len(), 4, "{line}");
        assert_eq!(cols[3], "1", "{line}");
        // Taille seule quand le tampon est trop petit.
        assert_eq!(
            unsafe { winlux_task_list(std::ptr::null_mut(), 0) } as usize,
            n
        );
    }
}

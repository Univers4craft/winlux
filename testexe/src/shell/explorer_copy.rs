//! Copier, couper-coller et glisser-déposer en tâche de fond (#11).
//!
//! L'Explorateur prépare les paires source → destination (noms « (2) »,
//! cycles, chemins trop longs), puis le runtime copie dans un fil à part
//! (`WinluxFileCopy`) : la fenêtre reste utilisable pendant la copie d'un
//! film ou d'une clé USB entière. Une carte façon Windows 11, en bas à
//! droite, montre le pourcentage, la vitesse, le temps restant, l'élément en
//! cours et un graphique du débit, avec Pause et Annuler.
//!
//! Quand des noms existent déjà dans la destination, la boîte « Remplacer ou
//! ignorer des fichiers » demande quoi faire, comme Windows : remplacer,
//! ignorer, ou conserver les deux (« nom (2) »). Copier dans le même dossier
//! garde toujours les deux, sans question.

use super::*;

/// Copies simultanées (une carte chacune).
const COPY_MAX: usize = 4;
/// Échantillons du graphique de débit (un toutes les 250 ms).
const HIST: usize = 48;

const FLAG_MOVE: u32 = 1;
const FLAG_REPLACE: u32 = 2;
const FLAG_SKIP: u32 = 4;
/// Démarrer en pause (tests seulement).
const FLAG_PAUSED: u32 = 8;
#[doc(hidden)]
static mut TEST_FLAGS: u32 = 0;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode {
    /// Demander si des noms existent déjà.
    Ask,
    KeepBoth,
    Replace,
    Skip,
}

#[derive(Clone, Copy)]
struct CopyJob {
    id: u32,
    moving: bool,
    /// Suppression définitive (Vider la Corbeille) : avancement en éléments.
    deleting: bool,
    count: u32,
    from: Path,
    to: Path,
    start: u32,
    paused: bool,
    done: u64,
    total: u64,
    items_done: u64,
    items_total: u64,
    /// Débit lissé, en octets par seconde.
    rate: u64,
    last_tick: u32,
    last_done: u64,
    hist: [u32; HIST],
    nhist: usize,
    name: [u8; NAME_LEN],
    name_len: usize,
}

const EMPTY_JOB: CopyJob = CopyJob {
    id: 0,
    moving: false,
    deleting: false,
    count: 0,
    from: EMPTY_PATH,
    to: EMPTY_PATH,
    start: 0,
    paused: false,
    done: 0,
    total: 0,
    items_done: 0,
    items_total: 0,
    rate: 0,
    last_tick: 0,
    last_done: 0,
    hist: [0; HIST],
    nhist: 0,
    name: [0; NAME_LEN],
    name_len: 0,
};

static mut COPIES: [CopyJob; COPY_MAX] = [EMPTY_JOB; COPY_MAX];

/// Liste « source NUL destination NUL … NUL » passée à `WinluxFileCopy`.
const LIST_LEN: usize = CLIP_MAX * 2 * (PATH_LEN + 1) + 1;
static mut LIST: [u16; LIST_LEN] = [0; LIST_LEN];

/// Transfert en attente de la réponse « Remplacer ou ignorer ».
static mut ASK_OPEN: bool = false;
static mut ASK_SRC: [Path; CLIP_MAX] = [EMPTY_PATH; CLIP_MAX];
static mut ASK_N: usize = 0;
static mut ASK_DEST: Path = EMPTY_PATH;
static mut ASK_MOVE: bool = false;
static mut ASK_CLIP: bool = false;
static mut ASK_CONFLICTS: usize = 0;
static mut ASK_FIRST: Path = EMPTY_PATH;

/// Bilan d'un transfert.
#[derive(Clone, Copy, Default)]
pub(super) struct Outcome {
    pub failed: u32,
    /// Éléments confiés à une copie de fond.
    pub queued: u32,
    /// Boîte « Remplacer ou ignorer » ouverte : rien n'est encore fait.
    pub asked: bool,
}

pub(super) unsafe fn busy() -> bool {
    COPIES.iter().any(|c| c.id != 0)
}

pub(super) unsafe fn dialog_open() -> bool {
    ASK_OPEN || EMPTY_ASK != 0
}

// ---------- Vider la Corbeille (#10) ----------

/// Éléments à supprimer quand la confirmation « Vider la Corbeille » est
/// ouverte, 0 sinon.
static mut EMPTY_ASK: u32 = 0;

/// Nombre d'éléments dans la Corbeille.
pub(super) unsafe fn trash_count() -> u32 {
    let mut pattern = [0u16; PATH_LEN + NAME_LEN + 2];
    if wjoin(&mut pattern, TRASH, b"*") == 0 {
        return 0;
    }
    let mut data: Win32FindDataW = core::mem::zeroed();
    let h = FindFirstFileW(pattern.as_ptr(), &mut data);
    if h as isize == -1 {
        return 0;
    }
    let mut n = 0;
    loop {
        let name = find_name_w(&data);
        if name != b"." && name != b".." && !name.is_empty() {
            n += 1;
        }
        if FindNextFileW(h, &mut data) == 0 {
            break;
        }
    }
    FindClose(h);
    n
}

/// « Vider la Corbeille » : demande confirmation, comme Windows.
pub(super) unsafe fn ask_empty_trash() {
    let n = trash_count();
    if n == 0 {
        notify(t4([
            b"La Corbeille est d\xE9j\xE0 vide.",
            b"The Recycle Bin is already empty.",
            b"La Papelera ya est\xE1 vac\xEDa.",
            b"Der Papierkorb ist bereits leer.",
        ]));
        return;
    }
    EMPTY_ASK = n;
}

/// Supprime définitivement tout le contenu de la Corbeille, en fond.
unsafe fn empty_trash() {
    empty_dirs(TRASH, TRASH_INFO);
}

unsafe fn empty_dirs(files: &[u8], info: &[u8]) {
    let mut at = 0;
    for part in [files, info] {
        for &c in part {
            LIST[at] = c as u16;
            at += 1;
        }
        LIST[at] = 0;
        at += 1;
    }
    LIST[at] = 0;
    let count = EMPTY_ASK;
    EMPTY_ASK = 0;
    let id = WinluxFileDelete(LIST.as_ptr(), 1);
    if start(id, false, count, TRASH, TRASH) {
        for c in COPIES.iter_mut() {
            if c.id == id {
                c.deleting = true;
            }
        }
    } else {
        notify(tr4(&[b"Certains \xE9l\xE9ments n'ont pas pu \xEAtre supprim\xE9s.", b"Some items couldn't be deleted.", b"Algunos elementos no se pudieron eliminar.", b"Einige Elemente konnten nicht gel\xF6scht werden."]));
    }
}

fn confirm_box(f: &Frame) -> Rect {
    let (w, h) = (440, 168);
    let cx = (f.client.left + f.client.right) / 2;
    let cy = (f.client.top + f.client.bottom) / 2;
    rect(cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2)
}

fn confirm_buttons(r: &Rect) -> (Rect, Rect) {
    let no = rect(r.right - 108, r.bottom - 44, r.right - 16, r.bottom - 14);
    let yes = rect(no.left - 100, no.top, no.left - 8, no.bottom);
    (yes, no)
}

unsafe fn draw_confirm(g: &mut Gfx, f: &Frame) {
    let p = pal();
    let r = confirm_box(f);
    round_rect(g.hdc, g.b, r.left - 1, r.top - 1, r.right + 1, r.bottom + 1, 9, p.border);
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.surface);
    text(g.hdc, g.f.semibold, p.text, r.left + 20, r.top + 18, tr(S_EMPTY_TRASH));
    let mut t = [0u8; 200];
    let mut n = 0;
    if EMPTY_ASK == 1 {
        push(
            &mut t,
            &mut n,
            t4([
                b"Voulez-vous vraiment supprimer d\xE9finitivement cet \xE9l\xE9ment ?",
                b"Are you sure you want to permanently delete this item?",
                b"\xBFSeguro que quiere eliminar permanentemente este elemento?",
                b"M\xF6chten Sie dieses Element wirklich endg\xFCltig l\xF6schen?",
            ]),
        );
    } else {
        push(
            &mut t,
            &mut n,
            t4([
                b"Voulez-vous vraiment supprimer d\xE9finitivement ces ",
                b"Are you sure you want to permanently delete these ",
                b"\xBFSeguro que quiere eliminar permanentemente estos ",
                b"M\xF6chten Sie diese ",
            ]),
        );
        n = write_num(&mut t, n, EMPTY_ASK as u64);
        push(
            &mut t,
            &mut n,
            t4([
                b" \xE9l\xE9ments ?",
                b" items?",
                b" elementos?",
                b" Elemente wirklich endg\xFCltig l\xF6schen?",
            ]),
        );
    }
    text_fit(g.hdc, g.f.caption, 12, p.text, r.left + 20, r.top + 52, &t[..n], r.right - r.left - 40);
    text_fit(
        g.hdc,
        g.f.small,
        11,
        p.text2,
        r.left + 20,
        r.top + 74,
        t4([
            b"Ils ne pourront plus \xEAtre restaur\xE9s.",
            b"They can't be restored afterwards.",
            b"Despu\xE9s no se podr\xE1n restaurar.",
            b"Sie k\xF6nnen danach nicht wiederhergestellt werden.",
        ]),
        r.right - r.left - 40,
    );
    let (yes, no) = confirm_buttons(&r);
    button(g, &yes, t4([b"Oui", b"Yes", b"S\xED", b"Ja"]), true);
    button(g, &no, t4([b"Non", b"No", b"No", b"Nein"]), false);
}



fn exists(path: &[u8]) -> bool {
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    unsafe { GetFileAttributesW(wstr(&mut w, path)) != u32::MAX }
}

/// Vrai si `src` est déjà dans le dossier `dest`.
unsafe fn in_folder(src: &[u8], dest: &[u8]) -> bool {
    let a = match parent(src) {
        Some(d) => canonical_path(d),
        None => None,
    };
    match a.zip(canonical_path(dest)) {
        Some((a, b)) => cmp_ci(a.get(), b.get()).is_eq(),
        None => false,
    }
}

fn split_name(name: &[u8]) -> (&[u8], &[u8]) {
    match name.iter().rposition(|&c| c == b'.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, &b""[..]),
    }
}

/// Copie ou déplace `srcs` dans `dest`. `from_clip` : le presse-papiers
/// est nettoyé après un couper-coller (même après la boîte de conflit).
pub(super) unsafe fn transfer(
    srcs: &[Path],
    dest: &[u8],
    moving: bool,
    mode: Mode,
    from_clip: bool,
) -> Outcome {
    let mut out = Outcome::default();
    if mode == Mode::Ask {
        let mut conflicts = 0;
        let mut first = EMPTY_PATH;
        for s in srcs {
            let mut direct = [0u8; PATH_LEN + NAME_LEN + 2];
            let dn = join(&mut direct, dest, last_component(s.get()));
            if dn != 0
                && exists(&direct[..dn])
                && !in_folder(s.get(), dest)
                && safe_destination(s.get(), dest)
            {
                if conflicts == 0 {
                    first.set(last_component(s.get()));
                }
                conflicts += 1;
            }
        }
        if conflicts > 0 {
            let n = srcs.len().min(CLIP_MAX);
            ASK_SRC[..n].copy_from_slice(&srcs[..n]);
            ASK_N = n;
            ASK_DEST.set(dest);
            ASK_MOVE = moving;
            ASK_CLIP = from_clip;
            ASK_CONFLICTS = conflicts;
            ASK_FIRST = first;
            ASK_OPEN = true;
            out.asked = true;
            return out;
        }
    }
    let mut at = 0usize;
    let mut pairs = 0u32;
    for s in srcs {
        let src = s.get();
        if moving && in_folder(src, dest) {
            // Couper puis coller au même endroit : rien à faire.
            continue;
        }
        if !safe_destination(src, dest) {
            out.failed += 1;
            continue;
        }
        let name = last_component(src);
        let mut direct = [0u8; PATH_LEN + NAME_LEN + 2];
        let dn = join(&mut direct, dest, name);
        if dn == 0 {
            out.failed += 1;
            continue;
        }
        let mut dst = [0u8; PATH_LEN + NAME_LEN + 2];
        let present = exists(&direct[..dn]);
        let n = if present && (in_folder(src, dest) || matches!(mode, Mode::KeepBoth | Mode::Ask))
        {
            let (base, ext) = split_name(name);
            unique_name(dest, base, ext, &mut dst)
        } else {
            dst[..dn + 1].copy_from_slice(&direct[..dn + 1]);
            dn
        };
        if n == 0 {
            out.failed += 1;
            continue;
        }
        let fresh = !exists(&dst[..n]);
        if fresh && moving {
            // Même disque : renommage instantané, sans tâche de fond.
            let mut ws = [0u16; PATH_LEN + NAME_LEN + 2];
            let mut wd = [0u16; PATH_LEN + NAME_LEN + 2];
            if MoveFileW(wstr(&mut ws, src), wstr(&mut wd, &dst[..n])) != 0 {
                continue;
            }
        }
        let mut ancestors = [EMPTY_PATH; 17];
        if !preflight_tree(src, &dst[..n], &mut ancestors, 0) {
            out.failed += 1;
            continue;
        }
        if at + src.len() + n + 3 >= LIST_LEN {
            out.failed += 1;
            continue;
        }
        for part in [src, &dst[..n]] {
            for &c in part {
                LIST[at] = c as u16;
                at += 1;
            }
            LIST[at] = 0;
            at += 1;
        }
        pairs += 1;
    }
    LIST[at] = 0;
    if pairs > 0 {
        let flags = if moving { FLAG_MOVE } else { 0 }
            | match mode {
                Mode::Replace => FLAG_REPLACE,
                Mode::Skip => FLAG_SKIP,
                _ => 0,
            }
            | TEST_FLAGS;
        let from = srcs.first().and_then(|s| parent(s.get())).unwrap_or(b"");
        if start(WinluxFileCopy(LIST.as_ptr(), flags), moving, pairs, from, dest) {
            out.queued = pairs;
        } else {
            out.failed += pairs;
        }
    }
    out
}

unsafe fn start(id: u32, moving: bool, count: u32, from: &[u8], to: &[u8]) -> bool {
    if id == 0 {
        return false;
    }
    let slot = COPIES.iter().position(|c| c.id == 0).unwrap_or(COPY_MAX - 1);
    if COPIES[slot].id != 0 {
        // Plus de carte libre : la plus ancienne n'est plus montrée, mais
        // sa tâche continue et finira seule.
        WinluxJobClose(COPIES[slot].id);
    }
    let c = &mut COPIES[slot];
    *c = EMPTY_JOB;
    c.id = id;
    c.moving = moving;
    c.count = count;
    c.from.set(from);
    c.to.set(to);
    c.start = GetTickCount();
    c.last_tick = c.start;
    true
}

/// Réponse à la boîte de conflit.
unsafe fn answer(mode: Option<Mode>) {
    ASK_OPEN = false;
    let Some(mode) = mode else {
        return;
    };
    let srcs = ASK_SRC;
    let dest = ASK_DEST;
    let out = transfer(&srcs[..ASK_N], dest.get(), ASK_MOVE, mode, ASK_CLIP);
    if ASK_CLIP && ASK_MOVE {
        super::after_cut_paste(out.failed);
    }
    super::after_transfer(out);
}

/// Suivi des copies ; vrai tant qu'il faut redessiner.
pub(super) unsafe fn tick() -> bool {
    let now = GetTickCount();
    let mut any = ASK_OPEN || EMPTY_ASK != 0;
    for k in 0..COPY_MAX {
        let c = &mut COPIES[k];
        if c.id == 0 {
            continue;
        }
        any = true;
        let mut stats = [0u64; 4];
        let mut wname = [0u16; NAME_LEN];
        let paused = WinluxJobStats(c.id, stats.as_mut_ptr(), wname.as_mut_ptr(), NAME_LEN as u32);
        if paused != u32::MAX {
            c.paused = paused == 1;
            [c.done, c.total, c.items_done, c.items_total] = stats;
            c.name_len = ansi_from_wide(&wname, &mut c.name);
        }
        let dt = now.wrapping_sub(c.last_tick);
        if dt >= 250 {
            let bytes = c.done.saturating_sub(c.last_done);
            let inst = bytes * 1000 / dt as u64;
            c.rate = if c.rate == 0 { inst } else { (c.rate * 3 + inst) / 4 };
            if c.nhist == HIST {
                c.hist.copy_within(1.., 0);
                c.nhist -= 1;
            }
            // Suppression : un élément compte pour un « octet ».
            let sample = if c.deleting { inst } else { inst / 1024 };
            c.hist[c.nhist] = sample.min(u32::MAX as u64) as u32;
            c.nhist += 1;
            c.last_tick = now;
            c.last_done = c.done;
        }
        let mut msg = [0u16; 512];
        let mut prog = 0u32;
        let st = WinluxJobPoll(c.id, &mut prog, msg.as_mut_ptr(), msg.len() as u32);
        if st == 0 {
            continue;
        }
        WinluxJobClose(c.id);
        let (moving, deleting) = (c.moving, c.deleting);
        *c = EMPTY_JOB;
        let mut m = [0u8; 512];
        let n = ansi_from_wide(&msg, &mut m);
        if deleting && st != 1 {
            notify_with(
                t4([b"Suppression interrompue : ", b"Deletion stopped: ", b"Eliminaci\xF3n interrumpida: ", b"L\xF6schen unterbrochen: "]),
                &m[..n],
            );
            finished(1, b"", false);
        } else {
            finished(st, &m[..n], moving);
        }
    }
    any
}

unsafe fn finished(st: u32, msg: &[u8], moving: bool) {
    match st {
        1 => {}
        7 => notify(if moving {
            t4([
                b"D\xE9placement annul\xE9. Les \xE9l\xE9ments d\xE9j\xE0 d\xE9plac\xE9s le restent.",
                b"Move cancelled. Items already moved stay where they are.",
                b"Movimiento cancelado. Los elementos ya movidos se quedan donde est\xE1n.",
                b"Verschieben abgebrochen. Bereits verschobene Elemente bleiben dort.",
            ])
        } else {
            t4([
                b"Copie annul\xE9e. Les \xE9l\xE9ments d\xE9j\xE0 copi\xE9s sont conserv\xE9s.",
                b"Copy cancelled. Items already copied are kept.",
                b"Copia cancelada. Se conservan los elementos ya copiados.",
                b"Kopieren abgebrochen. Bereits kopierte Elemente bleiben erhalten.",
            ])
        }),
        _ => notify_with(
            if moving {
                t4([
                    b"D\xE9placement impossible : ",
                    b"Move failed: ",
                    b"No se pudo mover: ",
                    b"Verschieben fehlgeschlagen: ",
                ])
            } else {
                t4([
                    b"Copie impossible : ",
                    b"Copy failed: ",
                    b"No se pudo copiar: ",
                    b"Kopieren fehlgeschlagen: ",
                ])
            },
            msg,
        ),
    }
    // Source et destination sont peut-être affichées dans d'autres onglets.
    for i in 0..NTABS {
        for which in 0..2 {
            reload(&mut TABS[i].panes[which]);
        }
    }
}

/// Copies d'un test : attend la fin de toutes les tâches.
#[doc(hidden)]
pub(super) unsafe fn wait_all() {
    for _ in 0..3000 {
        tick();
        if !busy() {
            return;
        }
        Sleep(10);
    }
}

#[doc(hidden)]
pub(super) unsafe fn first_job() -> u32 {
    COPIES.iter().find(|c| c.id != 0).map(|c| c.id).unwrap_or(0)
}

#[doc(hidden)]
pub(super) unsafe fn test_answer(mode: Option<Mode>) {
    answer(mode);
}

// ---------- Carte de progression ----------

const CARD_W: i32 = 400;
const CARD_H: i32 = 222;

/// Cartes visibles (après 400 ms, pour ne pas clignoter sur une petite copie).
unsafe fn visible(c: &CopyJob) -> bool {
    c.id != 0 && GetTickCount().wrapping_sub(c.start) >= 400
}

fn card_rect(f: &Frame, slot: i32) -> Rect {
    let right = f.client.right - 16;
    let bottom = f.client.bottom - STATUS_H - 12 - slot * (CARD_H + 10);
    rect(right - CARD_W, bottom - CARD_H, right, bottom)
}

fn card_buttons(r: &Rect) -> (Rect, Rect) {
    let cancel = rect(r.right - 104, r.bottom - 44, r.right - 16, r.bottom - 14);
    let pause = rect(cancel.left - 104, cancel.top, cancel.left - 8, cancel.bottom);
    (pause, cancel)
}

fn push(buf: &mut [u8], n: &mut usize, s: &[u8]) {
    let k = s.len().min(buf.len().saturating_sub(*n));
    buf[*n..*n + k].copy_from_slice(&s[..k]);
    *n += k;
}

/// « environ 3 minutes », « 1 h 12 min », « 40 secondes ».
fn eta_text(secs: u64, out: &mut [u8; 64]) -> usize {
    let mut n = 0;
    if secs >= 3600 {
        n = write_num(out, n, secs / 3600);
        push(out, &mut n, b" h ");
        n = write_num(out, n, secs % 3600 / 60);
        push(out, &mut n, b" min");
    } else if secs >= 90 {
        push(out, &mut n, t4([b"environ ", b"about ", b"unos ", b"etwa "]));
        n = write_num(out, n, (secs + 30) / 60);
        push(out, &mut n, t4([b" minutes", b" minutes", b" minutos", b" Minuten"]));
    } else {
        n = write_num(out, n, secs.max(1));
        push(out, &mut n, t4([b" secondes", b" seconds", b" segundos", b" Sekunden"]));
    }
    n
}

pub(super) unsafe fn button(g: &mut Gfx, r: &Rect, label: &[u8], primary: bool) {
    let p = pal();
    let bg = if primary {
        p.accent
    } else if hovered(r) {
        p.hover
    } else {
        p.window
    };
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.border);
    round_rect(g.hdc, g.b, r.left + 1, r.top + 1, r.right - 1, r.bottom - 1, 4, bg);
    text(
        g.hdc,
        g.f.caption,
        if primary { p.on_accent } else { p.text },
        (r.left + r.right - text_width(label, 12)) / 2,
        r.top + 8,
        label,
    );
}

unsafe fn draw_card(g: &mut Gfx, c: &CopyJob, r: &Rect) {
    let p = pal();
    round_rect(g.hdc, g.b, r.left - 1, r.top - 1, r.right + 1, r.bottom + 1, 9, p.border);
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.surface);
    let inner = r.right - r.left - 32;
    // « Copie de 3 éléments de Documents vers Clé USB »
    let mut t = [0u8; 600];
    let mut n = 0;
    push(
        &mut t,
        &mut n,
        if c.deleting {
            t4([b"Suppression de ", b"Deleting ", b"Eliminando ", b"L\xF6schen: "])
        } else if c.moving {
            t4([b"D\xE9placement de ", b"Moving ", b"Moviendo ", b"Verschieben: "])
        } else {
            t4([b"Copie de ", b"Copying ", b"Copiando ", b"Kopieren: "])
        },
    );
    n = write_num(&mut t, n, c.items_total.max(c.count as u64));
    push(
        &mut t,
        &mut n,
        if c.items_total.max(c.count as u64) == 1 {
            t4([b" \xE9l\xE9ment de ", b" item from ", b" elemento de ", b" Element von "])
        } else {
            t4([b" \xE9l\xE9ments de ", b" items from ", b" elementos de ", b" Elemente von "])
        },
    );
    push(&mut t, &mut n, last_component(c.from.get()));
    if !c.deleting {
        push(&mut t, &mut n, t4([b" vers ", b" to ", b" a ", b" nach "]));
        push(&mut t, &mut n, last_component(c.to.get()));
    }
    text_fit(g.hdc, g.f.small, 11, p.text2, r.left + 16, r.top + 14, &t[..n], inner);
    // « 42 % terminé » ou « En pause ».
    let pct = if c.total == 0 { 0 } else { (c.done.min(c.total) * 100 / c.total) as u32 };
    let mut l = [0u8; 64];
    let mut ln = 0;
    if c.paused {
        push(&mut l, &mut ln, t4([b"En pause", b"Paused", b"En pausa", b"Angehalten"]));
    } else {
        ln = write_num(&mut l, 0, pct as u64);
        push(
            &mut l,
            &mut ln,
            t4([b" % termin\xE9", b"% complete", b" % completado", b" % abgeschlossen"]),
        );
    }
    text(g.hdc, g.f.semibold, p.text, r.left + 16, r.top + 32, &l[..ln]);
    // Graphique du débit, barre d'avancement en fond.
    let gr = rect(r.left + 16, r.top + 56, r.right - 16, r.top + 104);
    fill(g.hdc, g.b, gr.left, gr.top, gr.right, gr.bottom, p.window);
    let gw = gr.right - gr.left;
    let donew = gw * pct as i32 / 100;
    let tint = if c.paused { p.text3 } else { p.accent };
    fill(g.hdc, g.b, gr.left, gr.top, gr.left + donew, gr.bottom, p.selected);
    // Comme Windows : le débit se dessine jusqu'au point d'avancement.
    let peak = c.hist[..c.nhist].iter().copied().max().unwrap_or(0).max(1);
    let cols = c.nhist.max(1) as i32;
    for (i, &v) in c.hist[..c.nhist].iter().enumerate() {
        let h = ((gr.bottom - gr.top - 4) as u64 * v as u64 / peak as u64) as i32;
        let x = gr.left + i as i32 * donew / cols;
        let x2 = gr.left + (i as i32 + 1) * donew / cols;
        if h > 0 && x2 > x {
            fill(g.hdc, g.b, x, gr.bottom - h, x2, gr.bottom, tint);
        }
    }
    fill(g.hdc, g.b, gr.left, gr.bottom - 1, gr.right, gr.bottom, p.border);
    // Vitesse, nom, temps et éléments restants.
    let mut line = [0u8; 320];
    let mut k = 0;
    push(&mut line, &mut k, t4([b"Vitesse : ", b"Speed: ", b"Velocidad: ", b"Geschwindigkeit: "]));
    let mut sb = [0u8; 24];
    if c.deleting {
        // Avancement en éléments : « 120 éléments/s ».
        k = write_num(&mut line, k, if c.paused { 0 } else { c.rate });
        push(&mut line, &mut k, t4([b" \xE9l\xE9ments/s", b" items/s", b" elementos/s", b" Elemente/s"]));
    } else {
        let sn = size_text(if c.paused { 0 } else { c.rate }, &mut sb);
        push(&mut line, &mut k, &sb[..sn]);
        push(&mut line, &mut k, b"/s");
    }
    text(g.hdc, g.f.small, p.text2, r.left + 16, r.top + 110, &line[..k]);
    k = 0;
    push(&mut line, &mut k, t4([b"Nom : ", b"Name: ", b"Nombre: ", b"Name: "]));
    push(&mut line, &mut k, &c.name[..c.name_len]);
    text_fit(g.hdc, g.f.small, 11, p.text2, r.left + 16, r.top + 126, &line[..k], inner);
    k = 0;
    push(
        &mut line,
        &mut k,
        t4([b"Temps restant : ", b"Time remaining: ", b"Tiempo restante: ", b"Verbleibende Zeit: "]),
    );
    if c.paused {
        push(&mut line, &mut k, b"-");
    } else if c.rate == 0 || c.nhist < 2 {
        push(
            &mut line,
            &mut k,
            t4([b"calcul en cours\x85", b"calculating\x85", b"calculando\x85", b"wird berechnet\x85"]),
        );
    } else {
        let mut e = [0u8; 64];
        let en = eta_text(c.total.saturating_sub(c.done) / c.rate.max(1), &mut e);
        push(&mut line, &mut k, &e[..en]);
    }
    text(g.hdc, g.f.small, p.text2, r.left + 16, r.top + 142, &line[..k]);
    k = 0;
    push(
        &mut line,
        &mut k,
        t4([b"\xC9l\xE9ments restants : ", b"Items remaining: ", b"Elementos restantes: ", b"Verbleibende Elemente: "]),
    );
    k = write_num(&mut line, k, c.items_total.saturating_sub(c.items_done));
    if !c.deleting {
        push(&mut line, &mut k, b" (");
        let sn = size_text(c.total.saturating_sub(c.done), &mut sb);
        push(&mut line, &mut k, &sb[..sn]);
        push(&mut line, &mut k, b")");
    }
    text(g.hdc, g.f.small, p.text2, r.left + 16, r.top + 158, &line[..k]);
    let (pause, cancel) = card_buttons(r);
    button(
        g,
        &pause,
        if c.paused {
            t4([b"Reprendre", b"Resume", b"Reanudar", b"Fortsetzen"])
        } else {
            t4([b"Pause", b"Pause", b"Pausa", b"Anhalten"])
        },
        false,
    );
    button(g, &cancel, t4([b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"]), false);
}

// ---------- Boîte « Remplacer ou ignorer des fichiers » ----------

fn ask_box(f: &Frame) -> Rect {
    let (w, h) = (460, 286);
    let cx = (f.client.left + f.client.right) / 2;
    let cy = (f.client.top + f.client.bottom) / 2;
    rect(cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2)
}

fn ask_rows(r: &Rect) -> [Rect; 3] {
    let row = |i: i32| rect(r.left + 16, r.top + 100 + i * 46, r.right - 16, r.top + 140 + i * 46);
    [row(0), row(1), row(2)]
}

fn ask_cancel(r: &Rect) -> Rect {
    rect(r.right - 108, r.bottom - 44, r.right - 16, r.bottom - 14)
}

const ASK_MODES: [Mode; 3] = [Mode::Replace, Mode::Skip, Mode::KeepBoth];

unsafe fn draw_ask(g: &mut Gfx, f: &Frame) {
    let p = pal();
    let r = ask_box(f);
    round_rect(g.hdc, g.b, r.left - 1, r.top - 1, r.right + 1, r.bottom + 1, 9, p.border);
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.surface);
    let inner = r.right - r.left - 40;
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        r.left + 20,
        r.top + 18,
        t4([
            b"Remplacer ou ignorer des fichiers",
            b"Replace or skip files",
            b"Reemplazar u omitir archivos",
            b"Dateien ersetzen oder \xFCberspringen",
        ]),
    );
    let mut t = [0u8; 400];
    let mut n = 0;
    if ASK_CONFLICTS == 1 {
        push(
            &mut t,
            &mut n,
            t4([
                b"La destination contient d\xE9j\xE0 un \xE9l\xE9ment nomm\xE9 \xAB ",
                b"The destination already has an item named \x93",
                b"El destino ya contiene un elemento llamado \xAB",
                b"Das Ziel enth\xE4lt bereits ein Element namens \x84",
            ]),
        );
        push(&mut t, &mut n, ASK_FIRST.get());
        push(&mut t, &mut n, t4([b" \xBB.", b"\x94.", b"\xBB.", b"\x93."]));
    } else {
        push(
            &mut t,
            &mut n,
            t4([
                b"La destination contient d\xE9j\xE0 ",
                b"The destination already has ",
                b"El destino ya contiene ",
                b"Das Ziel enth\xE4lt bereits ",
            ]),
        );
        n = write_num(&mut t, n, ASK_CONFLICTS as u64);
        push(
            &mut t,
            &mut n,
            t4([
                b" \xE9l\xE9ments portant le m\xEAme nom.",
                b" items with the same name.",
                b" elementos con el mismo nombre.",
                b" Elemente mit demselben Namen.",
            ]),
        );
    }
    text_fit(g.hdc, g.f.caption, 12, p.text, r.left + 20, r.top + 48, &t[..n], inner);
    text_fit(
        g.hdc,
        g.f.small,
        11,
        p.text2,
        r.left + 20,
        r.top + 70,
        ASK_DEST.get(),
        inner,
    );
    let labels: [&[u8]; 3] = [
        t4([
            b"Remplacer les fichiers dans la destination",
            b"Replace the files in the destination",
            b"Reemplazar los archivos en el destino",
            b"Dateien im Ziel ersetzen",
        ]),
        t4([
            b"Ignorer ces fichiers",
            b"Skip these files",
            b"Omitir estos archivos",
            b"Diese Dateien \xFCberspringen",
        ]),
        t4([
            b"Conserver les deux (le nouveau devient \xAB nom (2) \xBB)",
            b"Keep both (the new one becomes \x93name (2)\x94)",
            b"Conservar ambos (el nuevo pasa a \xABnombre (2)\xBB)",
            b"Beide behalten (das neue hei\xDFt \x84Name (2)\x93)",
        ]),
    ];
    for (row, label) in ask_rows(&r).iter().zip(labels) {
        let bg = if hovered(row) { p.hover } else { p.window };
        round_rect(g.hdc, g.b, row.left, row.top, row.right, row.bottom, 4, p.border);
        round_rect(g.hdc, g.b, row.left + 1, row.top + 1, row.right - 1, row.bottom - 1, 4, bg);
        fill(g.hdc, g.b, row.left + 12, row.top + 14, row.left + 24, row.top + 26, p.accent);
        text_fit(g.hdc, g.f.caption, 12, p.text, row.left + 36, row.top + 13, label, row.right - row.left - 48);
    }
    button(g, &ask_cancel(&r), t4([b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"]), false);
}

// ---------- Entrées ----------

pub(super) unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let mut slot = 0;
    for k in 0..COPY_MAX {
        let c = COPIES[k];
        if visible(&c) {
            draw_card(g, &c, &card_rect(f, slot));
            slot += 1;
        }
    }
    if ASK_OPEN {
        draw_ask(g, f);
    }
    if EMPTY_ASK != 0 {
        draw_confirm(g, f);
    }
}

/// Clics sur les cartes (Pause, Annuler) et la boîte de conflit, qui est
/// modale. Vrai si le clic est pris.
pub(super) unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    if EMPTY_ASK != 0 {
        if ev == Mouse::Down {
            let (yes, no) = confirm_buttons(&confirm_box(f));
            if inside(&yes, x, y) {
                empty_trash();
            } else if inside(&no, x, y) {
                EMPTY_ASK = 0;
            }
        }
        return true;
    }
    if ASK_OPEN {
        if ev == Mouse::Down {
            let r = ask_box(f);
            if let Some(i) = ask_rows(&r).iter().position(|row| inside(row, x, y)) {
                answer(Some(ASK_MODES[i]));
            } else if inside(&ask_cancel(&r), x, y) {
                answer(None);
            }
        }
        return true;
    }
    let mut slot = 0;
    for k in 0..COPY_MAX {
        let c = COPIES[k];
        if !visible(&c) {
            continue;
        }
        let r = card_rect(f, slot);
        slot += 1;
        if !inside(&r, x, y) {
            continue;
        }
        if ev == Mouse::Down {
            let (pause, cancel) = card_buttons(&r);
            if inside(&pause, x, y) {
                WinluxJobPause(c.id, u32::from(!c.paused));
                COPIES[k].paused = !c.paused;
            } else if inside(&cancel, x, y) {
                // Une copie en pause doit pouvoir s'arrêter.
                WinluxJobCancel(c.id);
                WinluxJobPause(c.id, 0);
            }
        }
        return true;
    }
    false
}

/// Clavier de la boîte de conflit : R, I (S), C (K) ou Échap.
pub(super) unsafe fn key(_vk: u32, ch: u16) -> bool {
    if EMPTY_ASK != 0 {
        match ch {
            13 => empty_trash(),
            27 => EMPTY_ASK = 0,
            c if c < 128 && matches!((c as u8).to_ascii_lowercase(), b'o' | b'y' | b's' | b'j') => empty_trash(),
            c if c < 128 && matches!((c as u8).to_ascii_lowercase(), b'n') => EMPTY_ASK = 0,
            _ => {}
        }
        return true;
    }
    match ch {
        27 => answer(None),
        13 => answer(Some(Mode::Replace)),
        c if c < 128 => match (c as u8).to_ascii_lowercase() {
            b'r' | b'e' => answer(Some(Mode::Replace)),
            b'i' | b's' | b'o' | b'u' => answer(Some(Mode::Skip)),
            b'c' | b'k' | b'b' => answer(Some(Mode::KeepBoth)),
            _ => {}
        },
        _ => {}
    }
    true
}

/// Échap dans la liste : annule la copie la plus récente.
pub(super) unsafe fn cancel_latest() -> bool {
    match COPIES.iter().rev().find(|c| c.id != 0) {
        Some(c) => {
            WinluxJobCancel(c.id);
            WinluxJobPause(c.id, 0);
            true
        }
        None => false,
    }
}

// ---------- Régression (explorer_regression.exe) ----------

#[doc(hidden)]
unsafe fn write_file(path: &[u8], data: &[u8], repeat: usize) -> bool {
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    let h = CreateFileW(wstr(&mut w, path), 0x4000_0000, 0, core::ptr::null_mut(), 2, 0x80, core::ptr::null_mut());
    if h.is_null() || h as isize == -1 {
        return false;
    }
    let mut ok = true;
    for _ in 0..repeat {
        let mut written = 0u32;
        ok &= WriteFile(h, data.as_ptr(), data.len() as u32, &mut written, core::ptr::null_mut()) != 0
            && written as usize == data.len();
    }
    CloseHandle(h);
    ok
}

#[doc(hidden)]
unsafe fn first_bytes(path: &[u8], out: &mut [u8; 16]) -> usize {
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    let h = CreateFileW(wstr(&mut w, path), 0x8000_0000, 7, core::ptr::null_mut(), 3, 0x80, core::ptr::null_mut());
    if h.is_null() || h as isize == -1 {
        return 0;
    }
    let mut read = 0u32;
    ReadFile(h, out.as_mut_ptr(), 16, &mut read, core::ptr::null_mut());
    CloseHandle(h);
    read as usize
}

/// Copie de fond, conflits, pause et annulation.
#[doc(hidden)]
pub(super) unsafe fn regression(root: &[u8]) {
    let mut a = [0u8; PATH_LEN + NAME_LEN + 2];
    let mut b = [0u8; PATH_LEN + NAME_LEN + 2];
    let an = join(&mut a, root, b"copie-a");
    let bn = join(&mut b, root, b"copie-b");
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    CreateDirectoryW(wstr(&mut w, &a[..an]), core::ptr::null_mut());
    CreateDirectoryW(wstr(&mut w, &b[..bn]), core::ptr::null_mut());
    let mut fa = [0u8; PATH_LEN + NAME_LEN + 2];
    let mut fb = [0u8; PATH_LEN + NAME_LEN + 2];
    let fan = join(&mut fa, &a[..an], b"note.txt");
    let fbn = join(&mut fb, &b[..bn], b"note.txt");
    write_file(&fa[..fan], b"nouveau", 1);
    write_file(&fb[..fbn], b"ancien", 1);
    let mut src = [EMPTY_PATH; 1];
    src[0].set(&fa[..fan]);
    let out = transfer(&src, &b[..bn], false, Mode::Ask, false);
    check!(
        out.asked && dialog_open() && ASK_CONFLICTS == 1 && ASK_FIRST.get() == b"note.txt",
        "Coller sur un nom existant ouvre \"Remplacer ou ignorer des fichiers\""
    );
    test_answer(Some(Mode::Skip));
    wait_all();
    let mut head = [0u8; 16];
    let n = first_bytes(&fb[..fbn], &mut head);
    check!(!dialog_open() && &head[..n] == b"ancien", "Ignorer garde le fichier de la destination");
    transfer(&src, &b[..bn], false, Mode::Ask, false);
    test_answer(Some(Mode::KeepBoth));
    wait_all();
    let mut both = [0u8; PATH_LEN + NAME_LEN + 2];
    let bon = join(&mut both, &b[..bn], b"note (2).txt");
    let n = first_bytes(&both[..bon], &mut head);
    check!(&head[..n] == b"nouveau", "Conserver les deux cree \"note (2).txt\"");
    transfer(&src, &b[..bn], false, Mode::Ask, false);
    test_answer(Some(Mode::Replace));
    wait_all();
    let n = first_bytes(&fb[..fbn], &mut head);
    check!(&head[..n] == b"nouveau" && exists(&fa[..fan]), "Remplacer ecrase la destination et garde la source");
    // Annuler la boîte : rien n'est fait.
    transfer(&src, &b[..bn], true, Mode::Ask, false);
    test_answer(None);
    check!(!busy() && exists(&fa[..fan]), "Annuler la boite de conflit ne deplace rien");
    // Gros fichier : la copie se fait en fond, se met en pause et s'annule.
    let mut big = [0u8; PATH_LEN + NAME_LEN + 2];
    let bgn = join(&mut big, &a[..an], b"gros.bin");
    let chunk = [0x5Au8; 65536];
    check!(write_file(&big[..bgn], &chunk, 48), "Prepare un fichier de 3 Mo");
    let mut one = [EMPTY_PATH; 1];
    one[0].set(&big[..bgn]);
    let t0 = GetTickCount();
    TEST_FLAGS = FLAG_PAUSED;
    let out = transfer(&one, &b[..bn], false, Mode::Ask, false);
    TEST_FLAGS = 0;
    let id = first_job();
    check!(
        out.queued == 1 && id != 0 && GetTickCount().wrapping_sub(t0) < 1000,
        "Une copie de 3 Mo rend la main tout de suite (tache de fond)"
    );
    Sleep(100);
    let mut s1 = [0u64; 4];
    let p1 = WinluxJobStats(id, s1.as_mut_ptr(), core::ptr::null_mut(), 0);
    tick();
    let c = COPIES.iter().find(|c| c.id == id).copied().unwrap_or(EMPTY_JOB);
    check!(
        p1 == 1 && s1[0] == 0 && c.paused,
        "Pause : la copie n'avance plus et la carte affiche \"En pause\""
    );
    check!(cancel_latest(), "Echap annule la copie en cours, meme en pause");
    wait_all();
    let mut dbig = [0u8; PATH_LEN + NAME_LEN + 2];
    let dbn = join(&mut dbig, &b[..bn], b"gros.bin");
    let mut part = [0u8; PATH_LEN + NAME_LEN + 2];
    let pn = join(&mut part, &b[..bn], b".~gros.bin.winlux-copie");
    check!(
        !busy() && !exists(&dbig[..dbn]) && !exists(&part[..pn]) && exists(&big[..bgn]),
        "Une copie annulee ne laisse aucun fichier tronque"
    );
    // Copie complète, suivie jusqu'au bout.
    transfer(&one, &b[..bn], false, Mode::Ask, false);
    wait_all();
    let mut wd = [0u16; PATH_LEN + NAME_LEN + 2];
    let mut data: Win32FindDataW = core::mem::zeroed();
    let h = FindFirstFileW(wstr(&mut wd, &dbig[..dbn]), &mut data);
    let size = if h as isize == -1 {
        0
    } else {
        FindClose(h);
        (data.size_high as u64) << 32 | data.size_low as u64
    };
    check!(size == 3 << 20, "La copie de fond arrive au bout (3 Mo)");
    // Déplacer vers un autre dossier : la source disparaît.
    let mut c2 = [0u8; PATH_LEN + NAME_LEN + 2];
    let c2n = join(&mut c2, root, b"copie-c");
    CreateDirectoryW(wstr(&mut w, &c2[..c2n]), core::ptr::null_mut());
    let mut moved = [EMPTY_PATH; 1];
    moved[0].set(&dbig[..dbn]);
    transfer(&moved, &c2[..c2n], true, Mode::Ask, false);
    wait_all();
    let mut dest = [0u8; PATH_LEN + NAME_LEN + 2];
    let dn = join(&mut dest, &c2[..c2n], b"gros.bin");
    check!(exists(&dest[..dn]) && !exists(&dbig[..dbn]), "Deplacer retire la source");
}

/// Vider la Corbeille, sur des dossiers de test (jamais la vraie Corbeille).
#[doc(hidden)]
pub(super) unsafe fn trash_regression(root: &[u8]) {
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    let mut files = [0u8; PATH_LEN + NAME_LEN + 2];
    let mut info = [0u8; PATH_LEN + NAME_LEN + 2];
    let fl = join(&mut files, root, b"corbeille-files");
    let il = join(&mut info, root, b"corbeille-info");
    CreateDirectoryW(wstr(&mut w, &files[..fl]), core::ptr::null_mut());
    CreateDirectoryW(wstr(&mut w, &info[..il]), core::ptr::null_mut());
    let mut x = [0u8; PATH_LEN + NAME_LEN + 2];
    let xl = join(&mut x, &files[..fl], b"vieux dossier");
    CreateDirectoryW(wstr(&mut w, &x[..xl]), core::ptr::null_mut());
    let mut y = [0u8; PATH_LEN + NAME_LEN + 2];
    let yl = join(&mut y, &x[..xl], b"photo.jpg");
    write_file(&y[..yl], b"jpeg", 1);
    let yl2 = join(&mut y, &info[..il], b"vieux dossier.trashinfo");
    write_file(&y[..yl2], b"[Trash Info]\n", 1);
    EMPTY_ASK = 2;
    key(0, 27);
    check!(EMPTY_ASK == 0 && !busy() && exists(&x[..xl]), "Vider la Corbeille : Non ne supprime rien");
    EMPTY_ASK = 2;
    empty_dirs(&files[..fl], &info[..il]);
    check!(EMPTY_ASK == 0 && busy(), "Vider la Corbeille : suppression en tache de fond");
    wait_all();
    check!(
        !exists(&x[..xl]) && !exists(&y[..yl2]) && exists(&files[..fl]) && exists(&info[..il]),
        "Vider la Corbeille supprime fichiers et fiches, garde les dossiers"
    );
}

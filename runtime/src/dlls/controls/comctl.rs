//! Contrôles communs de comctl32 : arborescence (SysTreeView32), liste à
//! colonnes (SysListView32), onglets (SysTabControl32), barre d'état
//! (msctls_statusbar32), barre de progression (msctls_progress32), curseur
//! (msctls_trackbar32), flèches haut/bas (msctls_updown32) et info-bulles
//! (tooltips_class32, sans affichage).
//!
//! Même principe que les contrôles de user32 : une fenêtre enfant peinte
//! par Winlux, qui prévient son parent par `WM_NOTIFY`.

use super::*;

const WM_NOTIFY: u32 = 0x004E;

const NM_CLICK: i32 = -2;
const NM_DBLCLK: i32 = -3;
const NM_RETURN: i32 = -4;
const NM_SETFOCUS: i32 = -7;
const NM_KILLFOCUS: i32 = -8;

#[repr(C)]
#[derive(Clone, Copy)]
struct NmHdr {
    hwnd_from: usize,
    id_from: usize,
    code: i32,
}

/// Envoie `WM_NOTIFY` au parent avec une structure qui commence par NMHDR.
fn notify_struct<T>(hwnd: usize, code: i32, data: &mut T) -> isize {
    let hdr = data as *mut T as *mut NmHdr;
    let id = user32::control_id(hwnd) as usize;
    unsafe {
        (*hdr).hwnd_from = hwnd;
        (*hdr).id_from = id;
        (*hdr).code = code;
    }
    send(parent(hwnd), WM_NOTIFY, id, data as *mut T as isize)
}

fn notify_simple(hwnd: usize, code: i32) -> isize {
    let mut hdr = NmHdr {
        hwnd_from: 0,
        id_from: 0,
        code: 0,
    };
    notify_struct(hwnd, code, &mut hdr)
}

/// Texte d'une structure TVITEM/LVITEM/TCITEM (A ou W selon le message).
unsafe fn read_text_ptr(ptr: usize, wide: bool) -> Option<String> {
    if ptr == 0 || ptr == usize::MAX {
        return None;
    }
    Some(if wide {
        super::super::wstr(ptr as *const u16)
    } else {
        super::super::cstr(ptr as *const u8)
    })
}

unsafe fn write_text_ptr(ptr: usize, cap: i32, s: &str, wide: bool) {
    if ptr == 0 || cap <= 0 {
        return;
    }
    if wide {
        super::super::put_text_w(s, ptr as *mut u16, cap as u32);
    } else {
        super::super::put_text_a(s, ptr as *mut u8, cap as u32);
    }
}

#[derive(Clone, Default)]
pub(super) struct CommonState {
    // TreeView
    tree: Vec<TreeNode>,
    next_item: usize,
    selected_item: usize,
    // ListView
    columns: Vec<(String, i32)>,
    rows: Vec<LvRow>,
    lv_focus: i32,
    lv_top: i32,
    // Onglets / barre d'état
    tabs: Vec<(String, isize)>,
    tab_sel: i32,
    parts: Vec<i32>,
    part_text: Vec<String>,
    simple: bool,
    // Progression / curseur / flèches
    lo: i32,
    hi: i32,
    value: i32,
    step: i32,
    page: i32,
    buddy: usize,
    dragging: bool,
}

#[derive(Clone)]
struct TreeNode {
    handle: usize,
    parent: usize,
    text: String,
    param: isize,
    state: u32,
    children_hint: i32,
}

#[derive(Clone, Default)]
struct LvRow {
    texts: Vec<String>,
    param: isize,
    state: u32,
    /// Bit k : le texte de la colonne k est fourni à la demande
    /// (LPSTR_TEXTCALLBACK, ou liste virtuelle LVS_OWNERDATA).
    callback: u64,
}

fn common() -> &'static Mutex<HashMap<usize, CommonState>> {
    static S: OnceLock<Mutex<HashMap<usize, CommonState>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn with_common<R>(hwnd: usize, f: impl FnOnce(&mut CommonState) -> R) -> R {
    let mut map = lock(common());
    f(map.entry(hwnd).or_default())
}

fn common_snapshot(hwnd: usize) -> CommonState {
    lock(common()).get(&hwnd).cloned().unwrap_or_default()
}

pub(super) fn forget(hwnd: usize) {
    lock(common()).remove(&hwnd);
}

pub(super) fn class_kind(name: &str) -> Option<Kind> {
    Some(match name {
        "systreeview32" => Kind::TreeView,
        "syslistview32" => Kind::ListView,
        "systabcontrol32" => Kind::Tab,
        "msctls_statusbar32" => Kind::StatusBar,
        "msctls_progress32" => Kind::Progress,
        "msctls_trackbar32" => Kind::TrackBar,
        "msctls_updown32" => Kind::UpDown,
        "tooltips_class32" => Kind::ToolTip,
        _ => return None,
    })
}

pub(super) fn init(kind: Kind, hwnd: usize) {
    with_common(hwnd, |s| {
        s.next_item = 0x1000;
        s.tab_sel = -1;
        s.lv_focus = -1;
        s.hi = 100;
        s.step = 10;
        s.page = 10;
        if kind == Kind::UpDown {
            s.lo = 100;
            s.hi = 0;
        }
    });
    match kind {
        Kind::StatusBar => {
            // Le titre de la fenêtre est le texte de la première partie.
            let t = title(hwnd);
            with_common(hwnd, |s| s.part_text = vec![t]);
            place_status_bar(hwnd);
        }
        Kind::UpDown => attach_buddy(hwnd),
        Kind::ToolBar | Kind::ReBar | Kind::ComboBoxEx => super::bars::init(kind, hwnd),
        Kind::ToolTip => {
            gui::set_window_style(hwnd, style(hwnd) & !0x1000_0000);
            unsafe {
                user32::show_window(hwnd, 0);
            }
        }
        _ => {}
    }
}

pub(super) fn paint(kind: Kind, hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    match kind {
        Kind::TreeView => paint_tree(hwnd, c, p, r),
        Kind::ListView => paint_listview(hwnd, c, p, r),
        Kind::Tab => paint_tabs(hwnd, p, r),
        Kind::StatusBar => paint_status(hwnd, p, r),
        Kind::Progress => paint_progress(hwnd, p, r),
        Kind::TrackBar => paint_trackbar(hwnd, p, r),
        Kind::UpDown => paint_updown(hwnd, c, p, r),
        Kind::ToolBar | Kind::ReBar | Kind::ComboBoxEx => super::bars::paint(kind, hwnd, c, p, r),
        _ => {}
    }
}

pub(super) unsafe fn proc_impl(
    kind: Kind,
    hwnd: usize,
    msg: u32,
    wp: usize,
    lp: isize,
) -> Option<isize> {
    match kind {
        Kind::TreeView => tree_proc(hwnd, msg, wp, lp),
        Kind::ListView => listview_proc(hwnd, msg, wp, lp),
        Kind::Tab => tab_proc(hwnd, msg, wp, lp),
        Kind::StatusBar => status_proc(hwnd, msg, wp, lp),
        Kind::Progress => progress_proc(hwnd, msg, wp, lp),
        Kind::TrackBar => trackbar_proc(hwnd, msg, wp, lp),
        Kind::UpDown => updown_proc(hwnd, msg, wp, lp),
        Kind::ToolBar | Kind::ReBar | Kind::ComboBoxEx => {
            super::bars::proc_impl(kind, hwnd, msg, wp, lp)
        }
        Kind::ToolTip => Some(match msg {
            0x0404 | 0x0432 => 1, // TTM_ADDTOOLA/W
            _ if (0x0400..0x0500).contains(&msg) => 0,
            _ => return None,
        }),
        _ => None,
    }
}

// =====================================================================
// SysTreeView32
// =====================================================================

const TVI_ROOT: usize = 0xFFFF_FFFF_FFFF_0000;
const TVI_FIRST: usize = 0xFFFF_FFFF_FFFF_0001;
const TVI_LAST: usize = 0xFFFF_FFFF_FFFF_0002;
const TVI_SORT: usize = 0xFFFF_FFFF_FFFF_0003;
const TVIF_TEXT: u32 = 0x1;
const TVIF_PARAM: u32 = 0x4;
const TVIF_STATE: u32 = 0x8;
const TVIF_HANDLE: u32 = 0x10;
const TVIF_CHILDREN: u32 = 0x40;
const TVIS_SELECTED: u32 = 0x2;
const TVIS_EXPANDED: u32 = 0x20;
const TVIS_BOLD: u32 = 0x10;
const TVS_HASBUTTONS: u32 = 0x1;
const TVS_HASLINES: u32 = 0x2;
const TVS_LINESATROOT: u32 = 0x4;

/// TVITEMW / TVITEMA (64 bits) : mêmes positions, texte A ou W.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TvItem {
    mask: u32,
    item: usize,
    state: u32,
    state_mask: u32,
    text: usize,
    text_max: i32,
    image: i32,
    selected_image: i32,
    children: i32,
    param: isize,
}

#[repr(C)]
struct NmTreeView {
    hdr: NmHdr,
    action: u32,
    old: TvItem,
    new: TvItem,
    pt: [i32; 2],
}

fn tree_node(s: &CommonState, h: usize) -> Option<&TreeNode> {
    s.tree.iter().find(|n| n.handle == h)
}

fn tree_children(s: &CommonState, parent: usize) -> Vec<usize> {
    s.tree
        .iter()
        .filter(|n| n.parent == parent)
        .map(|n| n.handle)
        .collect()
}

/// Éléments affichés (parents dépliés), avec leur profondeur.
fn visible_items(s: &CommonState) -> Vec<(usize, usize)> {
    fn walk(s: &CommonState, parent: usize, depth: usize, out: &mut Vec<(usize, usize)>) {
        for h in tree_children(s, parent) {
            out.push((h, depth));
            if tree_node(s, h).is_some_and(|n| n.state & TVIS_EXPANDED != 0) {
                walk(s, h, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(s, 0, 0, &mut out);
    out
}

fn has_children(s: &CommonState, h: usize) -> bool {
    s.tree.iter().any(|n| n.parent == h) || tree_node(s, h).is_some_and(|n| n.children_hint > 0)
}

fn tree_row_height(c: &Ctl) -> i32 {
    item_height(c).max(20)
}

fn paint_tree(hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    background(hwnd, p.hdc, 0, FIELD, p, r);
    if has_border(hwnd) {
        p.frame(r, CONTROL_BORDER);
    }
    let s = common_snapshot(hwnd);
    let rows = visible_items(&s);
    let ih = tree_row_height(c);
    let st = style(hwnd);
    let root_lines = st & TVS_LINESATROOT != 0;
    let indent = 18;
    let top = s.lv_top.max(0) as usize;
    for (k, &(h, depth)) in rows.iter().enumerate().skip(top) {
        let y = r.top + 2 + (k - top) as i32 * ih;
        if y > r.bottom {
            break;
        }
        let Some(n) = tree_node(&s, h) else {
            continue;
        };
        let level = depth as i32 + root_lines as i32;
        let x = r.left + 4 + level * indent;
        let row = Rect {
            left: r.left + 2,
            top: y,
            right: r.right - 2,
            bottom: y + ih,
        };
        let selected = h == s.selected_item;
        if selected {
            let bg = if c.focused {
                0x00F7_E6D9
            } else {
                SELECTION_SOFT
            };
            p.shape(row, bg, bg, 4, false);
            if c.focused {
                // Trait d'accent à gauche, comme dans l'Explorateur.
                p.shape(
                    Rect::from_size(row.left, y + 4, 3, ih - 8),
                    ACCENT,
                    ACCENT,
                    1,
                    false,
                );
            }
        }
        if (st & TVS_HASBUTTONS != 0 || st & TVS_HASLINES != 0) && has_children(&s, h) {
            // Chevron : vers la droite fermé, vers le bas ouvert.
            let cx = x - indent / 2;
            let cy = y + ih / 2;
            let col = 0x005D_5D5D;
            if n.state & TVIS_EXPANDED != 0 {
                p.line(cx - 4, cy - 2, cx, cy + 2, col, 1);
                p.line(cx, cy + 2, cx + 4, cy - 2, col, 1);
            } else {
                p.line(cx - 2, cy - 4, cx + 2, cy, col, 1);
                p.line(cx + 2, cy, cx - 2, cy + 4, col, 1);
            }
        }
        let tr = Rect {
            left: x + 2,
            right: r.right - 4,
            ..row
        };
        let color = if enabled(hwnd) { TEXT } else { TEXT_DISABLED };
        p.text(tr, &n.text, color, 0);
        if n.state & TVIS_BOLD != 0 {
            p.text(
                Rect {
                    left: tr.left + 1,
                    ..tr
                },
                &n.text,
                color,
                0,
            );
        }
    }
}

fn tree_notify_sel(
    hwnd: usize,
    code_w: i32,
    code_a: i32,
    old: usize,
    new: usize,
    action: u32,
) -> isize {
    let s = common_snapshot(hwnd);
    let item = |h: usize| {
        let n = tree_node(&s, h);
        TvItem {
            mask: TVIF_HANDLE | TVIF_PARAM | TVIF_STATE,
            item: h,
            state: n.map_or(0, |n| n.state),
            param: n.map_or(0, |n| n.param),
            ..TvItem::default()
        }
    };
    let mut nm = NmTreeView {
        hdr: NmHdr {
            hwnd_from: 0,
            id_from: 0,
            code: 0,
        },
        action,
        old: item(old),
        new: item(new),
        pt: [0, 0],
    };
    let code = if wide_for(hwnd) { code_w } else { code_a };
    notify_struct(hwnd, code, &mut nm)
}

/// Sélectionne un élément (et prévient le parent) ; vrai si accepté.
fn tree_select(hwnd: usize, h: usize, action: u32) -> bool {
    let old = common_snapshot(hwnd).selected_item;
    if old == h {
        return true;
    }
    if tree_notify_sel(hwnd, -450, -401, old, h, action) != 0 {
        return false; // TVN_SELCHANGING refusé
    }
    with_common(hwnd, |s| {
        for n in s.tree.iter_mut() {
            if n.handle == h {
                n.state |= TVIS_SELECTED;
            } else {
                n.state &= !TVIS_SELECTED;
            }
        }
        s.selected_item = h;
        // Les parents d'un élément sélectionné sont dépliés.
        let mut p = s
            .tree
            .iter()
            .find(|n| n.handle == h)
            .map_or(0, |n| n.parent);
        while p != 0 {
            let mut next = 0;
            if let Some(n) = s.tree.iter_mut().find(|n| n.handle == p) {
                n.state |= TVIS_EXPANDED;
                next = n.parent;
            }
            p = next;
        }
    });
    tree_ensure_visible(hwnd, h);
    tree_notify_sel(hwnd, -451, -402, old, h, action);
    invalidate(hwnd);
    true
}

fn tree_visible_rows(hwnd: usize) -> i32 {
    let c = snapshot(hwnd);
    let ih = c.as_ref().map_or(20, tree_row_height);
    ((client(hwnd).height() - 4) / ih).max(1)
}

fn tree_ensure_visible(hwnd: usize, h: usize) {
    let rows = tree_visible_rows(hwnd);
    with_common(hwnd, |s| {
        let vis = visible_items(s);
        if let Some(k) = vis.iter().position(|&(x, _)| x == h) {
            let k = k as i32;
            if k < s.lv_top {
                s.lv_top = k;
            } else if k >= s.lv_top + rows {
                s.lv_top = k - rows + 1;
            }
        }
    });
}

fn tree_expand(hwnd: usize, h: usize, how: u32) -> bool {
    let s = common_snapshot(hwnd);
    let Some(n) = tree_node(&s, h) else {
        return false;
    };
    let expanded = n.state & TVIS_EXPANDED != 0;
    let want = match how & 0x3 {
        1 => false,
        2 => true,
        3 => !expanded,
        _ => return false,
    };
    if want == expanded {
        return true;
    }
    let action = if want { 2 } else { 1 };
    if tree_notify_sel(hwnd, -454, -405, 0, h, action) != 0 {
        return false;
    }
    with_common(hwnd, |s| {
        if let Some(n) = s.tree.iter_mut().find(|n| n.handle == h) {
            if want {
                n.state |= TVIS_EXPANDED;
            } else {
                n.state &= !TVIS_EXPANDED;
            }
        }
        // Un élément sélectionné caché par le repli : sélection au parent.
        if !want {
            let mut p = s
                .tree
                .iter()
                .find(|n| n.handle == s.selected_item)
                .map_or(0, |n| n.parent);
            while p != 0 && p != h {
                p = s
                    .tree
                    .iter()
                    .find(|n| n.handle == p)
                    .map_or(0, |n| n.parent);
            }
            if p == h && s.selected_item != h {
                s.selected_item = h;
            }
        }
    });
    tree_notify_sel(hwnd, -455, -406, 0, h, action);
    invalidate(hwnd);
    true
}

unsafe fn tree_insert(hwnd: usize, lp: isize, wide: bool) -> isize {
    if lp == 0 {
        return 0;
    }
    let parent_h = *(lp as *const usize);
    let after = *((lp as *const usize).add(1));
    let item = std::ptr::read_unaligned((lp as *const u8).add(16) as *const TvItem);
    let parent_h = if parent_h == TVI_ROOT { 0 } else { parent_h };
    let text = if item.mask & TVIF_TEXT != 0 {
        read_text_ptr(item.text, wide).unwrap_or_default()
    } else {
        String::new()
    };
    with_common(hwnd, |s| {
        if parent_h != 0 && tree_node(s, parent_h).is_none() {
            return 0;
        }
        s.next_item += 1;
        let handle = s.next_item;
        let node = TreeNode {
            handle,
            parent: parent_h,
            text: text.clone(),
            param: if item.mask & TVIF_PARAM != 0 {
                item.param
            } else {
                0
            },
            state: if item.mask & TVIF_STATE != 0 {
                item.state & item.state_mask & !TVIS_SELECTED
            } else {
                0
            },
            children_hint: if item.mask & TVIF_CHILDREN != 0 {
                item.children
            } else {
                0
            },
        };
        let siblings: Vec<usize> = s
            .tree
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent == parent_h)
            .map(|(i, _)| i)
            .collect();
        let at = match after {
            TVI_FIRST => siblings.first().copied().unwrap_or(s.tree.len()),
            TVI_LAST | TVI_ROOT | 0 => siblings.last().map_or(s.tree.len(), |&i| i + 1),
            TVI_SORT => siblings
                .iter()
                .copied()
                .find(|&i| s.tree[i].text.to_lowercase() > text.to_lowercase())
                .unwrap_or_else(|| siblings.last().map_or(s.tree.len(), |&i| i + 1)),
            h => s.tree.iter().position(|n| n.handle == h).map_or_else(
                || siblings.last().map_or(s.tree.len(), |&i| i + 1),
                |i| i + 1,
            ),
        };
        s.tree.insert(at.min(s.tree.len()), node);
        handle as isize
    })
}

fn tree_delete(hwnd: usize, h: usize) {
    let doomed: Vec<usize> = {
        let s = common_snapshot(hwnd);
        if h == 0 || h == TVI_ROOT {
            s.tree.iter().map(|n| n.handle).collect()
        } else {
            let mut out = vec![h];
            let mut k = 0;
            while k < out.len() {
                out.extend(tree_children(&s, out[k]));
                k += 1;
            }
            out
        }
    };
    with_common(hwnd, |s| {
        s.tree.retain(|n| !doomed.contains(&n.handle));
        if doomed.contains(&s.selected_item) {
            s.selected_item = 0;
        }
        s.lv_top = 0;
    });
    invalidate(hwnd);
}

fn tree_next(hwnd: usize, flag: usize, from: usize) -> usize {
    let s = common_snapshot(hwnd);
    let vis = visible_items(&s);
    let parent_of = |h: usize| tree_node(&s, h).map_or(0, |n| n.parent);
    match flag {
        0 => tree_children(&s, 0).first().copied().unwrap_or(0),
        1 | 2 => {
            let sib = tree_children(&s, parent_of(from));
            let Some(i) = sib.iter().position(|&x| x == from) else {
                return 0;
            };
            if flag == 1 {
                sib.get(i + 1).copied().unwrap_or(0)
            } else if i > 0 {
                sib[i - 1]
            } else {
                0
            }
        }
        3 => parent_of(from),
        4 => tree_children(&s, if from == TVI_ROOT { 0 } else { from })
            .first()
            .copied()
            .unwrap_or(0),
        5 => vis.get(s.lv_top.max(0) as usize).map_or(0, |v| v.0),
        6 | 7 => {
            let Some(i) = vis.iter().position(|v| v.0 == from) else {
                return 0;
            };
            if flag == 6 {
                vis.get(i + 1).map_or(0, |v| v.0)
            } else if i > 0 {
                vis[i - 1].0
            } else {
                0
            }
        }
        9 => s.selected_item,
        0xA => vis.last().map_or(0, |v| v.0),
        _ => 0,
    }
}

unsafe fn tree_get_item(hwnd: usize, lp: isize, wide: bool) -> isize {
    if lp == 0 {
        return 0;
    }
    let it = &mut *(lp as *mut TvItem);
    let s = common_snapshot(hwnd);
    let Some(n) = tree_node(&s, it.item) else {
        return 0;
    };
    if it.mask & TVIF_TEXT != 0 {
        write_text_ptr(it.text, it.text_max, &n.text, wide);
    }
    if it.mask & TVIF_PARAM != 0 {
        it.param = n.param;
    }
    if it.mask & TVIF_STATE != 0 {
        it.state = n.state
            & if it.state_mask == 0 {
                u32::MAX
            } else {
                it.state_mask
            };
    }
    if it.mask & TVIF_CHILDREN != 0 {
        it.children = has_children(&s, n.handle) as i32;
    }
    1
}

unsafe fn tree_set_item(hwnd: usize, lp: isize, wide: bool) -> isize {
    if lp == 0 {
        return 0;
    }
    let it = std::ptr::read_unaligned(lp as *const TvItem);
    let text = if it.mask & TVIF_TEXT != 0 {
        read_text_ptr(it.text, wide)
    } else {
        None
    };
    let ok = with_common(hwnd, |s| {
        let Some(n) = s.tree.iter_mut().find(|n| n.handle == it.item) else {
            return 0;
        };
        if let Some(t) = text {
            n.text = t;
        }
        if it.mask & TVIF_PARAM != 0 {
            n.param = it.param;
        }
        if it.mask & TVIF_STATE != 0 {
            n.state = (n.state & !it.state_mask) | (it.state & it.state_mask);
        }
        if it.mask & TVIF_CHILDREN != 0 {
            n.children_hint = it.children;
        }
        1
    });
    invalidate(hwnd);
    ok
}

fn tree_hit(hwnd: usize, y: i32) -> usize {
    let c = snapshot(hwnd);
    let ih = c.as_ref().map_or(20, tree_row_height);
    let s = common_snapshot(hwnd);
    let vis = visible_items(&s);
    let k = s.lv_top + (y - 2) / ih;
    if y < 2 || k < 0 {
        return 0;
    }
    vis.get(k as usize).map_or(0, |v| v.0)
}

unsafe fn tree_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let wide = wide_for(hwnd);
    let r = match msg {
        0x1100 | 0x1132 => tree_insert(hwnd, lp, msg == 0x1132), // TVM_INSERTITEMA/W
        0x1101 => {
            tree_delete(hwnd, lp as usize);
            1
        }
        0x1102 => tree_expand(hwnd, lp as usize, wp as u32) as isize, // TVM_EXPAND
        0x1104 => {
            // TVM_GETITEMRECT : lp pointe sur le HTREEITEM, reçoit le RECT.
            if lp == 0 {
                return Some(0);
            }
            let h = *(lp as *const usize);
            let s = common_snapshot(hwnd);
            let vis = visible_items(&s);
            let Some(k) = vis.iter().position(|v| v.0 == h) else {
                return Some(0);
            };
            let c = snapshot(hwnd)?;
            let ih = tree_row_height(&c);
            let y = 2 + (k as i32 - s.lv_top) * ih;
            *(lp as *mut Rect) = Rect::from_size(0, y, client(hwnd).width(), ih);
            1
        }
        0x1105 => common_snapshot(hwnd).tree.len() as isize, // TVM_GETCOUNT
        0x1106 => 18,                                        // TVM_GETINDENT
        0x1107 | 0x1109 | 0x111D | 0x111E | 0x112C | 0x1121 => 0, // indentation, images, couleurs
        0x1108 => 0,                                         // TVM_GETIMAGELIST
        0x110A => tree_next(hwnd, wp, lp as usize) as isize, // TVM_GETNEXTITEM
        0x110B => {
            // TVM_SELECTITEM
            match wp {
                9 => (lp == 0 || tree_select(hwnd, lp as usize, 0)) as isize,
                5 => {
                    let k = visible_items(&common_snapshot(hwnd))
                        .iter()
                        .position(|v| v.0 == lp as usize);
                    if let Some(k) = k {
                        with_common(hwnd, |s| s.lv_top = k as i32);
                        invalidate(hwnd);
                    }
                    k.is_some() as isize
                }
                _ => 1,
            }
        }
        0x110C | 0x113E => tree_get_item(hwnd, lp, msg == 0x113E),
        0x110D | 0x113F => tree_set_item(hwnd, lp, msg == 0x113F),
        0x1110 => tree_visible_rows(hwnd) as isize, // TVM_GETVISIBLECOUNT
        0x1111 => {
            // TVM_HITTEST : TVHITTESTINFO { POINT, flags, hItem }
            if lp == 0 {
                return Some(0);
            }
            let y = *((lp as *const i32).add(1));
            let h = tree_hit(hwnd, y);
            *((lp as *mut u32).add(2)) = if h != 0 { 0x46 } else { 1 }; // TVHT_ONITEM / NOWHERE
            *((lp as *mut usize).add(2)) = h;
            h as isize
        }
        0x1113 => {
            // TVM_SORTCHILDREN
            let parent_h = if lp as usize == TVI_ROOT {
                0
            } else {
                lp as usize
            };
            with_common(hwnd, |s| {
                let mut kids: Vec<TreeNode> = s
                    .tree
                    .iter()
                    .filter(|n| n.parent == parent_h)
                    .cloned()
                    .collect();
                kids.sort_by_key(|n| n.text.to_lowercase());
                let mut it = kids.into_iter();
                for n in s.tree.iter_mut().filter(|n| n.parent == parent_h) {
                    if let Some(k) = it.next() {
                        *n = k;
                    }
                }
            });
            invalidate(hwnd);
            1
        }
        0x1114 => {
            tree_ensure_visible(hwnd, lp as usize);
            invalidate(hwnd);
            1
        }
        0x111B => {
            with_ctl(hwnd, |c| c.item_h = wp as i32);
            invalidate(hwnd);
            0
        }
        0x111C => snapshot(hwnd).map_or(20, |c| tree_row_height(&c)) as isize,
        0x1127 => {
            // TVM_GETITEMSTATE
            let s = common_snapshot(hwnd);
            tree_node(&s, wp).map_or(0, |n| (n.state & lp as u32) as isize)
        }
        0x110E | 0x110F => 0, // TVM_GETEDITCONTROL, TVM_EDITLABEL : pas d'édition
        WM_GETDLGCODE => DLGC_WANTARROWS | DLGC_WANTCHARS,
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            user32::set_focus(hwnd);
            let (x, y) = point(lp);
            let h = tree_hit(hwnd, y);
            if h != 0 {
                let s = common_snapshot(hwnd);
                let depth = visible_items(&s)
                    .iter()
                    .find(|v| v.0 == h)
                    .map_or(0, |v| v.1) as i32;
                let level = depth + (style(hwnd) & TVS_LINESATROOT != 0) as i32;
                let button_x = 4 + level * 18;
                if x < button_x && x >= button_x - 18 && has_children(&s, h) {
                    tree_expand(hwnd, h, 3);
                } else {
                    tree_select(hwnd, h, 1); // TVC_BYMOUSE
                    if msg == WM_LBUTTONDBLCLK && notify_simple(hwnd, NM_DBLCLK) == 0 {
                        tree_expand(hwnd, h, 3);
                    }
                }
            }
            if msg == WM_LBUTTONDOWN {
                notify_simple(hwnd, NM_CLICK);
            }
            0
        }
        WM_KEYDOWN => {
            #[repr(C)]
            struct NmTvKeyDown {
                hdr: NmHdr,
                vkey: u16,
                flags: u32,
            }
            let mut nm = NmTvKeyDown {
                hdr: NmHdr {
                    hwnd_from: 0,
                    id_from: 0,
                    code: 0,
                },
                vkey: wp as u16,
                flags: 0,
            };
            notify_struct(hwnd, -412, &mut nm); // TVN_KEYDOWN
            let s = common_snapshot(hwnd);
            let cur = s.selected_item;
            let vis = visible_items(&s);
            let pos = vis.iter().position(|v| v.0 == cur);
            let target = match wp {
                VK_UP => pos.and_then(|i| i.checked_sub(1)).map(|i| vis[i].0),
                VK_DOWN => match pos {
                    Some(i) => vis.get(i + 1).map(|v| v.0),
                    None => vis.first().map(|v| v.0),
                },
                VK_HOME => vis.first().map(|v| v.0),
                VK_END => vis.last().map(|v| v.0),
                VK_RIGHT => {
                    if has_children(&s, cur)
                        && tree_node(&s, cur).is_some_and(|n| n.state & TVIS_EXPANDED == 0)
                    {
                        tree_expand(hwnd, cur, 2);
                        None
                    } else {
                        tree_children(&s, cur).first().copied()
                    }
                }
                VK_LEFT => {
                    if tree_node(&s, cur).is_some_and(|n| n.state & TVIS_EXPANDED != 0) {
                        tree_expand(hwnd, cur, 1);
                        None
                    } else {
                        tree_node(&s, cur).map(|n| n.parent).filter(|&p| p != 0)
                    }
                }
                VK_RETURN => {
                    notify_simple(hwnd, NM_RETURN);
                    None
                }
                _ => None,
            };
            if let Some(t) = target {
                tree_select(hwnd, t, 2); // TVC_BYKEYBOARD
            }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = (wp >> 16) as u16 as i16 as i32;
            let rows = tree_visible_rows(hwnd);
            with_common(hwnd, |s| {
                let n = visible_items(s).len() as i32;
                s.lv_top = (s.lv_top - delta / 120 * 3).clamp(0, (n - rows).max(0));
            });
            invalidate(hwnd);
            0
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            with_ctl(hwnd, |c| c.focused = msg == WM_SETFOCUS);
            invalidate(hwnd);
            notify_simple(
                hwnd,
                if msg == WM_SETFOCUS {
                    NM_SETFOCUS
                } else {
                    NM_KILLFOCUS
                },
            );
            0
        }
        _ => return None,
    };
    let _ = wide;
    Some(r)
}

// =====================================================================
// SysListView32
// =====================================================================

const LVIF_TEXT: u32 = 0x1;
const LVIF_PARAM: u32 = 0x4;
const LVIF_STATE: u32 = 0x8;
const LVIS_FOCUSED: u32 = 0x1;
const LVIS_SELECTED: u32 = 0x2;
const LVS_REPORT: u32 = 0x1;
const LVS_SINGLESEL: u32 = 0x4;
const LVS_NOCOLUMNHEADER: u32 = 0x4000;
const LVS_OWNERDATA: u32 = 0x1000;
const LPSTR_TEXTCALLBACK: usize = usize::MAX;
const LVN_GETDISPINFOA: i32 = -150;
const LVN_GETDISPINFOW: i32 = -177;
/// Nombre de lignes maximal d'une liste virtuelle tenu en mémoire.
const LV_VIRTUAL_MAX: usize = 1 << 20;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LvItem {
    mask: u32,
    item: i32,
    sub_item: i32,
    state: u32,
    state_mask: u32,
    text: usize,
    text_max: i32,
    image: i32,
    param: isize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LvColumn {
    mask: u32,
    fmt: i32,
    cx: i32,
    text: usize,
    text_max: i32,
    sub_item: i32,
}

#[repr(C)]
struct NmListView {
    hdr: NmHdr,
    item: i32,
    sub_item: i32,
    new_state: u32,
    old_state: u32,
    changed: u32,
    pt: [i32; 2],
    param: isize,
}

fn report(hwnd: usize) -> bool {
    style(hwnd) & 0x3 == LVS_REPORT
}

fn callback_bit(sub: usize) -> u64 {
    if sub < 64 {
        1 << sub
    } else {
        0
    }
}

/// Texte d'une cellule. Quand le programme a donné LPSTR_TEXTCALLBACK, ou
/// pour une liste virtuelle (LVS_OWNERDATA), le texte est demandé au parent
/// par `LVN_GETDISPINFO`, comme le fait comctl32.
fn lv_cell_text(hwnd: usize, item: usize, sub: usize, row: &LvRow) -> String {
    if row.callback & callback_bit(sub) == 0 {
        return row.texts.get(sub).cloned().unwrap_or_default();
    }
    lv_dispinfo_text(hwnd, item, sub, row.param)
}

fn lv_dispinfo_text(hwnd: usize, item: usize, sub: usize, param: isize) -> String {
    #[repr(C)]
    struct DispInfo {
        hdr: NmHdr,
        item: LvItem,
        // iIndent, iGroupId, cColumns, puColumns, piColFmt, iGroup
        tail: [u8; 48],
    }
    let wide = snapshot(hwnd).is_none_or(|c| c.wide);
    let mut buf = [0u16; 520];
    let mut nm = DispInfo {
        hdr: NmHdr {
            hwnd_from: 0,
            id_from: 0,
            code: 0,
        },
        item: LvItem {
            mask: LVIF_TEXT,
            item: item as i32,
            sub_item: sub as i32,
            text: buf.as_mut_ptr() as usize,
            text_max: if wide { 260 } else { 520 },
            param,
            ..Default::default()
        },
        tail: [0; 48],
    };
    let code = if wide {
        LVN_GETDISPINFOW
    } else {
        LVN_GETDISPINFOA
    };
    notify_struct(hwnd, code, &mut nm);
    if nm.item.mask & LVIF_TEXT == 0 {
        return String::new();
    }
    // Le programme peut remplir notre tampon ou pointer vers le sien.
    unsafe { read_text_ptr(nm.item.text, wide) }.unwrap_or_default()
}

fn header_h(hwnd: usize, c: &Ctl) -> i32 {
    if report(hwnd) && style(hwnd) & LVS_NOCOLUMNHEADER == 0 {
        line_height(font_of(c)) + 10
    } else {
        0
    }
}

fn lv_row_h(c: &Ctl) -> i32 {
    item_height(c).max(22)
}

fn paint_listview(hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    background(hwnd, p.hdc, 0, FIELD, p, r);
    if has_border(hwnd) {
        p.frame(r, CONTROL_BORDER);
    }
    let s = common_snapshot(hwnd);
    let hh = header_h(hwnd, c);
    let rh = lv_row_h(c);
    let columns: Vec<(String, i32)> = if report(hwnd) && !s.columns.is_empty() {
        s.columns.clone()
    } else {
        vec![(String::new(), r.width())]
    };
    if hh > 0 {
        let mut x = r.left;
        for (title, w) in &columns {
            let cell = Rect::from_size(x, r.top, *w, hh);
            p.text(
                Rect {
                    left: x + 10,
                    right: x + w - 6,
                    ..cell
                },
                title,
                0x005D_5D5D,
                0,
            );
            p.line(
                x + w - 1,
                r.top + 6,
                x + w - 1,
                r.top + hh - 6,
                CONTROL_BORDER,
                1,
            );
            x += w;
        }
        p.line(
            r.left,
            r.top + hh - 1,
            r.right,
            r.top + hh - 1,
            CONTROL_BORDER,
            1,
        );
    }
    let top = s.lv_top.max(0) as usize;
    for (k, row) in s.rows.iter().enumerate().skip(top) {
        let y = r.top + hh + 2 + (k - top) as i32 * rh;
        if y > r.bottom {
            break;
        }
        let line = Rect {
            left: r.left + 2,
            top: y,
            right: r.right - 2,
            bottom: y + rh,
        };
        if row.state & LVIS_SELECTED != 0 {
            let bg = if c.focused {
                0x00F7_E6D9
            } else {
                SELECTION_SOFT
            };
            p.shape(line, bg, bg, 4, false);
        }
        let mut x = r.left;
        for (i, (_, w)) in columns.iter().enumerate() {
            let text = lv_cell_text(hwnd, k, i, row);
            let text = text.as_str();
            let cell = Rect {
                left: x + 10,
                right: x + w - 6,
                ..line
            };
            p.text(
                cell,
                text,
                if enabled(hwnd) { TEXT } else { TEXT_DISABLED },
                0,
            );
            x += w;
        }
    }
}

fn lv_notify_change(hwnd: usize, item: i32, old: u32, new: u32) {
    let param = common_snapshot(hwnd)
        .rows
        .get(item as usize)
        .map_or(0, |r| r.param);
    let mut nm = NmListView {
        hdr: NmHdr {
            hwnd_from: 0,
            id_from: 0,
            code: 0,
        },
        item,
        sub_item: 0,
        new_state: new,
        old_state: old,
        changed: LVIF_STATE,
        pt: [0, 0],
        param,
    };
    notify_struct(hwnd, -101, &mut nm); // LVN_ITEMCHANGED
}

/// Change l'état (sélection, focus) d'une ligne ou de toutes (-1).
fn lv_set_state(hwnd: usize, item: i32, state: u32, mask: u32) {
    let changes: Vec<(i32, u32, u32)> = with_common(hwnd, |s| {
        let mut out = Vec::new();
        let n = s.rows.len() as i32;
        for i in 0..n {
            if item >= 0 && i != item {
                continue;
            }
            let row = &mut s.rows[i as usize];
            let old = row.state;
            row.state = (old & !mask) | (state & mask);
            if row.state != old {
                out.push((i, old, row.state));
            }
            if state & mask & LVIS_FOCUSED != 0 {
                s.lv_focus = i;
            }
        }
        out
    });
    for (i, old, new) in changes {
        lv_notify_change(hwnd, i, old, new);
    }
    invalidate(hwnd);
}

fn lv_select_only(hwnd: usize, item: i32) {
    let others: Vec<i32> = common_snapshot(hwnd)
        .rows
        .iter()
        .enumerate()
        .filter(|(i, r)| *i as i32 != item && r.state & (LVIS_SELECTED | LVIS_FOCUSED) != 0)
        .map(|(i, _)| i as i32)
        .collect();
    for i in others {
        lv_set_state(hwnd, i, 0, LVIS_SELECTED | LVIS_FOCUSED);
    }
    lv_set_state(
        hwnd,
        item,
        LVIS_SELECTED | LVIS_FOCUSED,
        LVIS_SELECTED | LVIS_FOCUSED,
    );
    lv_ensure_visible(hwnd, item);
}

fn lv_rows_visible(hwnd: usize) -> i32 {
    let Some(c) = snapshot(hwnd) else {
        return 1;
    };
    ((client(hwnd).height() - header_h(hwnd, &c) - 2) / lv_row_h(&c)).max(1)
}

fn lv_ensure_visible(hwnd: usize, item: i32) {
    let rows = lv_rows_visible(hwnd);
    with_common(hwnd, |s| {
        if item < s.lv_top {
            s.lv_top = item.max(0);
        } else if item >= s.lv_top + rows {
            s.lv_top = item - rows + 1;
        }
    });
}

unsafe fn lv_set_item(hwnd: usize, lp: isize, wide: bool, insert: bool) -> isize {
    if lp == 0 {
        return -1;
    }
    let it = std::ptr::read_unaligned(lp as *const LvItem);
    let text = if it.mask & LVIF_TEXT != 0 {
        read_text_ptr(it.text, wide)
    } else {
        None
    };
    let is_callback = it.mask & LVIF_TEXT != 0 && it.text == LPSTR_TEXTCALLBACK;
    let r = with_common(hwnd, |s| {
        if insert {
            let at = (it.item.max(0) as usize).min(s.rows.len());
            let mut row = LvRow::default();
            row.texts.push(text.clone().unwrap_or_default());
            if is_callback {
                row.callback = 1;
            }
            if it.mask & LVIF_PARAM != 0 {
                row.param = it.param;
            }
            if it.mask & LVIF_STATE != 0 {
                row.state = it.state & it.state_mask;
            }
            s.rows.insert(at, row);
            if s.lv_focus >= at as i32 {
                s.lv_focus += 1;
            }
            return at as isize;
        }
        let Some(row) = s
            .rows
            .get_mut(it.item.max(0) as usize)
            .filter(|_| it.item >= 0)
        else {
            return -1;
        };
        let k = it.sub_item.max(0) as usize;
        if is_callback {
            row.callback |= callback_bit(k);
        } else if let Some(t) = text.clone() {
            if row.texts.len() <= k {
                row.texts.resize(k + 1, String::new());
            }
            row.texts[k] = t;
            row.callback &= !callback_bit(k);
        }
        if it.mask & LVIF_PARAM != 0 && it.sub_item == 0 {
            row.param = it.param;
        }
        1
    });
    if !insert && it.mask & LVIF_STATE != 0 && r >= 0 {
        lv_set_state(hwnd, it.item, it.state, it.state_mask);
    }
    invalidate(hwnd);
    r
}

unsafe fn lv_get_item(hwnd: usize, lp: isize, wide: bool, text_only: Option<usize>) -> isize {
    if lp == 0 {
        return 0;
    }
    let it = &mut *(lp as *mut LvItem);
    let s = common_snapshot(hwnd);
    let idx = text_only.unwrap_or(it.item.max(0) as usize);
    let Some(row) = s.rows.get(idx) else {
        return 0;
    };
    let wants_text = text_only.is_some() || it.mask & LVIF_TEXT != 0;
    let text = if wants_text {
        lv_cell_text(hwnd, idx, it.sub_item.max(0) as usize, row)
    } else {
        String::new()
    };
    if wants_text {
        write_text_ptr(it.text, it.text_max, &text, wide);
        if text_only.is_some() {
            return text.chars().count() as isize;
        }
    }
    if it.mask & LVIF_PARAM != 0 {
        it.param = row.param;
    }
    if it.mask & LVIF_STATE != 0 {
        it.state = row.state & it.state_mask;
    }
    1
}

unsafe fn lv_column(hwnd: usize, wp: usize, lp: isize, wide: bool, insert: bool) -> isize {
    if lp == 0 {
        return -1;
    }
    let col = std::ptr::read_unaligned(lp as *const LvColumn);
    let title = if col.mask & 0x4 != 0 {
        read_text_ptr(col.text, wide).unwrap_or_default()
    } else {
        String::new()
    };
    let width = if col.mask & 0x2 != 0 { col.cx } else { 100 };
    let r = with_common(hwnd, |s| {
        if insert {
            let at = wp.min(s.columns.len());
            s.columns.insert(at, (title.clone(), width));
            for row in s.rows.iter_mut() {
                if at > 0 && row.texts.len() >= at {
                    row.texts.insert(at, String::new());
                }
            }
            at as isize
        } else if let Some(c) = s.columns.get_mut(wp) {
            if col.mask & 0x4 != 0 {
                c.0 = title.clone();
            }
            if col.mask & 0x2 != 0 {
                c.1 = width;
            }
            1
        } else {
            0
        }
    });
    invalidate(hwnd);
    r
}

fn lv_hit(hwnd: usize, y: i32) -> i32 {
    let Some(c) = snapshot(hwnd) else {
        return -1;
    };
    let s = common_snapshot(hwnd);
    let y = y - header_h(hwnd, &c) - 2;
    if y < 0 {
        return -1;
    }
    let i = s.lv_top + y / lv_row_h(&c);
    if (i as usize) < s.rows.len() {
        i
    } else {
        -1
    }
}

unsafe fn listview_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let r = match msg {
        0x1004 => common_snapshot(hwnd).rows.len() as isize, // LVM_GETITEMCOUNT
        0x1007 | 0x104D => lv_set_item(hwnd, lp, msg == 0x104D, true), // INSERTITEM
        0x1006 | 0x104C => lv_set_item(hwnd, lp, msg == 0x104C, false), // SETITEM
        0x102E | 0x1074 => {
            // SETITEMTEXT : wp = ligne
            if lp != 0 {
                let mut it = std::ptr::read_unaligned(lp as *const LvItem);
                it.item = wp as i32;
                it.mask = LVIF_TEXT;
                lv_set_item(hwnd, &it as *const LvItem as isize, msg == 0x1074, false)
            } else {
                0
            }
        }
        0x1005 | 0x104B => lv_get_item(hwnd, lp, msg == 0x104B, None), // GETITEM
        0x102D | 0x1073 => lv_get_item(hwnd, lp, msg == 0x1073, Some(wp)), // GETITEMTEXT
        0x1008 => {
            // DELETEITEM
            let ok = with_common(hwnd, |s| {
                if wp < s.rows.len() {
                    s.rows.remove(wp);
                    true
                } else {
                    false
                }
            });
            invalidate(hwnd);
            ok as isize
        }
        0x1009 => {
            with_common(hwnd, |s| {
                s.rows.clear();
                s.lv_top = 0;
                s.lv_focus = -1;
            });
            invalidate(hwnd);
            1
        }
        0x101B | 0x1061 => lv_column(hwnd, wp, lp, msg == 0x1061, true), // INSERTCOLUMN
        0x101A | 0x1060 => lv_column(hwnd, wp, lp, msg == 0x1060, false), // SETCOLUMN
        0x101C => {
            with_common(hwnd, |s| {
                if wp < s.columns.len() {
                    s.columns.remove(wp);
                    for row in s.rows.iter_mut() {
                        if wp < row.texts.len() {
                            row.texts.remove(wp);
                        }
                    }
                }
            });
            invalidate(hwnd);
            1
        }
        0x101D => common_snapshot(hwnd)
            .columns
            .get(wp)
            .map_or(0, |c| c.1 as isize), // GETCOLUMNWIDTH
        0x101E => {
            // SETCOLUMNWIDTH (-1 / -2 : ajusté au contenu)
            let w = if (lp as i32) < 0 { 120 } else { lp as i32 };
            with_common(hwnd, |s| {
                if let Some(c) = s.columns.get_mut(wp) {
                    c.1 = w;
                }
            });
            invalidate(hwnd);
            1
        }
        0x100C => {
            // GETNEXTITEM : wp = départ (-1), lp = LVNI_*
            let s = common_snapshot(hwnd);
            let start = wp as isize as i32;
            let flags = lp as u32;
            let mut found = -1;
            for (i, row) in s.rows.iter().enumerate() {
                if (i as i32) <= start {
                    continue;
                }
                let ok = (flags & 0x2 == 0 || row.state & LVIS_SELECTED != 0)
                    && (flags & 0x1 == 0 || row.state & LVIS_FOCUSED != 0);
                if ok {
                    found = i as i32;
                    break;
                }
            }
            found as isize
        }
        0x102B => {
            // SETITEMSTATE
            if lp != 0 {
                let it = std::ptr::read_unaligned(lp as *const LvItem);
                lv_set_state(hwnd, wp as isize as i32, it.state, it.state_mask);
            }
            1
        }
        0x102C => common_snapshot(hwnd)
            .rows
            .get(wp)
            .map_or(0, |r| (r.state & lp as u32) as isize), // GETITEMSTATE
        0x1032 => common_snapshot(hwnd)
            .rows
            .iter()
            .filter(|r| r.state & LVIS_SELECTED != 0)
            .count() as isize, // GETSELECTEDCOUNT
        0x1013 => {
            lv_ensure_visible(hwnd, wp as i32);
            invalidate(hwnd);
            1
        }
        0x1012 | 0x1039 => {
            // HITTEST / SUBITEMHITTEST : LVHITTESTINFO { POINT, flags, iItem, iSubItem }
            if lp == 0 {
                return Some(-1);
            }
            let y = *((lp as *const i32).add(1));
            let i = lv_hit(hwnd, y);
            *((lp as *mut u32).add(2)) = if i >= 0 { 0x6 } else { 1 };
            *((lp as *mut i32).add(3)) = i;
            if msg == 0x1039 {
                *((lp as *mut i32).add(4)) = 0;
            }
            i as isize
        }
        0x1030 | 0x1051 => {
            // SORTITEMS / SORTITEMSEX : comparaison du programme
            let cmp_fn = lp as usize;
            if cmp_fn != 0 {
                let cmp: unsafe extern "win64" fn(isize, isize, isize) -> i32 =
                    std::mem::transmute(cmp_fn);
                let mut rows = common_snapshot(hwnd).rows;
                let ex = msg == 0x1051;
                let idx: Vec<usize> = (0..rows.len()).collect();
                let mut order = idx.clone();
                order.sort_by(|&a, &b| {
                    let (x, y) = if ex {
                        (a as isize, b as isize)
                    } else {
                        (rows[a].param, rows[b].param)
                    };
                    cmp(x, y, wp as isize).cmp(&0)
                });
                rows = order.into_iter().map(|i| rows[i].clone()).collect();
                with_common(hwnd, |s| s.rows = rows);
                invalidate(hwnd);
            }
            1
        }
        0x1053 | 0x100D => {
            // FINDITEMW/A : LVFINDINFO { flags, psz, lParam… }
            if lp == 0 {
                return Some(-1);
            }
            let flags = *(lp as *const u32);
            let text_ptr = *((lp as *const usize).add(1));
            let param = *((lp as *const isize).add(2));
            let needle = read_text_ptr(text_ptr, msg == 0x1053)
                .unwrap_or_default()
                .to_lowercase();
            let s = common_snapshot(hwnd);
            s.rows
                .iter()
                .enumerate()
                .skip((wp as isize + 1).max(0) as usize)
                .find(|(_, r)| {
                    if flags & 0x1 != 0 {
                        r.param == param
                    } else {
                        let t = r
                            .texts
                            .first()
                            .map(|t| t.to_lowercase())
                            .unwrap_or_default();
                        if flags & 0xA != 0 {
                            t.starts_with(&needle)
                        } else {
                            t == needle
                        }
                    }
                })
                .map_or(-1, |(i, _)| i as isize)
        }
        0x102F => {
            // SETITEMCOUNT : nombre de lignes d'une liste virtuelle, simple
            // réservation sinon.
            if style(hwnd) & LVS_OWNERDATA != 0 {
                let n = wp.min(LV_VIRTUAL_MAX);
                with_common(hwnd, |s| {
                    s.rows.resize_with(n, || LvRow {
                        callback: u64::MAX,
                        ..Default::default()
                    });
                    let max_top = (n as i32 - 1).max(0);
                    s.lv_top = s.lv_top.clamp(0, max_top);
                    if s.lv_focus >= n as i32 {
                        s.lv_focus = -1;
                    }
                });
                invalidate(hwnd);
            }
            1
        }
        0x1036 | 0x1037 | 0x1003 | 0x1002 | 0x1001 | 0x1024 | 0x1026 | 0x1015 => 0,
        0x101F => 0, // GETHEADER : pas de contrôle séparé
        0x1027 => common_snapshot(hwnd).lv_top as isize, // GETTOPINDEX
        0x1028 => lv_rows_visible(hwnd) as isize, // GETCOUNTPERPAGE
        WM_GETDLGCODE => DLGC_WANTARROWS | DLGC_WANTCHARS,
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            user32::set_focus(hwnd);
            let (_, y) = point(lp);
            let i = lv_hit(hwnd, y);
            let c = snapshot(hwnd)?;
            if y < header_h(hwnd, &c) {
                // Clic sur un en-tête : LVN_COLUMNCLICK
                let (x, _) = point(lp);
                let s = common_snapshot(hwnd);
                let mut acc = 0;
                let mut col = 0;
                for (k, (_, w)) in s.columns.iter().enumerate() {
                    acc += w;
                    if x < acc {
                        col = k;
                        break;
                    }
                }
                let mut nm = NmListView {
                    hdr: NmHdr {
                        hwnd_from: 0,
                        id_from: 0,
                        code: 0,
                    },
                    item: -1,
                    sub_item: col as i32,
                    new_state: 0,
                    old_state: 0,
                    changed: 0,
                    pt: [0, 0],
                    param: 0,
                };
                notify_struct(hwnd, -108, &mut nm);
                return Some(0);
            }
            if i >= 0 {
                let multi = style(hwnd) & LVS_SINGLESEL == 0 && ctrl_down();
                if multi {
                    let sel = common_snapshot(hwnd).rows[i as usize].state & LVIS_SELECTED;
                    lv_set_state(
                        hwnd,
                        i,
                        sel ^ LVIS_SELECTED | LVIS_FOCUSED,
                        LVIS_SELECTED | LVIS_FOCUSED,
                    );
                } else {
                    lv_select_only(hwnd, i);
                }
            } else {
                lv_set_state(hwnd, -1, 0, LVIS_SELECTED);
            }
            notify_simple(
                hwnd,
                if msg == WM_LBUTTONDBLCLK {
                    NM_DBLCLK
                } else {
                    NM_CLICK
                },
            );
            if msg == WM_LBUTTONDBLCLK && i >= 0 {
                notify_simple(hwnd, -114); // LVN_ITEMACTIVATE
            }
            0
        }
        WM_KEYDOWN => {
            let s = common_snapshot(hwnd);
            let n = s.rows.len() as i32;
            if n == 0 {
                return Some(0);
            }
            let cur = s.lv_focus.max(0);
            let rows = lv_rows_visible(hwnd);
            let target = match wp {
                VK_UP => Some(cur - 1),
                VK_DOWN => Some(if s.lv_focus < 0 { 0 } else { cur + 1 }),
                VK_HOME => Some(0),
                VK_END => Some(n - 1),
                VK_PRIOR => Some(cur - rows + 1),
                VK_NEXT => Some(cur + rows - 1),
                VK_RETURN => {
                    notify_simple(hwnd, -114);
                    None
                }
                _ => None,
            };
            if let Some(t) = target {
                lv_select_only(hwnd, t.clamp(0, n - 1));
            }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = (wp >> 16) as u16 as i16 as i32;
            let rows = lv_rows_visible(hwnd);
            with_common(hwnd, |s| {
                let n = s.rows.len() as i32;
                s.lv_top = (s.lv_top - delta / 120 * 3).clamp(0, (n - rows).max(0));
            });
            invalidate(hwnd);
            0
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            with_ctl(hwnd, |c| c.focused = msg == WM_SETFOCUS);
            invalidate(hwnd);
            notify_simple(
                hwnd,
                if msg == WM_SETFOCUS {
                    NM_SETFOCUS
                } else {
                    NM_KILLFOCUS
                },
            );
            0
        }
        _ => return None,
    };
    Some(r)
}

// =====================================================================
// SysTabControl32
// =====================================================================

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TcItem {
    mask: u32,
    state: u32,
    state_mask: u32,
    text: usize,
    text_max: i32,
    image: i32,
    param: isize,
}

fn tab_h(c: &Ctl) -> i32 {
    line_height(font_of(c)) + 14
}

/// Rectangles des onglets (une seule rangée).
fn tab_rects(hwnd: usize, c: &Ctl) -> Vec<Rect> {
    let s = common_snapshot(hwnd);
    let h = tab_h(c);
    let mut x = 2;
    s.tabs
        .iter()
        .map(|(t, _)| {
            let w = measure(font_of(c), t).0 + 24;
            let r = Rect::from_size(x, 0, w, h);
            x += w + 2;
            r
        })
        .collect()
}

fn paint_tabs(hwnd: usize, p: &Painter, r: Rect) {
    let Some(c) = snapshot(hwnd) else {
        return;
    };
    background(hwnd, p.hdc, WM_CTLCOLORSTATIC, face_color(), p, r);
    let s = common_snapshot(hwnd);
    let h = tab_h(&c);
    let body = Rect {
        top: r.top + h,
        ..r
    };
    p.shape(body, FIELD, CONTROL_BORDER, 4, false);
    for (i, tr) in tab_rects(hwnd, &c).into_iter().enumerate() {
        let selected = i as i32 == s.tab_sel;
        if selected {
            p.shape(
                Rect {
                    bottom: tr.bottom + 4,
                    ..tr
                },
                FIELD,
                CONTROL_BORDER,
                4,
                false,
            );
            p.fill(
                Rect::from_size(tr.left + 1, tr.bottom - 1, tr.width() - 2, 4),
                FIELD,
            );
            p.line(tr.left + 8, tr.top + 2, tr.right - 8, tr.top + 2, ACCENT, 2);
        }
        let color = if enabled(hwnd) { TEXT } else { TEXT_DISABLED };
        p.text(tr, &s.tabs[i].0, color, 1);
    }
}

fn tab_select(hwnd: usize, i: i32, from_user: bool) -> isize {
    let old = common_snapshot(hwnd).tab_sel;
    let n = common_snapshot(hwnd).tabs.len() as i32;
    if i < 0 || i >= n {
        return -1;
    }
    if i == old {
        return old as isize;
    }
    if from_user && notify_simple(hwnd, -552) != 0 {
        return old as isize; // TCN_SELCHANGING refusé
    }
    with_common(hwnd, |s| s.tab_sel = i);
    invalidate(hwnd);
    if from_user {
        notify_simple(hwnd, -551); // TCN_SELCHANGE
    }
    old as isize
}

unsafe fn tab_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let r = match msg {
        0x1307 | 0x133E => {
            // TCM_INSERTITEMA/W
            if lp == 0 {
                return Some(-1);
            }
            let it = std::ptr::read_unaligned(lp as *const TcItem);
            let text = if it.mask & 0x1 != 0 {
                read_text_ptr(it.text, msg == 0x133E).unwrap_or_default()
            } else {
                String::new()
            };
            let at = with_common(hwnd, |s| {
                let at = wp.min(s.tabs.len());
                s.tabs
                    .insert(at, (text, if it.mask & 0x8 != 0 { it.param } else { 0 }));
                if s.tab_sel < 0 {
                    s.tab_sel = 0;
                } else if s.tab_sel >= at as i32 && s.tabs.len() > 1 {
                    s.tab_sel += 1;
                }
                at as isize
            });
            invalidate(hwnd);
            at
        }
        0x1306 | 0x133D => {
            // TCM_SETITEMA/W
            if lp == 0 {
                return Some(0);
            }
            let it = std::ptr::read_unaligned(lp as *const TcItem);
            let text = read_text_ptr(it.text, msg == 0x133D);
            let ok = with_common(hwnd, |s| match s.tabs.get_mut(wp) {
                Some(t) => {
                    if it.mask & 0x1 != 0 {
                        if let Some(x) = text {
                            t.0 = x;
                        }
                    }
                    if it.mask & 0x8 != 0 {
                        t.1 = it.param;
                    }
                    1
                }
                None => 0,
            });
            invalidate(hwnd);
            ok
        }
        0x1305 | 0x133C => {
            // TCM_GETITEMA/W
            if lp == 0 {
                return Some(0);
            }
            let it = &mut *(lp as *mut TcItem);
            match common_snapshot(hwnd).tabs.get(wp) {
                Some((t, param)) => {
                    if it.mask & 0x1 != 0 {
                        write_text_ptr(it.text, it.text_max, t, msg == 0x133C);
                    }
                    if it.mask & 0x8 != 0 {
                        it.param = *param;
                    }
                    1
                }
                None => 0,
            }
        }
        0x1304 => common_snapshot(hwnd).tabs.len() as isize, // GETITEMCOUNT
        0x1308 => {
            let ok = with_common(hwnd, |s| {
                if wp < s.tabs.len() {
                    s.tabs.remove(wp);
                    s.tab_sel = s.tab_sel.min(s.tabs.len() as i32 - 1);
                    1
                } else {
                    0
                }
            });
            invalidate(hwnd);
            ok
        }
        0x1309 => {
            with_common(hwnd, |s| {
                s.tabs.clear();
                s.tab_sel = -1;
            });
            invalidate(hwnd);
            1
        }
        0x130B => common_snapshot(hwnd).tab_sel as isize, // GETCURSEL
        0x130C => tab_select(hwnd, wp as i32, false),     // SETCURSEL
        0x130A => {
            // GETITEMRECT
            let c = snapshot(hwnd)?;
            match tab_rects(hwnd, &c).get(wp) {
                Some(r) if lp != 0 => {
                    *(lp as *mut Rect) = *r;
                    1
                }
                _ => 0,
            }
        }
        0x1328 => {
            // TCM_ADJUSTRECT : wp TRUE = zone d'affichage → fenêtre
            if lp != 0 {
                let c = snapshot(hwnd)?;
                let h = tab_h(&c);
                let r = &mut *(lp as *mut Rect);
                if wp != 0 {
                    r.top -= h + 2;
                    r.left -= 2;
                    r.right += 2;
                    r.bottom += 2;
                } else {
                    r.top += h + 2;
                    r.left += 2;
                    r.right -= 2;
                    r.bottom -= 2;
                }
            }
            0
        }
        0x1303 | 0x1329 | 0x132B | 0x1331 => 0, // images, taille, padding
        WM_GETDLGCODE => DLGC_WANTARROWS,
        WM_LBUTTONDOWN => {
            let c = snapshot(hwnd)?;
            let (x, y) = point(lp);
            if let Some(i) = tab_rects(hwnd, &c)
                .iter()
                .position(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
            {
                user32::set_focus(hwnd);
                tab_select(hwnd, i as i32, true);
            }
            notify_simple(hwnd, NM_CLICK);
            0
        }
        WM_KEYDOWN => {
            let s = common_snapshot(hwnd);
            match wp {
                VK_LEFT => {
                    tab_select(hwnd, s.tab_sel - 1, true);
                }
                VK_RIGHT => {
                    tab_select(hwnd, s.tab_sel + 1, true);
                }
                _ => {}
            }
            0
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            with_ctl(hwnd, |c| c.focused = msg == WM_SETFOCUS);
            invalidate(hwnd);
            0
        }
        _ => return None,
    };
    Some(r)
}

// =====================================================================
// msctls_statusbar32
// =====================================================================

const STATUS_H: i32 = 26;

/// Une barre d'état se place seule en bas de la zone client de son parent.
fn place_status_bar(hwnd: usize) {
    let par = parent(hwnd);
    let Some(pr) = gui::window_client_rect(par) else {
        return;
    };
    if style(hwnd) & 0x1 != 0 {
        return; // CCS_TOP : laissé à sa place
    }
    unsafe {
        user32::set_window_pos(
            hwnd,
            0,
            0,
            pr.height() - STATUS_H,
            pr.width(),
            STATUS_H,
            0x14,
        );
    }
}

fn paint_status(hwnd: usize, p: &Painter, r: Rect) {
    p.fill(r, 0x00F3_F3F3);
    p.line(r.left, r.top, r.right, r.top, CONTROL_BORDER, 1);
    let s = common_snapshot(hwnd);
    let color = TEXT;
    if s.simple || s.parts.is_empty() {
        let text = if s.simple {
            s.part_text.get(255).cloned().unwrap_or_default()
        } else {
            s.part_text.first().cloned().unwrap_or_else(|| title(hwnd))
        };
        p.text(
            Rect {
                left: r.left + 10,
                ..r
            },
            &text,
            color,
            0,
        );
        return;
    }
    let mut x = r.left;
    for (i, &edge) in s.parts.iter().enumerate() {
        let right = if edge < 0 { r.right } else { edge.min(r.right) };
        let text = s.part_text.get(i).cloned().unwrap_or_default();
        p.text(
            Rect {
                left: x + 10,
                right: right - 4,
                ..r
            },
            &text,
            color,
            0,
        );
        if edge >= 0 && right < r.right {
            p.line(right, r.top + 6, right, r.bottom - 6, CONTROL_BORDER, 1);
        }
        x = right;
    }
}

unsafe fn status_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let r = match msg {
        0x0401 | 0x040B => {
            // SB_SETTEXTA/W : wp bas = partie (255 = mode simple)
            let part = wp & 0xFF;
            let text = read_text_ptr(lp as usize, msg == 0x040B).unwrap_or_default();
            with_common(hwnd, |s| {
                if s.part_text.len() <= part {
                    s.part_text.resize(part + 1, String::new());
                }
                s.part_text[part] = text;
            });
            invalidate(hwnd);
            1
        }
        0x0402 | 0x040D => {
            // SB_GETTEXTA/W
            let s = common_snapshot(hwnd);
            let text = s.part_text.get(wp & 0xFF).cloned().unwrap_or_default();
            if lp != 0 {
                write_text_ptr(lp as usize, i32::MAX / 4, &text, msg == 0x040D);
            }
            text.chars().count() as isize
        }
        0x0403 | 0x040C => {
            // SB_GETTEXTLENGTH
            let s = common_snapshot(hwnd);
            s.part_text
                .get(wp & 0xFF)
                .map_or(0, |t| t.chars().count() as isize)
        }
        0x0404 => {
            // SB_SETPARTS
            let n = wp.min(256);
            let edges: Vec<i32> = (0..n).map(|i| *(lp as *const i32).add(i)).collect();
            with_common(hwnd, |s| s.parts = edges);
            invalidate(hwnd);
            1
        }
        0x0406 => {
            // SB_GETPARTS
            let s = common_snapshot(hwnd);
            if lp != 0 {
                for (i, &e) in s.parts.iter().take(wp).enumerate() {
                    *(lp as *mut i32).add(i) = e;
                }
            }
            s.parts.len() as isize
        }
        0x0409 => {
            with_common(hwnd, |s| s.simple = wp != 0);
            invalidate(hwnd);
            0
        }
        0x040A => {
            // SB_GETRECT
            let s = common_snapshot(hwnd);
            let r = client(hwnd);
            if lp != 0 {
                let left = if wp == 0 {
                    0
                } else {
                    s.parts.get(wp - 1).copied().unwrap_or(0)
                };
                let right = s
                    .parts
                    .get(wp)
                    .copied()
                    .filter(|&e| e >= 0)
                    .unwrap_or(r.right);
                *(lp as *mut Rect) = Rect { left, right, ..r };
            }
            1
        }
        0x0407 | 0x0408 | 0x040E | 0x040F | 0x0410 | 0x2001 => 0, // bordures, hauteur, icônes, couleur
        WM_SIZE => {
            place_status_bar(hwnd);
            invalidate(hwnd);
            0
        }
        _ => return None,
    };
    Some(r)
}

// =====================================================================
// msctls_progress32
// =====================================================================

fn paint_progress(hwnd: usize, p: &Painter, r: Rect) {
    background(hwnd, p.hdc, WM_CTLCOLORSTATIC, face_color(), p, r);
    let s = common_snapshot(hwnd);
    let vertical = style(hwnd) & 0x4 != 0; // PBS_VERTICAL
    let track_h = 4.min(r.height());
    let track = if vertical {
        Rect::from_size(
            r.left + (r.width() - track_h) / 2,
            r.top,
            track_h,
            r.height(),
        )
    } else {
        Rect::from_size(
            r.left,
            r.top + (r.height() - track_h) / 2,
            r.width(),
            track_h,
        )
    };
    p.shape(track, 0x00D0_D0D0, 0x00D0_D0D0, 2, false);
    let range = (s.hi - s.lo).max(1);
    let marquee = style(hwnd) & 0x8 != 0; // PBS_MARQUEE
    let frac = if marquee {
        30
    } else {
        ((s.value - s.lo).clamp(0, range) * 100) / range
    };
    if frac > 0 {
        let fill = if vertical {
            let h = track.height() * frac / 100;
            Rect::from_size(track.left, track.bottom - h, track.width(), h)
        } else {
            let w = track.width() * frac / 100;
            let x = if marquee {
                track.left + (track.width() - w) / 2
            } else {
                track.left
            };
            Rect::from_size(x, track.top, w, track.height())
        };
        p.shape(fill, ACCENT, ACCENT, 2, false);
    }
}

unsafe fn progress_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let s = common_snapshot(hwnd);
    let r = match msg {
        0x0401 => {
            // PBM_SETRANGE (16 bits)
            let (lo, hi) = (lp as u32 as u16 as i32, (lp as u32 >> 16) as u16 as i32);
            with_common(hwnd, |s| {
                s.lo = lo;
                s.hi = hi;
            });
            invalidate(hwnd);
            ((s.hi as isize & 0xFFFF) << 16) | (s.lo as isize & 0xFFFF)
        }
        0x0406 => {
            // PBM_SETRANGE32
            with_common(hwnd, |s| {
                s.lo = wp as i32;
                s.hi = lp as i32;
            });
            invalidate(hwnd);
            ((s.hi as isize & 0xFFFF) << 16) | (s.lo as isize & 0xFFFF)
        }
        0x0407 => {
            // PBM_GETRANGE : wp TRUE = minimum
            if lp != 0 {
                *(lp as *mut i32) = s.lo;
                *((lp as *mut i32).add(1)) = s.hi;
            }
            if wp != 0 {
                s.lo as isize
            } else {
                s.hi as isize
            }
        }
        0x0402 => {
            with_common(hwnd, |st| {
                st.value = (wp as i32).clamp(st.lo.min(st.hi), st.hi.max(st.lo))
            });
            invalidate(hwnd);
            s.value as isize
        }
        0x0403 => {
            with_common(hwnd, |st| {
                st.value = (st.value + wp as i32).clamp(st.lo, st.hi)
            });
            invalidate(hwnd);
            s.value as isize
        }
        0x0404 => {
            with_common(hwnd, |st| st.step = wp as i32);
            s.step as isize
        }
        0x0405 => {
            with_common(hwnd, |st| {
                st.value += st.step;
                if st.value > st.hi {
                    st.value = st.lo + (st.value - st.hi);
                }
            });
            invalidate(hwnd);
            s.value as isize
        }
        0x0408 => s.value as isize, // PBM_GETPOS
        0x0409 | 0x2001 => 0,       // couleurs
        0x040A => 1,                // PBM_SETMARQUEE
        0x0410 => 1,                // PBM_SETSTATE
        0x0411 => 1,                // PBM_GETSTATE (PBST_NORMAL)
        _ => return None,
    };
    Some(r)
}

// =====================================================================
// msctls_trackbar32
// =====================================================================

fn trackbar_vertical(hwnd: usize) -> bool {
    style(hwnd) & 0x2 != 0 // TBS_VERT
}

fn thumb_pos(hwnd: usize, s: &CommonState) -> (i32, i32) {
    let r = client(hwnd);
    let len = if trackbar_vertical(hwnd) {
        r.height()
    } else {
        r.width()
    } - 20;
    let range = (s.hi - s.lo).max(1);
    (
        10 + len.max(1) * (s.value - s.lo).clamp(0, range) / range,
        len.max(1),
    )
}

fn paint_trackbar(hwnd: usize, p: &Painter, r: Rect) {
    background(hwnd, p.hdc, WM_CTLCOLORSTATIC, face_color(), p, r);
    let s = common_snapshot(hwnd);
    let (at, len) = thumb_pos(hwnd, &s);
    if trackbar_vertical(hwnd) {
        let cx = r.width() / 2;
        p.shape(
            Rect::from_size(cx - 2, 10, 4, len),
            STRONG_BORDER,
            STRONG_BORDER,
            2,
            false,
        );
        p.shape(
            Rect::from_size(cx - 2, 10, 4, at - 10),
            ACCENT,
            ACCENT,
            2,
            false,
        );
        p.shape(
            Rect::from_size(cx - 10, at - 10, 20, 20),
            FIELD,
            CONTROL_BORDER,
            0,
            true,
        );
        p.shape(
            Rect::from_size(cx - 5, at - 5, 10, 10),
            ACCENT,
            ACCENT,
            0,
            true,
        );
    } else {
        let cy = r.height() / 2;
        p.shape(
            Rect::from_size(10, cy - 2, len, 4),
            STRONG_BORDER,
            STRONG_BORDER,
            2,
            false,
        );
        p.shape(
            Rect::from_size(10, cy - 2, at - 10, 4),
            ACCENT,
            ACCENT,
            2,
            false,
        );
        p.shape(
            Rect::from_size(at - 10, cy - 10, 20, 20),
            FIELD,
            CONTROL_BORDER,
            0,
            true,
        );
        p.shape(
            Rect::from_size(at - 5, cy - 5, 10, 10),
            ACCENT,
            ACCENT,
            0,
            true,
        );
    }
}

fn trackbar_set(hwnd: usize, value: i32, code: usize) {
    let changed = with_common(hwnd, |s| {
        let v = value.clamp(s.lo.min(s.hi), s.hi.max(s.lo));
        std::mem::replace(&mut s.value, v) != v
    });
    invalidate(hwnd);
    if changed || code == 8 {
        let msg = if trackbar_vertical(hwnd) {
            WM_VSCROLL
        } else {
            WM_HSCROLL
        };
        let pos = common_snapshot(hwnd).value;
        send(
            parent(hwnd),
            msg,
            code | ((pos as u16 as usize) << 16),
            hwnd as isize,
        );
    }
}

unsafe fn trackbar_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let s = common_snapshot(hwnd);
    let r = match msg {
        0x0400 => s.value as isize, // TBM_GETPOS
        0x0401 => s.lo as isize,
        0x0402 => s.hi as isize,
        0x0405 => {
            with_common(hwnd, |st| {
                st.value = (lp as i32).clamp(st.lo.min(st.hi), st.hi.max(st.lo))
            });
            invalidate(hwnd);
            0
        }
        0x0406 => {
            with_common(hwnd, |st| {
                st.lo = lp as u32 as u16 as i32;
                st.hi = (lp as u32 >> 16) as u16 as i32;
            });
            invalidate(hwnd);
            0
        }
        0x0407 => {
            with_common(hwnd, |st| st.lo = lp as i32);
            invalidate(hwnd);
            0
        }
        0x0408 => {
            with_common(hwnd, |st| st.hi = lp as i32);
            invalidate(hwnd);
            0
        }
        0x0415 => {
            with_common(hwnd, |st| st.page = lp as i32);
            s.page as isize
        }
        0x0417 => {
            with_common(hwnd, |st| st.step = lp as i32);
            s.step as isize
        }
        0x0416 => s.page as isize,
        0x0418 => s.step as isize,
        0x0403..=0x0404 | 0x0409..=0x0414 | 0x0419..=0x0430 => 0, // repères, sélection, bulles
        WM_GETDLGCODE => DLGC_WANTARROWS,
        WM_LBUTTONDOWN | WM_MOUSEMOVE => {
            if msg == WM_LBUTTONDOWN {
                user32::set_focus(hwnd);
                with_common(hwnd, |st| st.dragging = true);
                gui::set_capture(hwnd);
            }
            if common_snapshot(hwnd).dragging {
                let (x, y) = point(lp);
                let at = if trackbar_vertical(hwnd) { y } else { x };
                let (_, len) = thumb_pos(hwnd, &s);
                let range = (s.hi - s.lo).max(1);
                let v = s.lo + ((at - 10).clamp(0, len) * range + len / 2) / len;
                trackbar_set(hwnd, v, 5); // TB_THUMBTRACK
            }
            0
        }
        WM_LBUTTONUP => {
            if with_common(hwnd, |st| std::mem::replace(&mut st.dragging, false)) {
                gui::release_capture();
                trackbar_set(hwnd, common_snapshot(hwnd).value, 4); // TB_THUMBPOSITION
                trackbar_set(hwnd, common_snapshot(hwnd).value, 8); // TB_ENDTRACK
            }
            0
        }
        WM_KEYDOWN => {
            let step = s.step.max(1);
            let page = s.page.max(1);
            let (v, code) = match wp {
                VK_LEFT | VK_UP => (s.value - step, 0),
                VK_RIGHT | VK_DOWN => (s.value + step, 1),
                VK_PRIOR => (s.value - page, 2),
                VK_NEXT => (s.value + page, 3),
                VK_HOME => (s.lo, 7),
                VK_END => (s.hi, 6),
                _ => return Some(0),
            };
            trackbar_set(hwnd, v, code);
            trackbar_set(hwnd, common_snapshot(hwnd).value, 8);
            0
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            with_ctl(hwnd, |c| c.focused = msg == WM_SETFOCUS);
            invalidate(hwnd);
            0
        }
        _ => return None,
    };
    Some(r)
}

// =====================================================================
// msctls_updown32
// =====================================================================

const UDS_SETBUDDYINT: u32 = 0x2;
const UDS_ALIGNRIGHT: u32 = 0x4;
const UDS_ALIGNLEFT: u32 = 0x8;
const UDS_AUTOBUDDY: u32 = 0x10;
const UDS_HORZ: u32 = 0x40;
const UDS_NOTHOUSANDS: u32 = 0x80;

fn attach_buddy(hwnd: usize) {
    if style(hwnd) & UDS_AUTOBUDDY == 0 {
        return;
    }
    let siblings = gui::child_windows(parent(hwnd));
    if let Some(i) = siblings.iter().position(|&h| h == hwnd) {
        if i > 0 {
            set_buddy(hwnd, siblings[i - 1]);
        }
    }
}

/// Colle les flèches au bord du champ associé, qui rétrécit d'autant.
fn set_buddy(hwnd: usize, buddy: usize) -> usize {
    let old = with_common(hwnd, |s| std::mem::replace(&mut s.buddy, buddy));
    let st = style(hwnd);
    if buddy != 0 && st & (UDS_ALIGNRIGHT | UDS_ALIGNLEFT) != 0 {
        if let (Some(b), Some(me)) = (gui::window_rect(buddy), gui::window_rect(hwnd)) {
            let w = me.width().clamp(12, 20);
            unsafe {
                if st & UDS_ALIGNRIGHT != 0 {
                    user32::set_window_pos(
                        buddy,
                        0,
                        b.left,
                        b.top,
                        b.width() - w,
                        b.height(),
                        0x14,
                    );
                    user32::set_window_pos(hwnd, 0, b.right - w, b.top, w, b.height(), 0x14);
                } else {
                    user32::set_window_pos(
                        buddy,
                        0,
                        b.left + w,
                        b.top,
                        b.width() - w,
                        b.height(),
                        0x14,
                    );
                    user32::set_window_pos(hwnd, 0, b.left, b.top, w, b.height(), 0x14);
                }
            }
        }
    }
    if buddy != 0 {
        updown_show(hwnd);
    }
    old
}

fn updown_show(hwnd: usize) {
    let s = common_snapshot(hwnd);
    if s.buddy == 0 || style(hwnd) & UDS_SETBUDDYINT == 0 {
        return;
    }
    let mut text = s.value.to_string();
    if style(hwnd) & UDS_NOTHOUSANDS == 0 && s.value.abs() >= 1000 {
        let digits: Vec<char> = s.value.abs().to_string().chars().collect();
        let mut out = String::new();
        for (i, d) in digits.iter().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push('\u{202F}');
            }
            out.push(*d);
        }
        text = if s.value < 0 { format!("-{out}") } else { out };
    }
    let w: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe {
        user32::set_window_text_w(s.buddy, w.as_ptr());
    }
}

fn paint_updown(hwnd: usize, c: &Ctl, p: &Painter, r: Rect) {
    background(hwnd, p.hdc, WM_CTLCOLORSTATIC, face_color(), p, r);
    let horz = style(hwnd) & UDS_HORZ != 0;
    let col = if enabled(hwnd) {
        0x005D_5D5D
    } else {
        TEXT_DISABLED
    };
    p.shape(
        Rect {
            right: r.right - 1,
            bottom: r.bottom - 1,
            ..r
        },
        CONTROL_FILL,
        CONTROL_BORDER,
        3,
        false,
    );
    if horz {
        let cy = r.height() / 2;
        let q = r.width() / 4;
        p.line(q + 2, cy - 3, q - 1, cy, col, 1);
        p.line(q - 1, cy, q + 2, cy + 3, col, 1);
        p.line(3 * q - 2, cy - 3, 3 * q + 1, cy, col, 1);
        p.line(3 * q + 1, cy, 3 * q - 2, cy + 3, col, 1);
    } else {
        let cx = r.width() / 2;
        let q = r.height() / 4;
        p.line(cx - 3, q + 2, cx, q - 1, col, 1);
        p.line(cx, q - 1, cx + 3, q + 2, col, 1);
        p.line(cx - 3, 3 * q - 2, cx, 3 * q + 1, col, 1);
        p.line(cx, 3 * q + 1, cx + 3, 3 * q - 2, col, 1);
        p.line(
            2,
            r.height() / 2,
            r.width() - 3,
            r.height() / 2,
            CONTROL_BORDER,
            1,
        );
    }
    let _ = c;
}

/// Un cran vers le haut (+1) ou vers le bas (-1), après UDN_DELTAPOS.
fn updown_step(hwnd: usize, delta: i32) {
    #[repr(C)]
    struct NmUpDown {
        hdr: NmHdr,
        pos: i32,
        delta: i32,
    }
    // UDS_SETBUDDYINT : la valeur part du nombre tapé dans le champ associé.
    let buddy = common_snapshot(hwnd).buddy;
    if style(hwnd) & 0x2 != 0 && buddy != 0 {
        let typed: String = title(buddy)
            .chars()
            .filter(|c| *c != ' ' && *c != '\u{202F}')
            .collect();
        if let Ok(v) = typed.trim().parse::<i32>() {
            with_common(hwnd, |st| st.value = v);
        }
    }
    let s = common_snapshot(hwnd);
    // Plage inversée (défaut 100..0) : le haut fait baisser la valeur.
    let delta = if s.lo > s.hi { -delta } else { delta };
    let mut nm = NmUpDown {
        hdr: NmHdr {
            hwnd_from: 0,
            id_from: 0,
            code: 0,
        },
        pos: s.value,
        delta,
    };
    if notify_struct(hwnd, -722, &mut nm) != 0 {
        return;
    }
    let (lo, hi) = (s.lo.min(s.hi), s.lo.max(s.hi));
    let v = (s.value + nm.delta).clamp(lo, hi);
    with_common(hwnd, |st| st.value = v);
    updown_show(hwnd);
    let msg = if style(hwnd) & UDS_HORZ != 0 {
        WM_HSCROLL
    } else {
        WM_VSCROLL
    };
    send(
        parent(hwnd),
        msg,
        4 | ((v as u16 as usize) << 16),
        hwnd as isize,
    );
    send(parent(hwnd), msg, 8, hwnd as isize);
}

unsafe fn updown_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let s = common_snapshot(hwnd);
    let r = match msg {
        0x0465 => {
            // UDM_SETRANGE : lp = MAKELONG(haut, bas)
            with_common(hwnd, |st| {
                st.hi = lp as u32 as u16 as i16 as i32;
                st.lo = (lp as u32 >> 16) as u16 as i16 as i32;
            });
            0
        }
        0x0466 => (((s.lo as isize) & 0xFFFF) << 16) | (s.hi as isize & 0xFFFF),
        0x046F => {
            with_common(hwnd, |st| {
                st.lo = wp as i32;
                st.hi = lp as i32;
            });
            0
        }
        0x0470 => {
            if wp != 0 {
                *(wp as *mut i32) = s.lo;
            }
            if lp != 0 {
                *(lp as *mut i32) = s.hi;
            }
            0
        }
        0x0467 | 0x0471 => {
            let v = if msg == 0x0467 {
                lp as i16 as i32
            } else {
                lp as i32
            };
            let (lo, hi) = (s.lo.min(s.hi), s.lo.max(s.hi));
            with_common(hwnd, |st| st.value = v.clamp(lo, hi));
            updown_show(hwnd);
            s.value as isize
        }
        0x0468 => s.value as isize & 0xFFFF,
        0x0472 => {
            if lp != 0 {
                *(lp as *mut i32) = 0;
            }
            s.value as isize
        }
        0x0469 => set_buddy(hwnd, wp) as isize,
        0x046A => s.buddy as isize,
        0x046B..=0x046E => 0, // accélérations, base
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            if !enabled(hwnd) {
                return Some(0);
            }
            let (x, y) = point(lp);
            let r = client(hwnd);
            let up = if style(hwnd) & UDS_HORZ != 0 {
                x >= r.width() / 2
            } else {
                y < r.height() / 2
            };
            updown_step(hwnd, if up { 1 } else { -1 });
            0
        }
        WM_GETDLGCODE => 0,
        _ => return None,
    };
    Some(r)
}

/* Contrôles communs et boîtes de dialogue communes dans un programme
 * Windows 32 bits (#232, étape 6). MinGW i686, en C, avec les vrais
 * en-têtes : les structures ont donc leur disposition x86 (OPENFILENAME de
 * 88 octets, PRINTDLG compactée, LVITEM de 60 octets…).
 *
 * Sans affichage, WINLUX_FILEDLG, WINLUX_CHOOSECOLOR et WINLUX_CHOOSEFONT
 * font le choix de l'utilisateur. Rend 0 si tout va bien, sinon le numéro
 * de la vérification en échec. */
#define _WIN32_IE 0x0600
#define _WIN32_WINNT 0x0601
#define COBJMACROS
#include <windows.h>
#include <commctrl.h>
#include <commdlg.h>
#include <objbase.h>
#include <shellapi.h>
#include <shlobj.h>
#include <stdio.h>
#include <string.h>
#include <wchar.h>

static int step;
#define CHECK(c, what) do { step++; if (!(c)) { \
    printf("dlg32 : vérification %d en échec : %s\n", step, what); \
    fflush(stdout); ExitProcess(step); } } while (0)

static int ends_with(const WCHAR *s, const WCHAR *tail)
{
    size_t n = wcslen(s), m = wcslen(tail);
    return n >= m && wcscmp(s + n - m, tail) == 0;
}

static HWND control(const WCHAR *cls, DWORD style)
{
    return CreateWindowExW(0, cls, L"", WS_POPUP | style, 0, 0, 300, 200, NULL, NULL, NULL, NULL);
}

static int CALLBACK by_text_desc(LPARAM a, LPARAM b, LPARAM sort)
{
    return (int)(b - a) * (int)sort;
}

static void list_view(void)
{
    HWND lv = control(WC_LISTVIEWW, LVS_REPORT);
    CHECK(lv != NULL, "SysListView32");
    LVCOLUMNW col = {0};
    col.mask = LVCF_TEXT | LVCF_WIDTH;
    col.cx = 120;
    col.pszText = L"Nom";
    CHECK(SendMessageW(lv, LVM_INSERTCOLUMNW, 0, (LPARAM)&col) == 0, "LVM_INSERTCOLUMNW");
    col.pszText = L"Taille";
    CHECK(SendMessageW(lv, LVM_INSERTCOLUMNW, 1, (LPARAM)&col) == 1, "seconde colonne");
    const WCHAR *names[] = { L"alpha.txt", L"beta.txt", L"gamma.txt" };
    for (int i = 0; i < 3; i++) {
        LVITEMW it = {0};
        it.mask = LVIF_TEXT | LVIF_PARAM;
        it.iItem = i;
        it.pszText = (WCHAR *)names[i];
        it.lParam = 100 + i;
        CHECK(SendMessageW(lv, LVM_INSERTITEMW, 0, (LPARAM)&it) == i, "LVM_INSERTITEMW");
        LVITEMW sub = {0};
        sub.iSubItem = 1;
        sub.pszText = L"1 Ko";
        CHECK(SendMessageW(lv, LVM_SETITEMTEXTW, i, (LPARAM)&sub) != 0, "LVM_SETITEMTEXTW");
    }
    CHECK(SendMessageW(lv, LVM_GETITEMCOUNT, 0, 0) == 3, "LVM_GETITEMCOUNT");
    WCHAR text[64] = {0};
    LVITEMW get = {0};
    get.iSubItem = 0;
    get.pszText = text;
    get.cchTextMax = 64;
    CHECK(SendMessageW(lv, LVM_GETITEMTEXTW, 1, (LPARAM)&get) == 8 && wcscmp(text, L"beta.txt") == 0,
          "LVM_GETITEMTEXTW");
    /* LVM_GETITEMW : lParam recopié dans la structure x86, sans déborder. */
    struct { LVITEMW it; DWORD guard; } g;
    memset(&g, 0, sizeof g);
    g.guard = 0xDEADBEEF;
    g.it.mask = LVIF_PARAM | LVIF_TEXT;
    g.it.iItem = 2;
    g.it.pszText = text;
    g.it.cchTextMax = 64;
    CHECK(SendMessageW(lv, LVM_GETITEMW, 0, (LPARAM)&g.it) != 0, "LVM_GETITEMW");
    CHECK(g.it.lParam == 102 && wcscmp(text, L"gamma.txt") == 0, "LVITEM relu");
    CHECK(g.guard == 0xDEADBEEF, "LVITEM sans débordement");
    LVCOLUMNW gc = {0};
    WCHAR ctext[32] = {0};
    gc.mask = LVCF_TEXT | LVCF_WIDTH;
    gc.pszText = ctext;
    gc.cchTextMax = 32;
    CHECK(SendMessageW(lv, LVM_GETCOLUMNW, 1, (LPARAM)&gc) != 0 && wcscmp(ctext, L"Taille") == 0,
          "LVM_GETCOLUMNW");
    LVFINDINFOW fi = {0};
    fi.flags = LVFI_STRING;
    fi.psz = L"gamma.txt";
    CHECK(SendMessageW(lv, LVM_FINDITEMW, -1, (LPARAM)&fi) == 2, "LVM_FINDITEMW");
    /* Tri par une fonction x86 (rappel 64 → 32). */
    CHECK(SendMessageW(lv, LVM_SORTITEMS, 1, (LPARAM)by_text_desc) != 0, "LVM_SORTITEMS");
    CHECK(SendMessageW(lv, LVM_GETITEMTEXTW, 0, (LPARAM)&get) > 0 && wcscmp(text, L"gamma.txt") == 0,
          "tri décroissant par le rappel x86");
    DestroyWindow(lv);
}

static void tree_view(void)
{
    HWND tv = control(WC_TREEVIEWW, TVS_HASLINES);
    CHECK(tv != NULL, "SysTreeView32");
    TVINSERTSTRUCTW ins = {0};
    ins.hParent = TVI_ROOT;
    ins.hInsertAfter = TVI_LAST;
    ins.item.mask = TVIF_TEXT | TVIF_PARAM;
    ins.item.pszText = L"Connexion";
    ins.item.lParam = 7;
    HTREEITEM root = (HTREEITEM)SendMessageW(tv, TVM_INSERTITEMW, 0, (LPARAM)&ins);
    CHECK(root != NULL, "TVM_INSERTITEMW (racine)");
    ins.hParent = root;
    ins.item.pszText = L"SSH";
    ins.item.lParam = 8;
    HTREEITEM child = (HTREEITEM)SendMessageW(tv, TVM_INSERTITEMW, 0, (LPARAM)&ins);
    CHECK(child != NULL && child != root, "TVM_INSERTITEMW (enfant)");
    CHECK(SendMessageW(tv, TVM_GETCOUNT, 0, 0) == 2, "TVM_GETCOUNT");
    CHECK((HTREEITEM)SendMessageW(tv, TVM_GETNEXTITEM, TVGN_CHILD, (LPARAM)root) == child,
          "TVM_GETNEXTITEM (HTREEITEM aller-retour)");
    struct { TVITEMW it; DWORD guard; } g;
    WCHAR text[32] = {0};
    memset(&g, 0, sizeof g);
    g.guard = 0xDEADBEEF;
    g.it.mask = TVIF_TEXT | TVIF_PARAM;
    g.it.hItem = child;
    g.it.pszText = text;
    g.it.cchTextMax = 32;
    CHECK(SendMessageW(tv, TVM_GETITEMW, 0, (LPARAM)&g.it) != 0, "TVM_GETITEMW");
    CHECK(wcscmp(text, L"SSH") == 0 && g.it.lParam == 8 && g.guard == 0xDEADBEEF, "TVITEM relu");
    DestroyWindow(tv);
}

static void tool_bar(void)
{
    HWND tb = control(TOOLBARCLASSNAMEW, 0);
    CHECK(tb != NULL, "ToolbarWindow32");
    SendMessageW(tb, TB_BUTTONSTRUCTSIZE, sizeof(TBBUTTON), 0);
    CHECK(sizeof(TBBUTTON) == 20, "TBBUTTON de 20 octets");
    TBBUTTON b[2];
    memset(b, 0, sizeof b);
    b[0].iBitmap = 0; b[0].idCommand = 101; b[0].fsState = TBSTATE_ENABLED; b[0].fsStyle = BTNS_BUTTON;
    b[0].iString = (INT_PTR)L"Ouvrir";
    b[1].iBitmap = 1; b[1].idCommand = 102; b[1].fsState = TBSTATE_ENABLED; b[1].fsStyle = BTNS_BUTTON;
    b[1].iString = (INT_PTR)L"Enregistrer";
    CHECK(SendMessageW(tb, TB_ADDBUTTONSW, 2, (LPARAM)b) != 0, "TB_ADDBUTTONSW");
    CHECK(SendMessageW(tb, TB_BUTTONCOUNT, 0, 0) == 2, "TB_BUTTONCOUNT");
    TBBUTTON got;
    memset(&got, 0, sizeof got);
    CHECK(SendMessageW(tb, TB_GETBUTTON, 1, (LPARAM)&got) != 0 && got.idCommand == 102, "TB_GETBUTTON");
    TBBUTTONINFOW bi = {0};
    bi.cbSize = sizeof bi;
    bi.dwMask = TBIF_COMMAND | TBIF_IMAGE;
    CHECK(SendMessageW(tb, TB_GETBUTTONINFOW, 101, (LPARAM)&bi) == 0 && bi.idCommand == 101, "TB_GETBUTTONINFOW");
    DestroyWindow(tb);
}

static void tab_control(void)
{
    HWND tab = control(WC_TABCONTROLW, 0);
    CHECK(tab != NULL, "SysTabControl32");
    TCITEMW t = {0};
    t.mask = TCIF_TEXT | TCIF_PARAM;
    t.pszText = L"Général";
    t.lParam = 11;
    CHECK(SendMessageW(tab, TCM_INSERTITEMW, 0, (LPARAM)&t) == 0, "TCM_INSERTITEMW");
    t.pszText = L"Avancé";
    t.lParam = 12;
    CHECK(SendMessageW(tab, TCM_INSERTITEMW, 1, (LPARAM)&t) == 1, "second onglet");
    WCHAR text[32] = {0};
    TCITEMW g = {0};
    g.mask = TCIF_TEXT | TCIF_PARAM;
    g.pszText = text;
    g.cchTextMax = 32;
    CHECK(SendMessageW(tab, TCM_GETITEMW, 1, (LPARAM)&g) != 0, "TCM_GETITEMW");
    CHECK(wcscmp(text, L"Avancé") == 0 && g.lParam == 12, "TCITEM relu");
    /* Comme la VCL de Delphi (#1129) : seuls mask et lParam sont remplis,
       pszText et cchTextMax restent non initialisés. */
    TCITEMW o;
    memset(&o, 0x77, sizeof o);
    o.mask = TCIF_PARAM;
    o.lParam = (LPARAM)0xff946730u;
    CHECK(SendMessageW(tab, TCM_SETITEMW, 0, (LPARAM)&o) != 0, "TCM_SETITEMW sans TCIF_TEXT");
    TCITEMW q;
    memset(&q, 0x77, sizeof q);
    q.mask = TCIF_PARAM;
    CHECK(SendMessageW(tab, TCM_GETITEMW, 0, (LPARAM)&q) != 0 && q.lParam == (LPARAM)0xff946730u,
          "objet de l'onglet relu (pszText non initialisé)");
    DestroyWindow(tab);
}

/* Superclasse sous-classée, comme TPageControl de la VCL (#1129) : classe
 * enregistrée avec la procédure de SysTabControl32, puis procédure x86 qui
 * passe tout par CallWindowProcW. Le TCITEM de 28 octets est juste sous
 * des données de la pile : rien ne doit être écrit au-delà. */
static WNDPROC page_old;
static LRESULT CALLBACK page_proc(HWND h, UINT m, WPARAM w, LPARAM l)
{
    return CallWindowProcW(page_old, h, m, w, l);
}

static void tab_superclass(void)
{
    WNDCLASSW wc;
    CHECK(GetClassInfoW(NULL, WC_TABCONTROLW, &wc), "GetClassInfoW(SysTabControl32)");
    wc.lpszClassName = L"TPageDeTest";
    wc.hInstance = GetModuleHandleW(NULL);
    CHECK(RegisterClassW(&wc) != 0, "superclasse d'onglets");
    HWND tab = CreateWindowExW(0, L"TPageDeTest", L"", WS_POPUP, 0, 0, 300, 200, NULL, NULL, NULL, NULL);
    CHECK(tab != NULL, "fenêtre de la superclasse");
    page_old = (WNDPROC)SetWindowLongPtrW(tab, GWLP_WNDPROC, (LONG_PTR)page_proc);
    TCITEMW t = {0};
    t.mask = TCIF_TEXT | TCIF_PARAM;
    t.pszText = L"Session";
    t.lParam = 0x1234;
    CHECK(SendMessageW(tab, TCM_INSERTITEMW, 0, (LPARAM)&t) == 0, "TCM_INSERTITEMW (superclasse)");
    struct {
        TCITEMW it;
        DWORD canary[4];
    } q;
    memset(&q, 0x77, sizeof q);
    q.it.mask = TCIF_PARAM;
    CHECK(SendMessageW(tab, TCM_GETITEMW, 0, (LPARAM)&q.it) != 0 && q.it.lParam == 0x1234,
          "TCM_GETITEMW (superclasse sous-classée)");
    CHECK(q.canary[0] == 0x77777777u && q.canary[1] == 0x77777777u && q.canary[2] == 0x77777777u,
          "rien d'écrit après le TCITEM x86");
    WCHAR text[16] = {0};
    TCITEMW g = {0};
    g.mask = TCIF_TEXT;
    g.pszText = text;
    g.cchTextMax = 16;
    CHECK(SendMessageW(tab, TCM_GETITEMW, 0, (LPARAM)&g) != 0 && wcscmp(text, L"Session") == 0,
          "texte relu (superclasse)");
    DestroyWindow(tab);
}

/* Comme la VCL de Delphi (#1129, WinSCP) : la classe est enregistrée avec
 * une procédure x86 (InitWndProc), la procédure de SysTabControl32 obtenue
 * par GetClassInfoW n'est connue que du programme, qui lui passe tout par
 * CallWindowProcW. */
static WNDPROC vcl_default;
static LRESULT CALLBACK vcl_proc(HWND h, UINT m, WPARAM w, LPARAM l)
{
    return CallWindowProcW(vcl_default, h, m, w, l);
}

static void tab_vcl(void)
{
    WNDCLASSW wc;
    CHECK(GetClassInfoW(NULL, WC_TABCONTROLW, &wc), "GetClassInfoW (VCL)");
    vcl_default = wc.lpfnWndProc;
    wc.lpfnWndProc = vcl_proc;
    wc.lpszClassName = L"TThemePageDeTest";
    wc.hInstance = GetModuleHandleW(NULL);
    CHECK(RegisterClassW(&wc) != 0, "classe VCL d'onglets");
    HWND tab = CreateWindowExW(0, L"TThemePageDeTest", L"", WS_POPUP, 0, 0, 300, 200, NULL, NULL, NULL, NULL);
    CHECK(tab != NULL, "fenêtre VCL d'onglets");
    TCITEMW t = {0};
    t.mask = TCIF_TEXT | TCIF_PARAM;
    t.pszText = L"Sessions";
    t.lParam = 0x5678;
    CHECK(SendMessageW(tab, TCM_INSERTITEMW, 0, (LPARAM)&t) == 0, "TCM_INSERTITEMW (VCL)");
    struct {
        TCITEMW it;
        DWORD canary[4];
    } q;
    memset(&q, 0x77, sizeof q);
    q.it.mask = TCIF_PARAM;
    CHECK(SendMessageW(tab, TCM_GETITEMW, 0, (LPARAM)&q.it) != 0 && q.it.lParam == 0x5678,
          "TCM_GETITEMW (classe VCL)");
    CHECK(q.canary[0] == 0x77777777u && q.canary[1] == 0x77777777u && q.canary[2] == 0x77777777u,
          "rien d'écrit après le TCITEM x86 (VCL)");
    DestroyWindow(tab);
}

/* Notifications des contrôles vers une fenêtre x86 (WM_NOTIFY en
 * disposition x86 : NMHDR de 12 octets, NMLISTVIEW, NMTREEVIEW). */
static int lv_changed = -1, lv_param;
static LPARAM tv_param;
static UINT tv_code;
static LRESULT CALLBACK parent_proc(HWND h, UINT msg, WPARAM wp, LPARAM lp)
{
    if (msg == WM_NOTIFY) {
        NMHDR *hdr = (NMHDR *)lp;
        if (hdr->code == LVN_ITEMCHANGED && hdr->idFrom == 51) {
            NMLISTVIEW *nm = (NMLISTVIEW *)lp;
            if (nm->uNewState & LVIS_SELECTED) {
                lv_changed = nm->iItem;
                lv_param = (int)nm->lParam;
            }
        } else if ((hdr->code == TVN_SELCHANGEDW || hdr->code == TVN_SELCHANGEDA) && hdr->idFrom == 52) {
            NMTREEVIEWW *nm = (NMTREEVIEWW *)lp;
            tv_code = hdr->code;
            tv_param = nm->itemNew.lParam;
        }
        return 0;
    }
    return DefWindowProcW(h, msg, wp, lp);
}

/* FindText / ReplaceText (#712) : le propriétaire x86 reçoit
   « commdlg_FindReplace » avec SA FINDREPLACE de 40 octets. */
static UINT find_msg;
static int find_count;
static DWORD find_flags;
static LPARAM find_lp, hook_lp;
static LRESULT CALLBACK find_owner_proc(HWND h, UINT msg, WPARAM wp, LPARAM lp)
{
    if (find_msg && msg == find_msg) {
        find_count++;
        find_lp = lp;
        find_flags = ((FINDREPLACEW *)lp)->Flags;
        return 0;
    }
    return DefWindowProcW(h, msg, wp, lp);
}

static UINT_PTR CALLBACK find_hook(HWND h, UINT msg, WPARAM wp, LPARAM lp)
{
    (void)h; (void)wp;
    if (msg == WM_INITDIALOG)
        hook_lp = lp;
    return 0;
}

static void find_replace32(void)
{
    WNDCLASSW wc = {0};
    wc.lpfnWndProc = find_owner_proc;
    wc.lpszClassName = L"Dlg32Find";
    RegisterClassW(&wc);
    HWND owner = CreateWindowExW(0, L"Dlg32Find", L"Recherche", WS_OVERLAPPEDWINDOW, 0, 0, 300, 200,
                                 NULL, NULL, NULL, NULL);
    find_msg = RegisterWindowMessageW(FINDMSGSTRINGW);
    CHECK(find_msg >= 0xC000 && find_msg != RegisterWindowMessageW(L"TaskbarCreated"),
          "RegisterWindowMessageW(FINDMSGSTRING) x86");
    CHECK(sizeof(FINDREPLACEW) == 40, "FINDREPLACE x86 de 40 octets");

    WCHAR what[32] = L"abc", with[32] = L"";
    FINDREPLACEW fr;
    memset(&fr, 0, sizeof fr);
    fr.lStructSize = sizeof fr;
    fr.hwndOwner = owner;
    fr.Flags = FR_DOWN | FR_ENABLEHOOK;
    fr.lpfnHook = find_hook;
    fr.lpstrFindWhat = what;
    fr.wFindWhatLen = 32;
    HWND fd = FindTextW(&fr);
    CHECK(fd != NULL && IsWindow(fd), "FindTextW x86 ouvre la boîte");
    if (!fd)
        return;
    CHECK(hook_lp == (LPARAM)&fr, "crochet x86 : WM_INITDIALOG avec la FINDREPLACE");
    WCHAR got[32];
    GetDlgItemTextW(fd, 0x480, got, 32);
    CHECK(wcscmp(got, L"abc") == 0, "texte de départ x86");
    SetDlgItemTextW(fd, 0x480, L"Winlux");
    SendMessageW(GetDlgItem(fd, 0x411), BM_SETCHECK, BST_CHECKED, 0);
    SendMessageW(fd, WM_COMMAND, IDOK, 0);
    CHECK(find_count == 1 && find_lp == (LPARAM)&fr, "Suivant x86 : le propriétaire reçoit sa structure");
    CHECK((find_flags & (FR_FINDNEXT | FR_MATCHCASE | FR_DOWN)) == (FR_FINDNEXT | FR_MATCHCASE | FR_DOWN)
              && wcscmp(what, L"Winlux") == 0,
          "Suivant x86 : options et texte");
    SendMessageW(fd, WM_COMMAND, IDCANCEL, 0);
    CHECK((find_flags & FR_DIALOGTERM) && !IsWindow(fd), "fermeture x86 : FR_DIALOGTERM");

    fr.Flags = 0;
    fr.lpfnHook = NULL;
    fr.lpstrReplaceWith = with;
    fr.wReplaceWithLen = 32;
    HWND rd = ReplaceTextW(&fr);
    CHECK(rd != NULL, "ReplaceTextW x86");
    if (!rd)
        return;
    SetDlgItemTextW(rd, 0x481, L"Ersatz");
    SendMessageW(rd, WM_COMMAND, 0x401, 0);
    CHECK((find_flags & FR_REPLACEALL) && wcscmp(with, L"Ersatz") == 0, "Remplacer tout x86");
    SendMessageW(rd, WM_COMMAND, IDCANCEL, 0);
    DestroyWindow(owner);
    find_msg = 0;
}

static void notifications(void)
{
    WNDCLASSW wc = {0};
    wc.lpfnWndProc = parent_proc;
    wc.lpszClassName = L"Dlg32Parent";
    CHECK(RegisterClassW(&wc) != 0, "RegisterClassW (parent x86)");
    HWND parent = CreateWindowExW(0, L"Dlg32Parent", L"Parent", WS_OVERLAPPEDWINDOW, 0, 0, 400, 300,
                                  NULL, NULL, NULL, NULL);
    CHECK(parent != NULL, "fenêtre parente x86");
    HWND lv = CreateWindowExW(0, WC_LISTVIEWW, L"", WS_CHILD | LVS_REPORT, 0, 0, 200, 200, parent,
                              (HMENU)51, NULL, NULL);
    CHECK(lv != NULL, "liste enfant");
    LVCOLUMNW col = {0};
    col.mask = LVCF_TEXT | LVCF_WIDTH;
    col.cx = 100;
    col.pszText = L"Nom";
    SendMessageW(lv, LVM_INSERTCOLUMNW, 0, (LPARAM)&col);
    for (int i = 0; i < 2; i++) {
        LVITEMW it = {0};
        it.mask = LVIF_TEXT | LVIF_PARAM;
        it.iItem = i;
        it.pszText = i ? L"deux" : L"un";
        it.lParam = 500 + i;
        SendMessageW(lv, LVM_INSERTITEMW, 0, (LPARAM)&it);
    }
    LVITEMW st = {0};
    st.stateMask = LVIS_SELECTED;
    st.state = LVIS_SELECTED;
    SendMessageW(lv, LVM_SETITEMSTATE, 1, (LPARAM)&st);
    CHECK(lv_changed == 1 && lv_param == 501, "LVN_ITEMCHANGED reçu en x86 (NMLISTVIEW)");

    HWND tv = CreateWindowExW(0, WC_TREEVIEWW, L"", WS_CHILD, 0, 0, 200, 200, parent, (HMENU)52, NULL, NULL);
    CHECK(tv != NULL, "arbre enfant");
    TVINSERTSTRUCTW ins = {0};
    ins.hParent = TVI_ROOT;
    ins.hInsertAfter = TVI_LAST;
    ins.item.mask = TVIF_TEXT | TVIF_PARAM;
    ins.item.pszText = L"Session";
    ins.item.lParam = 77;
    HTREEITEM h = (HTREEITEM)SendMessageW(tv, TVM_INSERTITEMW, 0, (LPARAM)&ins);
    SendMessageW(tv, TVM_SELECTITEM, TVGN_CARET, (LPARAM)h);
    CHECK(tv_code != 0 && tv_param == 77, "TVN_SELCHANGED reçu en x86 (NMTREEVIEW)");
    DestroyWindow(parent);
}

static void dialogs(void)
{
    WCHAR temp[MAX_PATH];
    DWORD tl = GetTempPathW(MAX_PATH, temp);
    CHECK(tl > 0, "GetTempPathW");
    WCHAR path[MAX_PATH];
    swprintf(path, MAX_PATH, L"%lsdlg32-test.txt", temp);
    HANDLE h = CreateFileW(path, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, 0, NULL);
    CloseHandle(h);
    SetEnvironmentVariableW(L"WINLUX_FILEDLG", L"dlg32-test.txt");

    CHECK(sizeof(OPENFILENAMEW) == 88, "OPENFILENAMEW de 88 octets");
    WCHAR file[MAX_PATH] = L"dlg32-test.txt";
    WCHAR title[MAX_PATH] = {0};
    OPENFILENAMEW o = {0};
    o.lStructSize = sizeof o;
    o.lpstrFilter = L"Texte (*.txt)\0*.txt\0Tous les fichiers\0*.*\0";
    o.nFilterIndex = 1;
    o.lpstrFile = file;
    o.nMaxFile = MAX_PATH;
    o.lpstrFileTitle = title;
    o.nMaxFileTitle = MAX_PATH;
    o.lpstrInitialDir = temp;
    o.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST;
    o.lCustData = 0x1234;
    CHECK(GetOpenFileNameW(&o) == TRUE, "GetOpenFileNameW");
    CHECK(ends_with(file, L"\\dlg32-test.txt"), "chemin choisi");
    CHECK(wcscmp(file + o.nFileOffset, L"dlg32-test.txt") == 0, "nFileOffset recopié");
    CHECK(o.lCustData == 0x1234 && o.lStructSize == 88, "structure x86 intacte");

    /* Ancienne taille (OPENFILENAME_SIZE_VERSION_400 : 76 octets). */
    WCHAR save[MAX_PATH] = L"dlg32-test.txt";
    struct { OPENFILENAMEW o; DWORD guard; } s;
    memset(&s, 0, sizeof s);
    s.guard = 0xDEADBEEF;
    s.o.lStructSize = 76;
    s.o.lpstrFile = save;
    s.o.nMaxFile = MAX_PATH;
    s.o.lpstrInitialDir = temp;
    s.o.lpstrDefExt = L"txt";
    s.o.FlagsEx = 0x55;
    CHECK(GetSaveFileNameW(&s.o) == TRUE, "GetSaveFileNameW (76 octets)");
    CHECK(ends_with(save, L"\\dlg32-test.txt") && s.o.FlagsEx == 0x55 && s.guard == 0xDEADBEEF,
          "champs récents laissés tels quels");

    SetEnvironmentVariableW(L"WINLUX_CHOOSECOLOR", L"6A3DF0");
    COLORREF custom[16] = {0};
    CHOOSECOLORW cc = {0};
    cc.lStructSize = sizeof cc;
    cc.lpCustColors = custom;
    cc.Flags = CC_RGBINIT;
    cc.rgbResult = RGB(1, 2, 3);
    CHECK(sizeof cc == 36, "CHOOSECOLORW de 36 octets");
    CHECK(ChooseColorW(&cc) == TRUE && cc.rgbResult == RGB(0x6A, 0x3D, 0xF0), "ChooseColorW");

    SetEnvironmentVariableW(L"WINLUX_CHOOSEFONT", L"DejaVu Sans Mono,12,gras");
    LOGFONTW lf = {0};
    CHOOSEFONTW cf = {0};
    cf.lStructSize = sizeof cf;
    cf.lpLogFont = &lf;
    cf.Flags = CF_SCREENFONTS | CF_INITTOLOGFONTSTRUCT;
    CHECK(sizeof cf == 60, "CHOOSEFONTW de 60 octets");
    CHECK(ChooseFontW(&cf) == TRUE, "ChooseFontW");
    CHECK(wcsstr(lf.lfFaceName, L"DejaVu") != NULL && lf.lfWeight >= 700, "police choisie");
    CHECK(cf.iPointSize == 120, "taille en dixièmes de point");

    CHECK(sizeof(PRINTDLGW) == 66, "PRINTDLGW compactée (66 octets)");
    PRINTDLGW pd = {0};
    pd.lStructSize = sizeof pd;
    pd.Flags = PD_RETURNDEFAULT;
    BOOL ok = PrintDlgW(&pd);
    /* Sans imprimante, la boîte rend FALSE avec PDERR_NODEFAULTPRN. */
    CHECK(ok ? pd.hDevNames != NULL : CommDlgExtendedError() != 0, "PrintDlgW (PD_RETURNDEFAULT)");
    if (pd.hDevMode)
        GlobalFree(pd.hDevMode);
    if (pd.hDevNames)
        GlobalFree(pd.hDevNames);
}

/* Fonctions de comctl32, comdlg32 et shell32 demandées par WinSCP 6.7 x86. */
static void winscp(void)
{
    /* Barres de défilement « plates » : celles de la fenêtre. */
    HWND w = CreateWindowExW(0, L"Dlg32Parent", L"", WS_POPUP | WS_VSCROLL, 0, 0, 100, 100, NULL, NULL, NULL, NULL);
    CHECK(w != NULL, "fenêtre à barre de défilement");
    InitializeFlatSB(w);
    SCROLLINFO si = { sizeof si, SIF_RANGE | SIF_POS | SIF_PAGE, 0, 100, 10, 30, 0 };
    FlatSB_SetScrollInfo(w, SB_VERT, &si, FALSE);
    CHECK(FlatSB_GetScrollPos(w, SB_VERT) == 30, "FlatSB_GetScrollPos");
    FlatSB_SetScrollPos(w, SB_VERT, 40, FALSE);
    SCROLLINFO got = { sizeof got, SIF_ALL };
    CHECK(FlatSB_GetScrollInfo(w, SB_VERT, &got) && got.nPos == 40 && got.nMax == 100, "FlatSB_GetScrollInfo");
    CHECK(FlatSB_SetScrollProp(w, WSB_PROP_VSTYLE, FSB_FLAT_MODE, FALSE), "FlatSB_SetScrollProp");
    UninitializeFlatSB(w);
    DestroyWindow(w);

    /* Liste d'images : copie, dessin indirect, écriture puis relecture. */
    HIMAGELIST il = ImageList_Create(4, 4, ILC_COLOR32, 2, 2);
    DWORD px[16 * 2];
    for (int i = 0; i < 32; i++)
        px[i] = (i % 8) < 4 ? 0xFFFF0000u : 0xFF0000FFu; /* bleu puis rouge (BGRA) */
    BITMAPINFO bi = {{ sizeof(BITMAPINFOHEADER), 8, -4, 1, 32, BI_RGB }};
    HDC screen = GetDC(NULL);
    HBITMAP bmp = CreateDIBitmap(screen, &bi.bmiHeader, CBM_INIT, px, &bi, DIB_RGB_COLORS);
    CHECK(ImageList_Add(il, bmp, NULL) == 0 && ImageList_GetImageCount(il) == 2, "deux images");
    CHECK(ImageList_Copy(il, 0, il, 1, ILCF_SWAP), "ImageList_Copy (échange)");
    HDC mem = CreateCompatibleDC(screen);
    HBITMAP canvas = CreateCompatibleBitmap(screen, 8, 8);
    SelectObject(mem, canvas);
    IMAGELISTDRAWPARAMS dp = { sizeof dp };
    dp.himl = il;
    dp.i = 1;
    dp.hdcDst = mem;
    dp.rgbBk = CLR_NONE;
    dp.rgbFg = CLR_DEFAULT;
    dp.fStyle = ILD_NORMAL;
    CHECK(sizeof dp == 68, "IMAGELISTDRAWPARAMS de 68 octets");
    CHECK(ImageList_DrawIndirect(&dp), "ImageList_DrawIndirect");
    POINT pt = { 5, 5 }, hot = { 5, 5 };
    ImageList_GetDragImage(&pt, &hot);
    CHECK(pt.x == 0 && hot.y == 0, "ImageList_GetDragImage");
    IStream *st = NULL;
    CHECK(CreateStreamOnHGlobal(NULL, TRUE, &st) == S_OK, "flux en mémoire");
    CHECK(ImageList_Write(il, st), "ImageList_Write");
    LARGE_INTEGER zero = {{0}};
    IStream_Seek(st, zero, STREAM_SEEK_SET, NULL);
    HIMAGELIST il2 = ImageList_Read(st);
    int cx = 0, cy = 0;
    CHECK(il2 != NULL && ImageList_GetImageCount(il2) == 2 && ImageList_GetIconSize(il2, &cx, &cy) && cx == 4,
          "ImageList_Read");
    IStream_Release(st);
    ImageList_Destroy(il2);
    /* Flux « IL » d'un programme Delphi (ButtonImages de WinSCP) : bfSize
       des BMP réduit aux en-têtes, pixels et masque juste derrière. */
    static const BYTE ilhead[28] = { 'I', 'L', 1, 1, 1, 0, 4, 0, 4, 0, 2, 0, 2, 0,
                                     0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x10, 0xFF, 0xFF,
                                     0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF };
    BYTE delphi[28 + 54 + 64 + 62 + 8 + 22] = { 0 };
    memcpy(delphi, ilhead, 28);
    BYTE *b = delphi + 28;
    for (int k = 0; k < 2; k++) {
        DWORD size = k ? 62 : 54;
        b[0] = 'B';
        b[1] = 'M';
        memcpy(b + 2, &size, 4);
        memcpy(b + 10, &size, 4);
        LONG hdr[3] = { 40, 8, 2 };
        memcpy(b + 14, hdr, 12);
        b[26] = 1;
        b[28] = k ? 1 : 32;
        b[46] = k ? 2 : 0; /* biClrUsed */
        if (k) {
            b[58] = b[59] = b[60] = 0xFF; /* palette : noir, blanc */
            b += 62 + 8;
        } else {
            for (int i = 0; i < 16; i++)
                memcpy(b + 54 + i * 4, "\x10\x20\x30\x00", 4);
            b += 54 + 64;
        }
    }
    IStream *ds = NULL;
    CHECK(CreateStreamOnHGlobal(NULL, TRUE, &ds) == S_OK, "flux Delphi");
    ULONG wrote = 0;
    IStream_Write(ds, delphi, sizeof delphi, &wrote);
    IStream_Seek(ds, zero, STREAM_SEEK_SET, NULL);
    il2 = ImageList_Read(ds);
    CHECK(il2 != NULL && ImageList_GetImageCount(il2) == 1 && ImageList_GetIconSize(il2, &cx, &cy) && cx == 2,
          "ImageList_Read (flux Delphi, bfSize réduit aux en-têtes)");
    IStream_Release(ds);
    ImageList_Destroy(il2);
    ImageList_Destroy(il);
    DeleteDC(mem);
    DeleteObject(canvas);
    DeleteObject(bmp);
    ReleaseDC(NULL, screen);

    /* Recherche et remplacement : boîtes non modales (#712). */
    find_replace32();

    /* shell32. */
    APPBARDATA abd = { sizeof abd };
    CHECK(sizeof abd == 36, "APPBARDATA de 36 octets");
    CHECK(SHAppBarMessage(ABM_GETTASKBARPOS, &abd) && abd.uEdge == ABE_BOTTOM && abd.rc.bottom > abd.rc.top,
          "SHAppBarMessage (barre des tâches)");
    CHECK(SHAppBarMessage(ABM_GETSTATE, &abd) != 0, "SHAppBarMessage (état)");
    WCHAR self[MAX_PATH], exe[MAX_PATH];
    GetModuleFileNameW(NULL, self, MAX_PATH);
    CHECK((INT_PTR)FindExecutableW(self, NULL, exe) > 32 && lstrcmpiW(self, exe) == 0, "FindExecutableW (.exe)");
    SHFreeNameMappings(NULL);
    LPITEMIDLIST pidl = ILCreateFromPathW(self);
    CHECK(pidl != NULL, "ILCreateFromPathW");
    ILFree(pidl);
    SHChangeNotifyEntry ne = { NULL, FALSE };
    ULONG id = SHChangeNotifyRegister(GetDesktopWindow(), SHCNRF_ShellLevel, SHCNE_ALLEVENTS, WM_USER, 1, &ne);
    CHECK(id != 0 && SHChangeNotifyDeregister(id), "SHChangeNotifyRegister");
}

/* Contrôles sous-classés par une procédure x86, comme le panneau de
 * 7zFM : les messages LVM_* et CBEM_* passent par la procédure x86 puis
 * par CallWindowProc vers celle du contrôle (structures converties une
 * seule fois). */
static WNDPROC lv_old, cbex_old;
static int lv_seen, cbex_seen;

static LRESULT CALLBACK lv_sub(HWND h, UINT m, WPARAM w, LPARAM l)
{
    if (m == LVM_INSERTITEMW) {
        const LVITEMW *it = (const LVITEMW *)l;
        if (it->pszText && it->pszText != LPSTR_TEXTCALLBACKW && it->pszText[0] == L's')
            lv_seen++;
    }
    return CallWindowProcW(lv_old, h, m, w, l);
}

static LRESULT CALLBACK cbex_sub(HWND h, UINT m, WPARAM w, LPARAM l)
{
    if (m == CBEM_INSERTITEMW && ((const COMBOBOXEXITEMW *)l)->iItem == 0)
        cbex_seen++;
    return CallWindowProcW(cbex_old, h, m, w, l);
}

static void subclassed(void)
{
    HWND lv = control(WC_LISTVIEWW, LVS_REPORT);
    lv_old = (WNDPROC)SetWindowLongW(lv, GWL_WNDPROC, (LONG)lv_sub);
    CHECK(lv_old != NULL, "sous-classement de la liste");
    LVCOLUMNW col = {0};
    col.mask = LVCF_TEXT | LVCF_WIDTH;
    col.cx = 120;
    col.pszText = L"Nom";
    CHECK(SendMessageW(lv, LVM_INSERTCOLUMNW, 0, (LPARAM)&col) == 0, "colonne d'une liste sous-classée");
    LVITEMW it = {0};
    it.mask = LVIF_TEXT;
    it.pszText = L"sous-classe.txt";
    CHECK(SendMessageW(lv, LVM_INSERTITEMW, 0, (LPARAM)&it) == 0 && lv_seen == 1,
          "élément d'une liste sous-classée (structure x86 dans la procédure x86)");
    WCHAR text[64] = {0}, ctext[32] = {0};
    LVITEMW get = {0};
    get.pszText = text;
    get.cchTextMax = 64;
    CHECK(SendMessageW(lv, LVM_GETITEMTEXTW, 0, (LPARAM)&get) > 0 && wcscmp(text, L"sous-classe.txt") == 0,
          "texte d'une liste sous-classée");
    LVCOLUMNW gc = {0};
    gc.mask = LVCF_TEXT;
    gc.pszText = ctext;
    gc.cchTextMax = 32;
    CHECK(SendMessageW(lv, LVM_GETCOLUMNW, 0, (LPARAM)&gc) != 0 && wcscmp(ctext, L"Nom") == 0,
          "en-tête d'une liste sous-classée");
    DestroyWindow(lv);

    HWND cb = CreateWindowExW(0, WC_COMBOBOXEXW, L"", WS_POPUP | CBS_DROPDOWN, 0, 0, 300, 200,
                              NULL, NULL, NULL, NULL);
    CHECK(cb != NULL, "ComboBoxEx32");
    cbex_old = (WNDPROC)SetWindowLongW(cb, GWL_WNDPROC, (LONG)cbex_sub);
    COMBOBOXEXITEMW ci = {0};
    ci.mask = CBEIF_TEXT | CBEIF_IMAGE | CBEIF_SELECTEDIMAGE;
    ci.iItem = 0;
    ci.pszText = L"C:\\Users";
    ci.iImage = 0;
    ci.iSelectedImage = 1;
    CHECK(SendMessageW(cb, CBEM_INSERTITEMW, 0, (LPARAM)&ci) == 0 && cbex_seen == 1,
          "CBEM_INSERTITEMW d'une ComboBoxEx sous-classée");
    WCHAR ctx[32] = {0};
    COMBOBOXEXITEMW cg = {0};
    cg.mask = CBEIF_TEXT;
    cg.iItem = 0;
    cg.pszText = ctx;
    cg.cchTextMax = 32;
    CHECK(SendMessageW(cb, CBEM_GETITEMW, 0, (LPARAM)&cg) != 0 && wcscmp(ctx, L"C:\\Users") == 0,
          "texte d'une ComboBoxEx sous-classée");
    DestroyWindow(cb);
}

int main(void)
{
    INITCOMMONCONTROLSEX icc = { sizeof icc, ICC_WIN95_CLASSES | ICC_TAB_CLASSES };
    CHECK(InitCommonControlsEx(&icc), "InitCommonControlsEx");
    HIMAGELIST il = ImageList_Create(16, 16, ILC_COLOR32, 2, 2);
    CHECK(il != NULL && ImageList_GetImageCount(il) == 0, "ImageList_Create");
    ImageList_Destroy(il);
    list_view();
    subclassed();
    tree_view();
    tool_bar();
    tab_control();
    tab_superclass();
    tab_vcl();
    notifications();
    dialogs();
    winscp();
    printf("dlg32 : contrôles et boîtes communes 32 bits, %d vérifications passées. Tout est correct\n", step);
    return 0;
}

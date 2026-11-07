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
#include <windows.h>
#include <commctrl.h>
#include <commdlg.h>
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
    DestroyWindow(tab);
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

int main(void)
{
    INITCOMMONCONTROLSEX icc = { sizeof icc, ICC_WIN95_CLASSES | ICC_TAB_CLASSES };
    CHECK(InitCommonControlsEx(&icc), "InitCommonControlsEx");
    HIMAGELIST il = ImageList_Create(16, 16, ILC_COLOR32, 2, 2);
    CHECK(il != NULL && ImageList_GetImageCount(il) == 0, "ImageList_Create");
    ImageList_Destroy(il);
    list_view();
    tree_view();
    tool_bar();
    tab_control();
    dialogs();
    printf("dlg32 : contrôles et boîtes communes 32 bits, %d vérifications passées. Tout est correct\n", step);
    return 0;
}

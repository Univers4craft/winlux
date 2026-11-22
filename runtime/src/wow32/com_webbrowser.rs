//! Contrôle WebBrowser hébergé par un programme x86 (#2768).
//!
//! Description, pour les mandataires COM de `com.rs`, des interfaces que
//! sert `dlls/webbrowser.rs` (IOleObject, IOleInPlaceObject,
//! IWebBrowser2) et de celles du site que le programme lui donne
//! (IOleClientSite, IOleInPlaceSite). Ordre des méthodes d'après les
//! en-têtes publics du SDK Windows `oleidl.h` et `exdisp.h` (mêmes
//! déclarations dans mingw-w64). Sans elles, l'installeur de Creative
//! Cloud (x86) voyait IOleObject et IWebBrowser2 refusées et utilisait le
//! pointeur nul rendu.
//!
//! Conventions :
//! - HWND, LONG, BOOL, VARIANT_BOOL, OLECMDID, READYSTATE sont `I` ;
//! - LPCRECT, SIZEL*, CLSID*, long*, VARIANT_BOOL*, int* pointent des
//!   données sans pointeur : `I` ;
//! - HWND* et SHANDLE_PTR* ont la taille d'un pointeur : `OutPtr` ;
//! - BSTR passé est `I` (la chaîne est déjà en mémoire x86) ; BSTR* rendu
//!   est `OutPtr` (le tas du runtime est sous 4 Go) ;
//! - LPMSG de DoVerb : le runtime ne le lit pas, il passe tel quel (`I`) ;
//! - OLEINPLACEFRAMEINFO* de GetWindowContext : le runtime réserve 32
//!   octets et ne relit rien, le code x86 en écrit 20 : `I` ;
//! - les énumérateurs (EnumVerbs, EnumAdvise) ne sont pas décrits ;
//! - les méthodes d'IDispatch restent non décrites (automation à venir).

use super::com::{Iface, A, A::*, IDATAOBJECT, IDISPATCH, IMONIKER, IPERSIST, ISTREAM, IUNKNOWN};
use crate::dlls::comobj::{guid, Guid};

const NONE: Option<&[A]> = Some(&[]);
const ONE: Option<&[A]> = Some(&[I]);

const fn ole(d1: u32) -> Guid {
    guid(d1, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46])
}

/// IOleClientSite {00000118-0000-0000-C000-000000000046}.
pub(super) static IOLECLIENTSITE: Iface = Iface {
    name: "IOleClientSite",
    iid: ole(0x118),
    parent: Some(&IUNKNOWN),
    methods: &[
        NONE,                          // SaveObject
        Some(&[I, I, Out(&IMONIKER)]), // GetMoniker(dwAssign, dwWhich, ppmk)
        Some(&[Out(&IUNKNOWN)]),       // GetContainer(IOleContainer**)
        NONE,                          // ShowObject
        ONE,                           // OnShowWindow(fShow)
        NONE,                          // RequestNewObjectLayout
    ],
};

/// IOleWindow {00000114-0000-0000-C000-000000000046}.
pub(super) static IOLEWINDOW: Iface = Iface {
    name: "IOleWindow",
    iid: ole(0x114),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[OutPtr]), // GetWindow(HWND*)
        ONE,             // ContextSensitiveHelp(fEnterMode)
    ],
};

/// IOleInPlaceSite {00000119-0000-0000-C000-000000000046}.
pub(super) static IOLEINPLACESITE: Iface = Iface {
    name: "IOleInPlaceSite",
    iid: ole(0x119),
    parent: Some(&IOLEWINDOW),
    methods: &[
        NONE, // CanInPlaceActivate
        NONE, // OnInPlaceActivate
        NONE, // OnUIActivate
        // GetWindowContext(IOleInPlaceFrame**, IOleInPlaceUIWindow**,
        // LPRECT, LPRECT, LPOLEINPLACEFRAMEINFO)
        Some(&[Out(&IUNKNOWN), Out(&IUNKNOWN), I, I, I]),
        Some(&[Q]), // Scroll(SIZE)
        ONE,        // OnUIDeactivate(fUndoable)
        NONE,       // OnInPlaceDeactivate
        NONE,       // DiscardUndoState
        NONE,       // DeactivateAndUndo
        ONE,        // OnPosRectChange(LPCRECT)
    ],
};

/// IOleObject {00000112-0000-0000-C000-000000000046}.
pub(super) static IOLEOBJECT: Iface = Iface {
    name: "IOleObject",
    iid: ole(0x112),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[In(&IOLECLIENTSITE)]),    // SetClientSite
        Some(&[Out(&IOLECLIENTSITE)]),   // GetClientSite
        Some(&[I, I]),                   // SetHostNames(app, obj)
        ONE,                             // Close(dwSaveOption)
        Some(&[I, In(&IMONIKER)]),       // SetMoniker
        Some(&[I, I, Out(&IMONIKER)]),   // GetMoniker
        Some(&[In(&IDATAOBJECT), I, I]), // InitFromData
        Some(&[I, Out(&IDATAOBJECT)]),   // GetClipboardData
        // DoVerb(iVerb, lpmsg, pActiveSite, lindex, hwndParent, lprcPosRect)
        Some(&[I, I, In(&IOLECLIENTSITE), I, I, I]),
        None,                      // EnumVerbs(IEnumOLEVERB**)
        NONE,                      // Update
        NONE,                      // IsUpToDate
        ONE,                       // GetUserClassID(CLSID*)
        Some(&[I, OutPtr]),        // GetUserType(dwFormOfType, LPOLESTR*)
        Some(&[I, I]),             // SetExtent(dwDrawAspect, SIZEL*)
        Some(&[I, I]),             // GetExtent(dwDrawAspect, SIZEL*)
        Some(&[In(&IUNKNOWN), I]), // Advise(IAdviseSink*, DWORD*)
        ONE,                       // Unadvise(dwConnection)
        None,                      // EnumAdvise(IEnumSTATDATA**)
        Some(&[I, I]),             // GetMiscStatus(dwAspect, DWORD*)
        ONE,                       // SetColorScheme(LOGPALETTE*)
    ],
};

/// IOleInPlaceObject {00000113-0000-0000-C000-000000000046}.
pub(super) static IOLEINPLACEOBJECT: Iface = Iface {
    name: "IOleInPlaceObject",
    iid: ole(0x113),
    parent: Some(&IOLEWINDOW),
    methods: &[
        NONE,          // InPlaceDeactivate
        NONE,          // UIDeactivate
        Some(&[I, I]), // SetObjectRects(lprcPosRect, lprcClipRect)
        NONE,          // ReactivateAndUndo
    ],
};

/// IWebBrowser {EAB22AC1-30C1-11CF-A7EB-0000C05BAE0B}.
pub(super) static IWEBBROWSER: Iface = Iface {
    name: "IWebBrowser",
    iid: guid(
        0xEAB2_2AC1,
        0x30C1,
        0x11CF,
        [0xA7, 0xEB, 0x00, 0x00, 0xC0, 0x5B, 0xAE, 0x0B],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        NONE,                           // GoBack
        NONE,                           // GoForward
        NONE,                           // GoHome
        NONE,                           // GoSearch
        Some(&[I, Var, Var, Var, Var]), // Navigate(URL, Flags, Target, PostData, Headers)
        NONE,                           // Refresh
        Some(&[Var]),                   // Refresh2(Level)
        NONE,                           // Stop
        Some(&[Out(&IDISPATCH)]),       // get_Application
        Some(&[Out(&IDISPATCH)]),       // get_Parent
        Some(&[Out(&IDISPATCH)]),       // get_Container
        Some(&[Out(&IDISPATCH)]),       // get_Document
        ONE,                            // get_TopLevelContainer(VARIANT_BOOL*)
        Some(&[OutPtr]),                // get_Type(BSTR*)
        ONE,                            // get_Left(long*)
        ONE,                            // put_Left
        ONE,                            // get_Top(long*)
        ONE,                            // put_Top
        ONE,                            // get_Width(long*)
        ONE,                            // put_Width
        ONE,                            // get_Height(long*)
        ONE,                            // put_Height
        Some(&[OutPtr]),                // get_LocationName(BSTR*)
        Some(&[OutPtr]),                // get_LocationURL(BSTR*)
        ONE,                            // get_Busy(VARIANT_BOOL*)
    ],
};

/// IWebBrowserApp {0002DF05-0000-0000-C000-000000000046}.
pub(super) static IWEBBROWSERAPP: Iface = Iface {
    name: "IWebBrowserApp",
    iid: ole(0x2_DF05),
    parent: Some(&IWEBBROWSER),
    methods: &[
        NONE,              // Quit
        Some(&[I, I]),     // ClientToWindow(int*, int*)
        Some(&[I, VarIn]), // PutProperty(BSTR, VARIANT)
        Some(&[I, Var]),   // GetProperty(BSTR, VARIANT*)
        Some(&[OutPtr]),   // get_Name(BSTR*)
        Some(&[OutPtr]),   // get_HWND(SHANDLE_PTR*)
        Some(&[OutPtr]),   // get_FullName(BSTR*)
        Some(&[OutPtr]),   // get_Path(BSTR*)
        ONE,               // get_Visible
        ONE,               // put_Visible
        ONE,               // get_StatusBar
        ONE,               // put_StatusBar
        Some(&[OutPtr]),   // get_StatusText(BSTR*)
        ONE,               // put_StatusText(BSTR)
        ONE,               // get_ToolBar(int*)
        ONE,               // put_ToolBar
        ONE,               // get_MenuBar
        ONE,               // put_MenuBar
        ONE,               // get_FullScreen
        ONE,               // put_FullScreen
    ],
};

/// IWebBrowser2 {D30C1661-CDAF-11D0-8A3E-00C04FC9E26E}.
pub(super) static IWEBBROWSER2: Iface = Iface {
    name: "IWebBrowser2",
    iid: guid(
        0xD30C_1661,
        0xCDAF,
        0x11D0,
        [0x8A, 0x3E, 0x00, 0xC0, 0x4F, 0xC9, 0xE2, 0x6E],
    ),
    parent: Some(&IWEBBROWSERAPP),
    methods: &[
        Some(&[Var, Var, Var, Var, Var]), // Navigate2(URL, Flags, Target, PostData, Headers)
        Some(&[I, I]),                    // QueryStatusWB(cmdID, OLECMDF*)
        Some(&[I, I, Var, Var]),          // ExecWB(cmdID, cmdexecopt, pvaIn, pvaOut)
        Some(&[Var, Var, Var]),           // ShowBrowserBar(pvaClsid, pvarShow, pvarSize)
        ONE,                              // get_ReadyState(READYSTATE*)
        ONE,                              // get_Offline
        ONE,                              // put_Offline
        ONE,                              // get_Silent
        ONE,                              // put_Silent
        ONE,                              // get_RegisterAsBrowser
        ONE,                              // put_RegisterAsBrowser
        ONE,                              // get_RegisterAsDropTarget
        ONE,                              // put_RegisterAsDropTarget
        ONE,                              // get_TheaterMode
        ONE,                              // put_TheaterMode
        ONE,                              // get_AddressBar
        ONE,                              // put_AddressBar
        ONE,                              // get_Resizable
        ONE,                              // put_Resizable
    ],
};

/// IAdviseSink {0000010F-0000-0000-C000-000000000046} (objidl.h, #2971) :
/// abonné x86 aux changements de vue, passé à IViewObject::SetAdvise. Ses
/// méthodes ne rendent rien. OnDataChange (FORMATETC et STGMEDIUM, qui
/// contiennent des pointeurs) n'est pas décrite : appelée par le runtime,
/// elle rendrait E_NOTIMPL au lieu de lire une disposition fausse.
pub(super) static IADVISESINK: Iface = Iface {
    name: "IAdviseSink",
    iid: ole(0x10F),
    parent: Some(&IUNKNOWN),
    methods: &[
        None,                   // OnDataChange(FORMATETC*, STGMEDIUM*)
        Some(&[I, I]),          // OnViewChange(dwAspect, lindex)
        Some(&[In(&IMONIKER)]), // OnRename(IMoniker*)
        NONE,                   // OnSave
        NONE,                   // OnClose
    ],
};

/// IViewObject {0000010D-0000-0000-C000-000000000046} (oleidl.h, #2971) :
/// l'hôte ATL la demande au contrôle après SetClientSite. pvAspect et
/// DVTARGETDEVICE* sont des données sans pointeur, HDC des handles : `I`.
/// Draw reçoit `pfnContinue`, adresse d'une fonction x86 que le runtime ne
/// saurait pas appeler : elle passe comme un entier, ce qui n'est sûr que
/// tant que Draw du runtime ne l'appelle pas (dlls/webbrowser.rs : Draw,
/// GetColorSet, Freeze, Unfreeze et GetAdvise rendent E_NOTIMPL ; SetAdvise
/// accepte sans garder l'abonné).
pub(super) static IVIEWOBJECT: Iface = Iface {
    name: "IViewObject",
    iid: ole(0x10D),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I, I, I, I, I, I, I, I, I, I]), // Draw
        Some(&[I, I, I, I, I, OutPtr]),        // GetColorSet(…, LOGPALETTE**)
        Some(&[I, I, I, I]),                   // Freeze(…, DWORD*)
        ONE,                                   // Unfreeze(dwFreeze)
        Some(&[I, I, In(&IADVISESINK)]),       // SetAdvise(aspects, advf, pAdvSink)
        Some(&[I, I, Out(&IADVISESINK)]),      // GetAdvise(DWORD*, DWORD*, ppAdvSink)
    ],
};

/// IPersistStreamInit {7FD52380-4E07-101B-AE2D-08002B2EC713} (ocidl.h,
/// #2970) : l'hôte ATL initialise le contrôle par InitNew ou Load juste
/// après SetClientSite. ULARGE_INTEGER* de GetSizeMax pointe des données
/// sans pointeur : `I`.
pub(super) static IPERSISTSTREAMINIT: Iface = Iface {
    name: "IPersistStreamInit",
    iid: guid(
        0x7FD5_2380,
        0x4E07,
        0x101B,
        [0xAE, 0x2D, 0x08, 0x00, 0x2B, 0x2E, 0xC7, 0x13],
    ),
    parent: Some(&IPERSIST),
    methods: &[
        NONE,                     // IsDirty
        Some(&[In(&ISTREAM)]),    // Load(IStream*)
        Some(&[In(&ISTREAM), I]), // Save(IStream*, fClearDirty)
        ONE,                      // GetSizeMax(ULARGE_INTEGER*)
        NONE,                     // InitNew
    ],
};

pub(super) static ALL: &[&Iface] = &[
    &IOLECLIENTSITE,
    &IOLEWINDOW,
    &IOLEINPLACESITE,
    &IOLEOBJECT,
    &IOLEINPLACEOBJECT,
    &IWEBBROWSER,
    &IWEBBROWSERAPP,
    &IWEBBROWSER2,
    &IADVISESINK,
    &IVIEWOBJECT,
    &IPERSISTSTREAMINIT,
];

#[cfg(test)]
mod tests {
    use super::super::com::by_iid;
    use super::*;

    /// #2768 : chaque interface du contrôle WebBrowser et de son site est
    /// décrite en entier, avec autant de méthodes que les tables du
    /// runtime (dlls/webbrowser.rs : WB_VT 71, OO_VT 24, IP_VT 9), et se
    /// retrouve par son IID.
    #[test]
    fn controle_webbrowser_decrit_pour_le_code_x86() {
        for (f, n) in [
            (&IOLECLIENTSITE, 9),
            (&IOLEWINDOW, 5),
            (&IOLEINPLACESITE, 15),
            (&IOLEOBJECT, 24),
            (&IOLEINPLACEOBJECT, 9),
            (&IWEBBROWSER, 32),
            (&IWEBBROWSERAPP, 52),
            (&IWEBBROWSER2, 71),
        ] {
            assert_eq!(f.count(), n, "{}", f.name);
            assert!(
                by_iid(&f.iid).is_some_and(|g| std::ptr::eq(g, f)),
                "{}",
                f.name
            );
        }
        // Seuls les énumérateurs d'IOleObject restent non décrits.
        let vides = (3..IOLEOBJECT.count())
            .filter(|&i| IOLEOBJECT.method(i).is_none())
            .count();
        assert_eq!(vides, 2);
        // DoVerb reçoit un vrai site x86 ; GetWindow rend un HWND.
        assert!(
            matches!(IOLEOBJECT.method(11), Some([I, I, In(s), I, I, I]) if std::ptr::eq(*s, &IOLECLIENTSITE))
        );
        assert!(matches!(IOLEWINDOW.method(3), Some([OutPtr])));
        // Navigate2 : cinq VARIANT* ; get_HWND rend un pointeur.
        assert!(matches!(
            IWEBBROWSER2.method(52),
            Some([Var, Var, Var, Var, Var])
        ));
        assert!(matches!(IWEBBROWSER2.method(37), Some([OutPtr])));
    }

    /// #2971 : IViewObject se retrouve par son IID, avec les neuf entrées
    /// de IUnknown + IViewObject et la taille de pile de chaque méthode
    /// (oleidl.h : Draw prend dix arguments de 4 octets en x86).
    #[test]
    fn view_object_decrit_2971() {
        use super::super::com::slots;
        assert_eq!(IVIEWOBJECT.count(), 9);
        assert_eq!(IADVISESINK.count(), 8);
        for f in [&IVIEWOBJECT, &IADVISESINK] {
            assert!(
                by_iid(&f.iid).is_some_and(|g| std::ptr::eq(g, f)),
                "{}",
                f.name
            );
        }
        let pile: Vec<usize> = (3..9)
            .map(|i| slots(IVIEWOBJECT.method(i).unwrap()))
            .collect();
        assert_eq!(pile, [10, 6, 4, 1, 3, 3]);
        assert!(
            matches!(IVIEWOBJECT.method(7), Some([I, I, In(s)]) if std::ptr::eq(*s, &IADVISESINK))
        );
        assert!(
            matches!(IVIEWOBJECT.method(8), Some([I, I, Out(s)]) if std::ptr::eq(*s, &IADVISESINK))
        );
        // OnDataChange reste non décrite ; OnViewChange prend deux cases.
        assert!(IADVISESINK.method(3).is_none());
        assert_eq!(slots(IADVISESINK.method(4).unwrap()), 2);
    }

    /// #2970 : IPersistStreamInit, comme la table du runtime
    /// (dlls/webbrowser.rs : PS_VT, 9 entrées), se retrouve par son IID ;
    /// Load reçoit le flux x86 de l'hôte par un mandataire.
    #[test]
    fn persist_stream_init_decrit_2970() {
        assert_eq!(IPERSISTSTREAMINIT.count(), 9);
        assert!(
            by_iid(&IPERSISTSTREAMINIT.iid).is_some_and(|g| std::ptr::eq(g, &IPERSISTSTREAMINIT))
        );
        for i in 3..9 {
            assert!(IPERSISTSTREAMINIT.method(i).is_some(), "méthode {i}");
        }
        assert!(
            matches!(IPERSISTSTREAMINIT.method(3), Some([I])),
            "GetClassID"
        );
        assert!(
            matches!(IPERSISTSTREAMINIT.method(5), Some([In(s)]) if std::ptr::eq(*s, &ISTREAM))
        );
        assert!(matches!(IPERSISTSTREAMINIT.method(6), Some([In(_), I])));
        assert!(matches!(IPERSISTSTREAMINIT.method(8), Some([])), "InitNew");
    }
}

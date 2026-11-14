//! Activation audio asynchrone (ActivateAudioInterfaceAsync) pour les
//! programmes x86 (#2428).
//!
//! Description des interfaces de `mmdeviceapi.h` (mingw-w64, domaine
//! public) pour les mandataires COM de `com.rs`. L'objet lui-même est celui
//! de `dlls/lot_f_divers.rs`.
//!
//! GetActivateResult rend une interface du type demandé à l'activation,
//! qu'aucune case de la méthode ne nomme : l'objet la convertit lui-même
//! par `iface_to32` et la méthode la rend comme une valeur de la taille
//! d'un pointeur (`OutPtr`).

use super::com::{by_iid, to32, Iface, A::*, IUNKNOWN};
use crate::dlls::comobj::Guid;
use crate::dlls::lot_f_divers as act;

/// IActivateAudioInterfaceAsyncOperation, objet du runtime.
pub(super) static IACTIVATEAUDIOINTERFACEASYNCOPERATION: Iface = Iface {
    name: "IActivateAudioInterfaceAsyncOperation",
    iid: act::IID_IACTIVATEAUDIOINTERFACEASYNCOPERATION,
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I, OutPtr]), // GetActivateResult (HRESULT*, IUnknown**)
    ],
};

/// IActivateAudioInterfaceCompletionHandler, écrit dans le programme.
pub(super) static IACTIVATEAUDIOINTERFACECOMPLETIONHANDLER: Iface = Iface {
    name: "IActivateAudioInterfaceCompletionHandler",
    iid: act::IID_IACTIVATEAUDIOINTERFACECOMPLETIONHANDLER,
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[In(&IACTIVATEAUDIOINTERFACEASYNCOPERATION)]), // ActivateCompleted
    ],
};

pub(super) static ALL: &[&Iface] = &[
    &IACTIVATEAUDIOINTERFACEASYNCOPERATION,
    &IACTIVATEAUDIOINTERFACECOMPLETIONHANDLER,
];

/// Interface 64 bits `v` (une référence comprise) rendue au code x86 sous
/// le type `iid`. 0 si ce type n'est pas décrit (la référence est rendue).
pub(crate) fn iface_to32(v: usize, iid: &Guid) -> u32 {
    to32(v as u64, by_iid(iid), iid)
}

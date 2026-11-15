//! Media Foundation, chaîne de lecture, pour les programmes x86 (#2428,
//! lot 4).
//!
//! Description des interfaces de `mfobjects.h`, `mfidl.h` et
//! `mfreadwrite.h` (mingw-w64, domaine public) pour les mandataires COM de
//! `com.rs`, et des exports de mfplat, mf et mfreadwrite qui les rendent :
//! lecteur de source (`dlls/mfreader.rs`, `dlls/mfcapture.rs`), sources de
//! caméra et énumération des transformations. Les autres exports de la
//! chaîne (session, topologie, résolveur, écrivain, renderers) rendent un
//! refus sans objet : ils n'ont rien à décrire.
//!
//! Mêmes conventions que `com_mf.rs`. Un GUID passé par valeur (catégorie
//! de MFTEnum*) occupe quatre cases de la pile x86 (`GuidIn`).

use super::com::{Iface, A, A::*, IUNKNOWN};
use super::com_mf::{
    IMFASYNCCALLBACK, IMFASYNCRESULT, IMFATTRIBUTES, IMFMEDIAEVENT, IMFMEDIATYPE,
    IMFPRESENTATIONDESCRIPTOR, IMFSAMPLE,
};
use crate::dlls::comobj::{guid, Guid};

const NONE: Option<&[A]> = Some(&[]);
const ONE: Option<&[A]> = Some(&[I]);
const TWO: Option<&[A]> = Some(&[I, I]);

pub(crate) const IID_IMFACTIVATE: Guid = guid(
    0x7fee_9e9a,
    0x4a89,
    0x47a6,
    [0x89, 0x9c, 0xb6, 0xa5, 0x3a, 0x70, 0xfb, 0x67],
);

/// IMFActivate : attributs, puis l'objet qu'il crée (source de caméra).
pub(super) static IMFACTIVATE: Iface = Iface {
    name: "IMFActivate",
    iid: IID_IMFACTIVATE,
    parent: Some(&IMFATTRIBUTES),
    methods: &[
        Some(&[Iid, OutQ]), // ActivateObject
        NONE,               // ShutdownObject
        NONE,               // DetachObject
    ],
};

pub(super) static IMFMEDIAEVENTGENERATOR: Iface = Iface {
    name: "IMFMediaEventGenerator",
    iid: guid(
        0x2cd0_bd52,
        0xbcd5,
        0x4b89,
        [0xb6, 0x2c, 0xea, 0xdc, 0x0c, 0x03, 0x1e, 0x7d],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I, Out(&IMFMEDIAEVENT)]),                   // GetEvent
        Some(&[In(&IMFASYNCCALLBACK), In(&IUNKNOWN)]),     // BeginGetEvent
        Some(&[In(&IMFASYNCRESULT), Out(&IMFMEDIAEVENT)]), // EndGetEvent
        Some(&[I, I, I, Var]),                             // QueueEvent
    ],
};

pub(super) static IMFMEDIASOURCE: Iface = Iface {
    name: "IMFMediaSource",
    iid: guid(
        0x279a_808d,
        0xaec7,
        0x40c8,
        [0x9c, 0x6b, 0xa6, 0xb4, 0x92, 0xc7, 0x8a, 0x66],
    ),
    parent: Some(&IMFMEDIAEVENTGENERATOR),
    methods: &[
        ONE,                                             // GetCharacteristics
        Some(&[Out(&IMFPRESENTATIONDESCRIPTOR)]),        // CreatePresentationDescriptor
        Some(&[In(&IMFPRESENTATIONDESCRIPTOR), I, Var]), // Start
        NONE,                                            // Stop
        NONE,                                            // Pause
        NONE,                                            // Shutdown
    ],
};

pub(super) static IMFSOURCEREADER: Iface = Iface {
    name: "IMFSourceReader",
    iid: guid(
        0x70ae_66f2,
        0xc809,
        0x4e4f,
        [0x89, 0x15, 0xbd, 0xcb, 0x40, 0x6b, 0x79, 0x93],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        TWO,                                     // GetStreamSelection
        TWO,                                     // SetStreamSelection
        Some(&[I, I, Out(&IMFMEDIATYPE)]),       // GetNativeMediaType
        Some(&[I, Out(&IMFMEDIATYPE)]),          // GetCurrentMediaType
        Some(&[I, I, In(&IMFMEDIATYPE)]),        // SetCurrentMediaType
        Some(&[I, Var]),                         // SetCurrentPosition
        Some(&[I, I, I, I, I, Out(&IMFSAMPLE)]), // ReadSample
        ONE,                                     // Flush
        Some(&[I, I, Iid, OutQ]),                // GetServiceForStream
        Some(&[I, I, Var]),                      // GetPresentationAttribute
    ],
};

/// Rappel du mode asynchrone, écrit par le programme x86 et appelé par le
/// fil du lecteur (mandataire `P64`) ; LONGLONG llTimestamp : deux cases.
pub(super) static IMFSOURCEREADERCALLBACK: Iface = Iface {
    name: "IMFSourceReaderCallback",
    iid: guid(
        0xdeec_8d99,
        0xfa1d,
        0x4d82,
        [0x84, 0xc2, 0x2c, 0x89, 0x69, 0x94, 0x48, 0x67],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I, I, I, Q, In(&IMFSAMPLE)]), // OnReadSample
        ONE,                                 // OnFlush
        Some(&[I, In(&IMFMEDIAEVENT)]),      // OnEvent
    ],
};

pub(super) static ALL: &[&Iface] = &[
    &IMFACTIVATE,
    &IMFMEDIAEVENTGENERATOR,
    &IMFMEDIASOURCE,
    &IMFSOURCEREADER,
    &IMFSOURCEREADERCALLBACK,
];

/// Liste d'arguments statique (les statiques peuvent pointer sur les
/// descriptions d'interfaces, pas les temporaires).
macro_rules! k {
    ($($e:expr),*) => {{
        static K: &[A] = &[$($e),*];
        K
    }};
}

/// Arguments des exports de la chaîne (mfapi.h, mfidl.h, mfreadwrite.h),
/// servis par mfplat, mf et mfreadwrite.
pub(super) fn api_kinds(func: &str) -> Option<&'static [A]> {
    Some(match func {
        "MFCreateSourceReaderFromURL" => k!(I, In(&IMFATTRIBUTES), Out(&IMFSOURCEREADER)),
        "MFCreateSourceReaderFromMediaSource" => k!(
            In(&IMFMEDIASOURCE),
            In(&IMFATTRIBUTES),
            Out(&IMFSOURCEREADER)
        ),
        "MFCreateDeviceSource" => k!(In(&IMFATTRIBUTES), Out(&IMFMEDIASOURCE)),
        // IMFActivate*** : le tableau rendu (CoTaskMemAlloc, sous 4 Go) est
        // déjà fait de mandataires x86 (mfplat::mf_enum_device_sources).
        "MFEnumDeviceSources" => k!(In(&IMFATTRIBUTES), OutPtr, I),
        // GUID guidCategory passé par valeur. Les listes rendues sont vides
        // (aucune transformation servie) : `OutPtr` rend le NULL.
        "MFTEnum" => k!(GuidIn, I, I, I, In(&IMFATTRIBUTES), OutPtr, I),
        "MFTEnumEx" => k!(GuidIn, I, I, I, OutPtr, I),
        "MFTEnum2" => k!(GuidIn, I, I, I, In(&IMFATTRIBUTES), OutPtr, I),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Méthodes propres comptées dans mfobjects.h, mfidl.h et mfreadwrite.h.
    #[test]
    fn interfaces_completes() {
        for (f, n) in [
            (&IMFACTIVATE, 3),
            (&IMFMEDIAEVENTGENERATOR, 4),
            (&IMFMEDIASOURCE, 6),
            (&IMFSOURCEREADER, 10),
            (&IMFSOURCEREADERCALLBACK, 3),
        ] {
            assert_eq!(f.methods.len(), n, "{}", f.name);
            assert!(f.methods.iter().all(|m| m.is_some()), "{}", f.name);
        }
    }

    #[test]
    fn exports_decrits() {
        for f in [
            "MFCreateSourceReaderFromURL",
            "MFCreateSourceReaderFromMediaSource",
            "MFCreateDeviceSource",
            "MFEnumDeviceSources",
            "MFTEnum",
            "MFTEnumEx",
            "MFTEnum2",
        ] {
            assert!(api_kinds(f).is_some(), "{f}");
        }
    }
}

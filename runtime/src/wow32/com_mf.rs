//! Media Foundation, objets de base, pour les programmes x86 (#2428, lot 3).
//!
//! Description des interfaces de `mfobjects.h` et `mfidl.h` (mingw-w64,
//! domaine public) pour les mandataires COM de `com.rs`, et des exports de
//! mfplat qui rendent ou reçoivent ces objets. Les objets eux-mêmes sont
//! ceux de `dlls/mfcapture.rs`, `dlls/mfplat.rs` et `dlls/lot_f_mfplat.rs`.
//! La chaîne (session, topologie, lecteur, écrivain, résolveur) est décrite
//! à part ; les exports liés à DXGI rendent E_NOTIMPL et ne sortent aucun
//! objet.
//!
//! Conventions : un REFGUID, un GUID*, un pointeur vers des données sans
//! pointeur et un DWORD rendu sont `I` ; un UINT64, un LONGLONG et un double
//! passés par valeur occupent deux cases de la pile x86 (`Q`, le double
//! arrive aussi dans xmm, voir `winlux_wow_call64`) ; un PROPVARIANT* (16
//! octets en x86, 24 en x64) est `Var`, BLOB compris ; une chaîne ou un
//! bloc alloués par CoTaskMemAlloc et le pointeur rendu par
//! IMFMediaBuffer::Lock sont `OutPtr` (le tas d'un processus x86 est sous
//! 4 Go).

use super::com::{Iface, A, A::*, ISTREAM, IUNKNOWN};
use crate::dlls::comobj::{guid, Guid};

const NONE: Option<&[A]> = Some(&[]);
const ONE: Option<&[A]> = Some(&[I]);
const TWO: Option<&[A]> = Some(&[I, I]);

const fn mf(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    guid(d1, d2, d3, d4)
}

pub(super) static IMFATTRIBUTES: Iface = Iface {
    name: "IMFAttributes",
    iid: mf(
        0x2cd2_d921,
        0xc447,
        0x44a7,
        [0xa1, 0x3c, 0x4a, 0xda, 0xbf, 0xc2, 0x47, 0xe3],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I, Var]),                   // GetItem
        TWO,                               // GetItemType
        Some(&[I, Var, I]),                // CompareItem
        Some(&[In(&IMFATTRIBUTES), I, I]), // Compare
        TWO,                               // GetUINT32
        TWO,                               // GetUINT64
        TWO,                               // GetDouble
        TWO,                               // GetGUID
        TWO,                               // GetStringLength
        Some(&[I, I, I, I]),               // GetString
        Some(&[I, OutPtr, I]),             // GetAllocatedString
        TWO,                               // GetBlobSize
        Some(&[I, I, I, I]),               // GetBlob
        Some(&[I, OutPtr, I]),             // GetAllocatedBlob
        Some(&[I, Iid, OutQ]),             // GetUnknown
        Some(&[I, Var]),                   // SetItem
        ONE,                               // DeleteItem
        NONE,                              // DeleteAllItems
        TWO,                               // SetUINT32
        Some(&[I, Q]),                     // SetUINT64
        Some(&[I, Q]),                     // SetDouble
        TWO,                               // SetGUID
        TWO,                               // SetString
        Some(&[I, I, I]),                  // SetBlob
        Some(&[I, In(&IUNKNOWN)]),         // SetUnknown
        NONE,                              // LockStore
        NONE,                              // UnlockStore
        ONE,                               // GetCount
        Some(&[I, I, Var]),                // GetItemByIndex
        Some(&[In(&IMFATTRIBUTES)]),       // CopyAllItems
    ],
};

pub(super) static IMFMEDIATYPE: Iface = Iface {
    name: "IMFMediaType",
    iid: mf(
        0x44ae_0fa8,
        0xea31,
        0x4109,
        [0x8d, 0x2e, 0x4c, 0xae, 0x49, 0x97, 0xc5, 0x55],
    ),
    parent: Some(&IMFATTRIBUTES),
    methods: &[
        ONE,                           // GetMajorType
        ONE,                           // IsCompressedFormat
        Some(&[In(&IMFMEDIATYPE), I]), // IsEqual
        // GetRepresentation et FreeRepresentation rendent des structures
        // (AM_MEDIA_TYPE…) dont la disposition change en x86 : non décrites.
        None,
        None,
    ],
};

pub(super) static IMFMEDIABUFFER: Iface = Iface {
    name: "IMFMediaBuffer",
    iid: mf(
        0x045f_a593,
        0x8799,
        0x42b8,
        [0xbc, 0x8d, 0x89, 0x68, 0xc6, 0x45, 0x35, 0x07],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[OutPtr, I, I]), // Lock
        NONE,                  // Unlock
        ONE,                   // GetCurrentLength
        ONE,                   // SetCurrentLength
        ONE,                   // GetMaxLength
    ],
};

pub(super) static IMFSAMPLE: Iface = Iface {
    name: "IMFSample",
    iid: mf(
        0xc40a_00f2,
        0xb93a,
        0x4d80,
        [0xae, 0x8c, 0x5a, 0x1c, 0x63, 0x4f, 0x58, 0xe4],
    ),
    parent: Some(&IMFATTRIBUTES),
    methods: &[
        ONE,                              // GetSampleFlags
        ONE,                              // SetSampleFlags
        ONE,                              // GetSampleTime
        Some(&[Q]),                       // SetSampleTime
        ONE,                              // GetSampleDuration
        Some(&[Q]),                       // SetSampleDuration
        ONE,                              // GetBufferCount
        Some(&[I, Out(&IMFMEDIABUFFER)]), // GetBufferByIndex
        Some(&[Out(&IMFMEDIABUFFER)]),    // ConvertToContiguousBuffer
        Some(&[In(&IMFMEDIABUFFER)]),     // AddBuffer
        ONE,                              // RemoveBufferByIndex
        NONE,                             // RemoveAllBuffers
        ONE,                              // GetTotalLength
        Some(&[In(&IMFMEDIABUFFER)]),     // CopyToBuffer
    ],
};

pub(super) static IMFCOLLECTION: Iface = Iface {
    name: "IMFCollection",
    iid: mf(
        0x5bc8_a76b,
        0x869a,
        0x46a3,
        [0x9b, 0x03, 0xfa, 0x21, 0x8a, 0x66, 0xae, 0xbe],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        ONE,                        // GetElementCount
        Some(&[I, Out(&IUNKNOWN)]), // GetElement
        Some(&[In(&IUNKNOWN)]),     // AddElement
        Some(&[I, Out(&IUNKNOWN)]), // RemoveElement
        Some(&[I, In(&IUNKNOWN)]),  // InsertElementAt
        NONE,                       // RemoveAllElements
    ],
};

pub(super) static IMFMEDIAEVENT: Iface = Iface {
    name: "IMFMediaEvent",
    iid: mf(
        0xdf59_8932,
        0xf10c,
        0x4e39,
        [0xbb, 0xa2, 0xc3, 0x08, 0xf1, 0x01, 0xda, 0xa3],
    ),
    parent: Some(&IMFATTRIBUTES),
    methods: &[
        ONE,          // GetType
        ONE,          // GetExtendedType
        ONE,          // GetStatus
        Some(&[Var]), // GetValue
    ],
};

pub(super) static IMFASYNCRESULT: Iface = Iface {
    name: "IMFAsyncResult",
    iid: mf(
        0xac6b_7889,
        0x0740,
        0x4d51,
        [0x86, 0x19, 0x90, 0x59, 0x94, 0xa5, 0x5c, 0xc6],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[Out(&IUNKNOWN)]), // GetState
        NONE,                    // GetStatus
        ONE,                     // SetStatus
        Some(&[Out(&IUNKNOWN)]), // GetObject
        // GetStateNoAddRef rend l'interface comme valeur de retour, ce que
        // les mandataires ne savent pas faire pour une méthode : non décrite.
        None,
    ],
};

/// Rappel écrit par le programme x86, appelé par le runtime (mandataire
/// `P64`).
pub(super) static IMFASYNCCALLBACK: Iface = Iface {
    name: "IMFAsyncCallback",
    iid: mf(
        0xa270_03cf,
        0x2354,
        0x4f2a,
        [0x8d, 0x6a, 0xab, 0x7c, 0xff, 0x15, 0x43, 0x7e],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        TWO,                          // GetParameters
        Some(&[In(&IMFASYNCRESULT)]), // Invoke
    ],
};

pub(super) static IMFMEDIAEVENTQUEUE: Iface = Iface {
    name: "IMFMediaEventQueue",
    iid: mf(
        0x36f8_46fc,
        0x2256,
        0x48b6,
        [0xb5, 0x8e, 0xe2, 0xb6, 0x38, 0x31, 0x65, 0x81],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I, Out(&IMFMEDIAEVENT)]),                   // GetEvent
        Some(&[In(&IMFASYNCCALLBACK), In(&IUNKNOWN)]),     // BeginGetEvent
        Some(&[In(&IMFASYNCRESULT), Out(&IMFMEDIAEVENT)]), // EndGetEvent
        Some(&[In(&IMFMEDIAEVENT)]),                       // QueueEvent
        Some(&[I, I, I, Var]),                             // QueueEventParamVar
        Some(&[I, I, I, In(&IUNKNOWN)]),                   // QueueEventParamUnk
        NONE,                                              // Shutdown
    ],
};

pub(super) static IMFMEDIATYPEHANDLER: Iface = Iface {
    name: "IMFMediaTypeHandler",
    iid: mf(
        0xe93d_cf6c,
        0x4b07,
        0x4e1e,
        [0x81, 0x23, 0xaa, 0x16, 0xed, 0x6e, 0xad, 0xf5],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[In(&IMFMEDIATYPE), Out(&IMFMEDIATYPE)]), // IsMediaTypeSupported
        ONE,                                            // GetMediaTypeCount
        Some(&[I, Out(&IMFMEDIATYPE)]),                 // GetMediaTypeByIndex
        Some(&[In(&IMFMEDIATYPE)]),                     // SetCurrentMediaType
        Some(&[Out(&IMFMEDIATYPE)]),                    // GetCurrentMediaType
        ONE,                                            // GetMajorType
    ],
};

pub(super) static IMFSTREAMDESCRIPTOR: Iface = Iface {
    name: "IMFStreamDescriptor",
    iid: mf(
        0x56c0_3d9c,
        0x9dbb,
        0x45f5,
        [0xab, 0x4b, 0xd8, 0x0f, 0x47, 0xc0, 0x59, 0x38],
    ),
    parent: Some(&IMFATTRIBUTES),
    methods: &[
        ONE,                                // GetStreamIdentifier
        Some(&[Out(&IMFMEDIATYPEHANDLER)]), // GetMediaTypeHandler
    ],
};

pub(super) static IMFPRESENTATIONDESCRIPTOR: Iface = Iface {
    name: "IMFPresentationDescriptor",
    iid: mf(
        0x03cb_2711,
        0x24d7,
        0x4db6,
        [0xa1, 0x7f, 0xf3, 0xa7, 0xa4, 0x79, 0xa5, 0x36],
    ),
    parent: Some(&IMFATTRIBUTES),
    methods: &[
        ONE,                                      // GetStreamDescriptorCount
        Some(&[I, I, Out(&IMFSTREAMDESCRIPTOR)]), // GetStreamDescriptorByIndex
        ONE,                                      // SelectStream
        ONE,                                      // DeselectStream
        Some(&[Out(&IMFPRESENTATIONDESCRIPTOR)]), // Clone
    ],
};

pub(super) static ALL: &[&Iface] = &[
    &IMFATTRIBUTES,
    &IMFMEDIATYPE,
    &IMFMEDIABUFFER,
    &IMFSAMPLE,
    &IMFCOLLECTION,
    &IMFMEDIAEVENT,
    &IMFASYNCRESULT,
    &IMFASYNCCALLBACK,
    &IMFMEDIAEVENTQUEUE,
    &IMFMEDIATYPEHANDLER,
    &IMFSTREAMDESCRIPTOR,
    &IMFPRESENTATIONDESCRIPTOR,
];

/// Liste d'arguments statique (les statiques peuvent pointer sur les
/// descriptions d'interfaces, pas les temporaires).
macro_rules! k {
    ($($e:expr),*) => {{
        static K: &[A] = &[$($e),*];
        K
    }};
}

/// Arguments des exports de mfplat qui rendent ou reçoivent un objet de
/// base (mfapi.h).
pub(super) fn api_kinds(func: &str) -> Option<&'static [A]> {
    Some(match func {
        "MFCreateAttributes" => k!(Out(&IMFATTRIBUTES), I),
        "MFCreateMediaType" => k!(Out(&IMFMEDIATYPE)),
        "MFCreateSample" => k!(Out(&IMFSAMPLE)),
        "MFCreateMemoryBuffer" => k!(I, Out(&IMFMEDIABUFFER)),
        "MFCreateAlignedMemoryBuffer" => k!(I, I, Out(&IMFMEDIABUFFER)),
        "MFCreateMediaBufferWrapper" => {
            k!(In(&IMFMEDIABUFFER), I, I, Out(&IMFMEDIABUFFER))
        }
        // LONGLONG llDuration : deux cases de la pile x86.
        "MFCreateMediaBufferFromMediaType" => {
            k!(In(&IMFMEDIATYPE), Q, I, I, Out(&IMFMEDIABUFFER))
        }
        "MFCreateCollection" => k!(Out(&IMFCOLLECTION)),
        "MFCreateEventQueue" => k!(Out(&IMFMEDIAEVENTQUEUE)),
        "MFCreateMediaEvent" => k!(I, I, I, Var, Out(&IMFMEDIAEVENT)),
        "MFCreateAsyncResult" => k!(
            In(&IUNKNOWN),
            In(&IMFASYNCCALLBACK),
            In(&IUNKNOWN),
            Out(&IMFASYNCRESULT)
        ),
        "MFInvokeCallback" => k!(In(&IMFASYNCRESULT)),
        "MFCreateStreamDescriptor" => {
            k!(I, I, InArr(&IMFMEDIATYPE), Out(&IMFSTREAMDESCRIPTOR))
        }
        "MFCreatePresentationDescriptor" => k!(
            I,
            InArr(&IMFSTREAMDESCRIPTOR),
            Out(&IMFPRESENTATIONDESCRIPTOR)
        ),
        "MFSerializeAttributesToStream" | "MFDeserializeAttributesFromStream" => {
            k!(In(&IMFATTRIBUTES), I, In(&ISTREAM))
        }
        "MFShutdownObject" => k!(In(&IUNKNOWN)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Méthodes propres de chaque interface, comptées dans mfobjects.h et
    /// mfidl.h ; seules trois restent non décrites, pour une raison dite.
    #[test]
    fn interfaces_completes() {
        let counts = [
            ("IMFAttributes", 30),
            ("IMFMediaType", 5),
            ("IMFMediaBuffer", 5),
            ("IMFSample", 14),
            ("IMFCollection", 6),
            ("IMFMediaEvent", 4),
            ("IMFAsyncResult", 5),
            ("IMFAsyncCallback", 2),
            ("IMFMediaEventQueue", 7),
            ("IMFMediaTypeHandler", 6),
            ("IMFStreamDescriptor", 2),
            ("IMFPresentationDescriptor", 5),
        ];
        assert_eq!(ALL.len(), counts.len());
        for (f, (name, n)) in ALL.iter().zip(counts) {
            assert_eq!(f.name, name);
            assert_eq!(f.methods.len(), n, "{name}");
            for (i, m) in f.methods.iter().enumerate() {
                let exception =
                    matches!((name, i), ("IMFMediaType", 3 | 4) | ("IMFAsyncResult", 4));
                assert_eq!(m.is_none(), exception, "{name} méthode propre n° {i}");
            }
        }
        // Les objets « attributs » héritent des 30 méthodes d'IMFAttributes.
        for f in [
            &IMFMEDIATYPE,
            &IMFSAMPLE,
            &IMFMEDIAEVENT,
            &IMFSTREAMDESCRIPTOR,
            &IMFPRESENTATIONDESCRIPTOR,
        ] {
            assert!(f.parent.is_some_and(|p| std::ptr::eq(p, &IMFATTRIBUTES)));
        }
    }

    /// Octets d'arguments x86 de chaque export (mfapi.h), comme le
    /// décorateur stdcall `@n` : un LONGLONG compte 8.
    #[test]
    fn exports_decrits() {
        let bytes =
            |k: &[A]| -> usize { k.iter().map(|a| if matches!(a, Q) { 8 } else { 4 }).sum() };
        for (func, n) in [
            ("MFCreateAttributes", 8),
            ("MFCreateMediaType", 4),
            ("MFCreateSample", 4),
            ("MFCreateMemoryBuffer", 8),
            ("MFCreateAlignedMemoryBuffer", 12),
            ("MFCreateMediaBufferWrapper", 16),
            ("MFCreateMediaBufferFromMediaType", 24),
            ("MFCreateCollection", 4),
            ("MFCreateEventQueue", 4),
            ("MFCreateMediaEvent", 20),
            ("MFCreateAsyncResult", 16),
            ("MFInvokeCallback", 4),
            ("MFCreateStreamDescriptor", 16),
            ("MFCreatePresentationDescriptor", 12),
            ("MFSerializeAttributesToStream", 12),
            ("MFDeserializeAttributesFromStream", 12),
            ("MFShutdownObject", 4),
        ] {
            let k = api_kinds(func).unwrap_or_else(|| panic!("{func} non décrite"));
            assert_eq!(bytes(k), n, "{func}");
        }
        // Chaîne et DXGI : hors de ce lot.
        assert!(api_kinds("MFCreateMediaSession").is_none());
        assert!(api_kinds("MFCreateDXGIDeviceManager").is_none());
        // Le tableau suit son nombre d'éléments.
        let k = api_kinds("MFCreateStreamDescriptor").unwrap();
        assert!(matches!(k[1], I) && matches!(k[2], InArr(f) if std::ptr::eq(f, &IMFMEDIATYPE)));
    }
}

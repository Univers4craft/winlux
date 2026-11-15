//! XAudio2 pour les programmes 32 bits (#2428).
//!
//! Comme DirectSound (`dsound_x86`) : objets « deux ABI ». Un programme x86
//! reçoit des tables de passerelles stdcall construites d'après la
//! signature de chaque méthode, au lieu des mandataires génériques de
//! `wow32::com` : les voix XAudio2 ne sont pas des objets COM (elles se
//! détruisent par DestroyVoice, sans IUnknown), et XAUDIO2_BUFFER, les
//! listes d'envois et les chaînes d'effets portent des pointeurs. Ces
//! structures sont lues directement dans la disposition du programme
//! (`layout` de xaudio2.rs, décalages selon la taille d'un pointeur, #2442).
//!
//! Signatures d'après les déclarations publiques de xaudio2.h (en-têtes
//! mingw-w64) : `i` = argument de 4 octets, `p` = case de pointeur rendue,
//! `f` = float (MaxFrequencyRatio, volumes).

use crate::dlls::audio_com::VTable;
use crate::dlls::legacy_com::{call_back, rd32, x86, x86_table};

const UNKNOWN: [&str; 3] = ["ip", "", ""];

fn with(rest: &[&'static str]) -> Vec<&'static str> {
    UNKNOWN.iter().chain(rest).copied().collect()
}

/// Méthodes communes d'IXAudio2Voice (19, sans IUnknown).
const VOICE: [&str; 19] = [
    "i",      // GetVoiceDetails
    "i",      // SetOutputVoices
    "i",      // SetEffectChain
    "ii",     // EnableEffect
    "ii",     // DisableEffect
    "ii",     // GetEffectState
    "iiii",   // SetEffectParameters
    "iii",    // GetEffectParameters
    "ii",     // SetFilterParameters
    "i",      // GetFilterParameters
    "iii",    // SetOutputFilterParameters
    "ii",     // GetOutputFilterParameters
    "fi",     // SetVolume
    "i",      // GetVolume
    "iii",    // SetChannelVolumes
    "ii",     // GetChannelVolumes
    "iiiiii", // SetOutputMatrix
    "iiiii",  // GetOutputMatrix
    "",       // DestroyVoice
];

/// IXAudio2SourceVoice après les méthodes communes. GetState prend un
/// argument Flags depuis XAudio2 2.8 ; 2.7 n'en a pas.
fn source(state: &'static str) -> Vec<&'static str> {
    let mut v = VOICE.to_vec();
    v.extend([
        "ii",  // Start
        "ii",  // Stop
        "ii",  // SubmitSourceBuffer
        "",    // FlushSourceBuffers
        "",    // Discontinuity
        "i",   // ExitLoop
        state, // GetState
        "fi",  // SetFrequencyRatio
        "i",   // GetFrequencyRatio
        "i",   // SetSourceSampleRate
    ]);
    v
}

pub(super) fn signatures(name: &str) -> Vec<&'static str> {
    match name {
        // XAudio2 2.8 et 2.9. CreateMasteringVoice : szDeviceId (LPCWSTR)
        // puis StreamCategory, sept arguments.
        "IXAudio2" => with(&[
            "i",       // RegisterForCallbacks
            "i",       // UnregisterForCallbacks
            "piifiii", // CreateSourceVoice
            "piiiiii", // CreateSubmixVoice
            "piiiiii", // CreateMasteringVoice
            "",        // StartEngine
            "",        // StopEngine
            "i",       // CommitChanges
            "i",       // GetPerformanceData
            "ii",      // SetDebugConfiguration
        ]),
        // XAudio2 2.7 : GetDeviceCount, GetDeviceDetails, Initialize ;
        // CreateMasteringVoice prend un DeviceIndex et pas de catégorie.
        "IXAudio2_27" => with(&[
            "i",       // GetDeviceCount
            "ii",      // GetDeviceDetails
            "ii",      // Initialize
            "i",       // RegisterForCallbacks
            "i",       // UnregisterForCallbacks
            "piifiii", // CreateSourceVoice
            "piiiiii", // CreateSubmixVoice
            "piiiii",  // CreateMasteringVoice
            "",        // StartEngine
            "",        // StopEngine
            "i",       // CommitChanges
            "i",       // GetPerformanceData
            "ii",      // SetDebugConfiguration
        ]),
        "IXAudio2SubmixVoice" => VOICE.to_vec(),
        "IXAudio2MasteringVoice" => {
            let mut v = VOICE.to_vec();
            v.push("i"); // GetChannelMask
            v
        }
        "IXAudio2SourceVoice" => source("ii"),
        "IXAudio2SourceVoice_27" => source("i"),
        // Effets intégrés (CreateAudioReverb, CreateAudioVolumeMeter).
        "XAudio2Effect" => UNKNOWN.to_vec(),
        _ => Vec::new(),
    }
}

/// Table à donner à un objet XAudio2 selon l'ABI du programme.
pub(super) fn abi<const N: usize>(vt: &'static VTable<N>, name: &'static str) -> *const () {
    if !x86() {
        return vt.ptr();
    }
    let sig = signatures(name);
    let methods: Vec<(usize, usize, &str)> =
        vt.0.iter()
            .enumerate()
            .map(|(i, &f)| (f as usize, f as usize, sig.get(i).copied().unwrap_or("")))
            .collect();
    x86_table(name, &methods) as *const ()
}

/// Appelle la méthode `idx` d'un rappel du programme
/// (IXAudio2VoiceCallback, IXAudio2EngineCallback) : STDMETHOD, donc
/// stdcall dans un programme 32 bits, win64 sinon.
///
/// # Safety
/// `obj` est un objet de rappel vivant du programme.
pub(super) unsafe fn callback(obj: usize, idx: usize, arg: Option<usize>) {
    if x86() {
        let f = rd32(rd32(obj) as usize + 4 * idx) as usize;
        match arg {
            Some(a) => call_back(f, &[obj, a]),
            None => call_back(f, &[obj]),
        };
        return;
    }
    let o = obj as *mut std::ffi::c_void;
    match arg {
        Some(a) => crate::dlls::audio_com::call1(o, idx, a),
        None => crate::dlls::audio_com::call0(o, idx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une signature par méthode de chaque table, et la pile x86 de
    /// chaque méthode (4 octets par case, `this` compris) conforme aux
    /// déclarations de xaudio2.h.
    #[test]
    fn signatures_completes() {
        use super::super::tables;
        for (name, len) in tables() {
            assert_eq!(signatures(name).len(), len, "{name}");
        }
        let src = signatures("IXAudio2SourceVoice");
        assert_eq!(src[21], "ii", "SubmitSourceBuffer(pBuffer, pBufferWMA)");
        assert_eq!(src[25], "ii", "GetState(pVoiceState, Flags) en 2.8+");
        assert_eq!(signatures("IXAudio2SourceVoice_27")[25], "i");
        // CreateSourceVoice : 7 arguments + this = 32 octets dépilés.
        assert_eq!(signatures("IXAudio2")[5].len() + 1, 8);
        assert_eq!(signatures("IXAudio2")[7].len(), 7, "catégorie de flux");
        assert_eq!(signatures("IXAudio2_27")[10].len(), 6);
    }
}

//! DXVA2 : décodage vidéo matériel exposé aux .exe (placeholder temporaire).
use crate::dlls::gpu_caps::Codec;

pub(crate) fn bridge_supports(codec: Codec) -> bool {
    matches!(codec, Codec::H264)
}

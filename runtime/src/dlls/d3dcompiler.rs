//! d3dcompiler_47 : `D3DCompile` compile les vertex et pixel shaders HLSL
//! (modèles 4 et 5) avec le compilateur maison (`dlls::hlsl`) en GLSL, rangé
//! dans un conteneur DXBC que la couche D3D11 reconnaît. Les autres étages
//! sont encore emballés tels quels (chunk SHEX portant le type de shader,
//! plus un chunk « HLSL » contenant le texte source). Sert notamment à ANGLE
//! (moteur D3D11 de libGLESv2), qui charge d3dcompiler_47.dll.
//!
//! WINLUX_HLSL_DUMP=dossier copie chaque source et son GLSL ;
//! WINLUX_HLSL_STRICT=1 renvoie les erreurs de compilation au programme.
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::dlls::ole32::Guid;

#[path = "d3dcompiler_reflect.rs"]
mod reflect;

const S_OK: i32 = 0;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;

#[repr(C)]
struct BlobVTable {
    query_interface: unsafe extern "win64" fn(*mut Blob, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut Blob) -> u32,
    release: unsafe extern "win64" fn(*mut Blob) -> u32,
    get_buffer_pointer: unsafe extern "win64" fn(*mut Blob) -> *mut c_void,
    get_buffer_size: unsafe extern "win64" fn(*mut Blob) -> usize,
}

#[repr(C)]
struct Blob {
    vtable: *const BlobVTable,
    ref_count: AtomicU32,
    data: Vec<u8>,
}

/// IUnknown et ID3DBlob (ID3D10Blob, 8ba5fb08-5195-40e2-ac58-0d989c3a0102).
unsafe extern "win64" fn blob_qi(this: *mut Blob, riid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() || riid.is_null() {
        return E_INVALIDARG;
    }
    let d1 = unsafe { (*riid).data1 };
    if d1 == 0 || d1 == 0x8ba5_fb08 {
        unsafe {
            (*this).ref_count.fetch_add(1, Ordering::Relaxed);
            *out = this.cast();
        }
        return S_OK;
    }
    unsafe { *out = std::ptr::null_mut() };
    E_NOINTERFACE
}
unsafe extern "win64" fn blob_addref(this: *mut Blob) -> u32 {
    unsafe { (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}
unsafe extern "win64" fn blob_release(this: *mut Blob) -> u32 {
    let prev = unsafe { (*this).ref_count.fetch_sub(1, Ordering::AcqRel) };
    if prev == 1 {
        drop(unsafe { Box::from_raw(this) });
        return 0;
    }
    prev - 1
}
unsafe extern "win64" fn blob_ptr(this: *mut Blob) -> *mut c_void {
    unsafe { (*this).data.as_mut_ptr().cast() }
}
unsafe extern "win64" fn blob_size(this: *mut Blob) -> usize {
    unsafe { (*this).data.len() }
}

static BLOB_VTABLE: BlobVTable = BlobVTable {
    query_interface: blob_qi,
    add_ref: blob_addref,
    release: blob_release,
    get_buffer_pointer: blob_ptr,
    get_buffer_size: blob_size,
};

fn new_blob(data: Vec<u8>) -> *mut c_void {
    Box::into_raw(Box::new(Blob {
        vtable: &BLOB_VTABLE,
        ref_count: AtomicU32::new(1),
        data,
    }))
    .cast()
}

/// Type de programme DXBC (bits 16..31 du jeton de version) d'après la cible.
fn program_type(target: &str) -> u32 {
    match target.get(..2) {
        Some("vs") => 1,
        Some("gs") => 2,
        Some("hs") => 3,
        Some("ds") => 4,
        Some("cs") => 5,
        _ => 0, // ps et inconnu
    }
}

/// Conteneur DXBC : en-tête, chunk SHEX (jeton de version seul) et chunk HLSL.
fn dxbc_container(target: &str, source: &[u8]) -> Vec<u8> {
    let version = (program_type(target) << 16) | 0x50;
    let shex: Vec<u8> = version.to_le_bytes().to_vec();
    let chunks: [(&[u8; 4], &[u8]); 2] = [(b"SHEX", &shex), (b"HLSL", source)];
    let header_len = 32 + 4 * chunks.len();
    let mut offsets = Vec::new();
    let mut body = Vec::new();
    for (fourcc, data) in chunks {
        offsets.push((header_len + body.len()) as u32);
        body.extend_from_slice(fourcc);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    let total = header_len + body.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"DXBC");
    out.extend_from_slice(&[0u8; 16]); // somme de contrôle non calculée
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
    for o in offsets {
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend_from_slice(&body);
    out
}

const E_FAIL: i32 = 0x8000_4005u32 as i32;

/// D3D_SHADER_MACRO[] terminé par un nom nul.
unsafe fn read_defines(p: *const c_void) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if p.is_null() {
        return out;
    }
    let mut m = p.cast::<[*const u8; 2]>();
    for _ in 0..4096 {
        let [name, def] = unsafe { *m };
        if name.is_null() {
            break;
        }
        let def = if def.is_null() {
            "1".to_string()
        } else {
            unsafe { crate::dlls::cstr(def) }
        };
        out.push((unsafe { crate::dlls::cstr(name) }, def));
        m = unsafe { m.add(1) };
    }
    out
}

type IncludeOpen = unsafe extern "win64" fn(
    *mut c_void,
    u32,
    *const u8,
    *const c_void,
    *mut *const c_void,
    *mut u32,
) -> i32;
type IncludeClose = unsafe extern "win64" fn(*mut c_void, *const c_void) -> i32;

/// Lecture d'un fichier inclus : ID3DInclude de l'application, ou
/// D3D_COMPILE_STANDARD_FILE_INCLUDE (valeur 1) relatif au fichier source.
fn include_file(
    include: *const c_void,
    source_name: &str,
    name: &str,
    system: bool,
) -> Option<String> {
    if include.is_null() {
        return None;
    }
    if include as usize == 1 {
        let base = if source_name.is_empty() {
            std::path::PathBuf::from(".")
        } else {
            crate::paths::to_linux(source_name)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default()
        };
        let direct = crate::paths::to_linux(name);
        let path = if name.contains(':') || name.starts_with('\\') || name.starts_with('/') {
            direct
        } else {
            base.join(name.replace('\\', "/"))
        };
        return std::fs::read(path)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned());
    }
    unsafe {
        let vt = *(include as *const *const usize);
        if vt.is_null() {
            return None;
        }
        let open: IncludeOpen = std::mem::transmute::<usize, IncludeOpen>(*vt);
        let close: IncludeClose = std::mem::transmute::<usize, IncludeClose>(*vt.add(1));
        let cname = std::ffi::CString::new(name).ok()?;
        let mut data: *const c_void = std::ptr::null();
        let mut bytes = 0u32;
        let hr = open(
            include as *mut c_void,
            u32::from(system),
            cname.as_ptr().cast(),
            std::ptr::null(),
            &mut data,
            &mut bytes,
        );
        if hr < 0 || data.is_null() {
            return None;
        }
        let text = String::from_utf8_lossy(std::slice::from_raw_parts(
            data.cast::<u8>(),
            bytes as usize,
        ))
        .into_owned();
        close(include as *mut c_void, data);
        Some(text)
    }
}

/// Message d'erreur au format de fxc : « fichier(ligne,1): error X3000: … ».
fn error_text(source_name: &str, msg: &str) -> String {
    let name = if source_name.is_empty() {
        "hlsl"
    } else {
        source_name
    };
    if let Some(rest) = msg.strip_prefix('(') {
        if let Some((line, m)) = rest.split_once(") : ") {
            if line.chars().all(|c| c.is_ascii_digit()) {
                return format!("{name}({line},1): error X3000: {m}\n");
            }
        }
    }
    format!("{name}(1,1): error X3000: {msg}\n")
}

/// Modèle de shader 4 ou plus (« vs_4_0 », « ps_5_0 », « ps_4_0_level_9_3 »).
fn is_sm4_plus(target: &str) -> bool {
    target
        .get(3..4)
        .and_then(|c| c.parse::<u32>().ok())
        .is_some_and(|m| m >= 4)
}

/// Profils Direct3D 9 : « vs_1_1 », « vs_2_0 », « ps_2_a », « ps_3_0 »…
fn is_d3d9_profile(target: &str) -> bool {
    (target.starts_with("vs_") || target.starts_with("ps_"))
        && target
            .get(3..4)
            .and_then(|c| c.parse::<u32>().ok())
            .is_some_and(|m| (1..=3).contains(&m))
}

/// Copie du source et du résultat quand WINLUX_HLSL_DUMP désigne un dossier.
fn dump(
    source: &str,
    target: &str,
    entry: &str,
    result: &Result<crate::dlls::hlsl::Compiled, String>,
) {
    let Some(dir) = std::env::var_os("WINLUX_HLSL_DUMP") else {
        return;
    };
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::path::PathBuf::from(dir);
    let _ = std::fs::create_dir_all(&dir);
    let stem = format!("{}-{n:04}-{target}-{entry}", std::process::id());
    let _ = std::fs::write(dir.join(format!("{stem}.hlsl")), source);
    let out = match result {
        Ok(c) => c.glsl.clone(),
        Err(e) => format!("// ERREUR : {e}\n"),
    };
    let _ = std::fs::write(dir.join(format!("{stem}.glsl")), out);
}

#[allow(clippy::too_many_arguments)]
unsafe fn compile_common(
    src: *const c_void,
    src_size: usize,
    source_name: &str,
    defines: *const c_void,
    include: *const c_void,
    entry: *const u8,
    target: *const u8,
    flags1: u32,
    code: *mut *mut c_void,
    errors: *mut *mut c_void,
) -> i32 {
    if !errors.is_null() {
        unsafe { *errors = std::ptr::null_mut() };
    }
    if code.is_null() || (src.is_null() && src_size != 0) {
        return E_INVALIDARG;
    }
    unsafe { *code = std::ptr::null_mut() };
    let source = if src.is_null() {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(src.cast::<u8>(), src_size) }
    };
    let target = if target.is_null() {
        String::new()
    } else {
        unsafe { crate::dlls::cstr(target) }
    };
    let entry = if entry.is_null() {
        "main".to_string()
    } else {
        unsafe { crate::dlls::cstr(entry) }
    };
    crate::trace!("D3DCompile(cible {target}, entrée {entry}, {src_size} octets de HLSL)");
    if is_d3d9_profile(&target) {
        // vs/ps 1.x à 3.0 : D3DCompile rend le code Direct3D 9 brut (jetons
        // avec la table des constantes), comme D3DXCompileShader (#931).
        let text =
            String::from_utf8_lossy(source.strip_suffix(&[0]).unwrap_or(source)).into_owned();
        let defs = unsafe { read_defines(defines) };
        let inc = |name: &str| include_file(include, source_name, name, false);
        return match crate::dlls::d3dx9_hlsl::compile(&text, &defs, &inc, &entry, &target, flags1) {
            Ok(c) => {
                let bytes: Vec<u8> = c.tokens.iter().flat_map(|t| t.to_le_bytes()).collect();
                unsafe { *code = new_blob(bytes) };
                S_OK
            }
            Err(e) => {
                let msg = error_text(source_name, &e);
                crate::log_warn!(
                    "d3dcompiler",
                    "HLSL non compilé ({target}, {entry}) : {}",
                    msg.trim_end()
                );
                if !errors.is_null() {
                    let mut b = msg.into_bytes();
                    b.push(0);
                    unsafe { *errors = new_blob(b) };
                }
                E_FAIL
            }
        };
    }
    if !is_sm4_plus(&target) || crate::dlls::hlsl::stage_of(&target).is_err() {
        // Étages non traduits (géométrie, calcul…) : source emballé tel quel.
        unsafe { *code = new_blob(dxbc_container(&target, source)) };
        return S_OK;
    }
    let text = String::from_utf8_lossy(source.strip_suffix(&[0]).unwrap_or(source)).into_owned();
    let defs = unsafe { read_defines(defines) };
    let mut inc = |name: &str, system: bool| include_file(include, source_name, name, system);
    let result = crate::dlls::hlsl::compile(&text, &defs, &mut inc, &entry, &target, flags1);
    dump(&text, &target, &entry, &result);
    match result {
        Ok(c) => {
            unsafe { *code = new_blob(crate::dlls::hlsl::container(&c)) };
            S_OK
        }
        Err(e) => {
            let msg = error_text(source_name, &e);
            crate::log_warn!(
                "d3dcompiler",
                "HLSL non compilé ({target}, {entry}) : {}",
                msg.trim_end()
            );
            if std::env::var_os("WINLUX_HLSL_STRICT").is_none() {
                // Ancien comportement : conteneur portant le source, pour ne
                // pas bloquer un programme qui marchait sans rendu.
                unsafe { *code = new_blob(dxbc_container(&target, source)) };
                return S_OK;
            }
            if !errors.is_null() {
                let mut b = msg.into_bytes();
                b.push(0);
                unsafe { *errors = new_blob(b) };
            }
            E_FAIL
        }
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn d3d_compile(
    src: *const c_void,
    src_size: usize,
    source_name: *const u8,
    defines: *const c_void,
    include: *const c_void,
    entry: *const u8,
    target: *const u8,
    flags1: u32,
    _flags2: u32,
    code: *mut *mut c_void,
    errors: *mut *mut c_void,
) -> i32 {
    let name = if source_name.is_null() {
        String::new()
    } else {
        unsafe { crate::dlls::cstr(source_name) }
    };
    unsafe {
        compile_common(
            src, src_size, &name, defines, include, entry, target, flags1, code, errors,
        )
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn d3d_compile2(
    src: *const c_void,
    src_size: usize,
    source_name: *const u8,
    defines: *const c_void,
    include: *const c_void,
    entry: *const u8,
    target: *const u8,
    flags1: u32,
    flags2: u32,
    _secondary_flags: u32,
    _secondary: *const c_void,
    _secondary_size: usize,
    code: *mut *mut c_void,
    errors: *mut *mut c_void,
) -> i32 {
    unsafe {
        d3d_compile(
            src,
            src_size,
            source_name,
            defines,
            include,
            entry,
            target,
            flags1,
            flags2,
            code,
            errors,
        )
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn d3d_compile_from_file(
    file: *const u16,
    defines: *const c_void,
    include: *const c_void,
    entry: *const u8,
    target: *const u8,
    flags1: u32,
    _flags2: u32,
    code: *mut *mut c_void,
    errors: *mut *mut c_void,
) -> i32 {
    if file.is_null() {
        return E_INVALIDARG;
    }
    let name = unsafe { crate::dlls::wstr(file) };
    let Ok(data) = std::fs::read(crate::paths::to_linux(&name)) else {
        return 0x8007_0002u32 as i32; // ERROR_FILE_NOT_FOUND
    };
    unsafe {
        compile_common(
            data.as_ptr().cast(),
            data.len(),
            &name,
            defines,
            include,
            entry,
            target,
            flags1,
            code,
            errors,
        )
    }
}

unsafe extern "win64" fn d3d_preprocess(
    src: *const c_void,
    src_size: usize,
    source_name: *const u8,
    defines: *const c_void,
    include: *const c_void,
    code: *mut *mut c_void,
    errors: *mut *mut c_void,
) -> i32 {
    if !errors.is_null() {
        unsafe { *errors = std::ptr::null_mut() };
    }
    if code.is_null() || src.is_null() {
        return E_INVALIDARG;
    }
    let name = if source_name.is_null() {
        String::new()
    } else {
        unsafe { crate::dlls::cstr(source_name) }
    };
    let source = unsafe { std::slice::from_raw_parts(src.cast::<u8>(), src_size) };
    let text = String::from_utf8_lossy(source).into_owned();
    let defs = unsafe { read_defines(defines) };
    let mut inc = |n: &str, system: bool| include_file(include, &name, n, system);
    match crate::dlls::hlsl::preprocess_text(&text, &defs, &mut inc) {
        Ok(t) => {
            let mut b = t.into_bytes();
            b.push(0);
            unsafe { *code = new_blob(b) };
            S_OK
        }
        Err(e) => {
            if !errors.is_null() {
                let mut b = error_text(&name, &e).into_bytes();
                b.push(0);
                unsafe { *errors = new_blob(b) };
            }
            E_FAIL
        }
    }
}

unsafe extern "win64" fn d3d_create_blob(size: usize, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    unsafe { *out = new_blob(vec![0; size]) };
    S_OK
}

unsafe extern "win64" fn d3d_not_impl() -> i32 {
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D3DCompile"=>fn d3d_compile,
        "D3DCompile2"=>fn d3d_compile2,
        "D3DCompileFromFile"=>fn d3d_compile_from_file,
        "D3DPreprocess"=>fn d3d_preprocess,
        "D3DCreateBlob"=>fn d3d_create_blob,
        "D3DDisassemble"=>fn d3d_not_impl,
        "D3DReflect"=>fn reflect::d3d_reflect,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conteneur_dxbc_porte_le_type_de_shader() {
        let v = dxbc_container("vs_5_0", b"float4 main() : SV_Position { return 0; }");
        let t = crate::dlls::d3d_shader::translate_dxbc(&v).expect("DXBC lisible");
        assert_eq!(t.shader_type, crate::dlls::d3d_shader::ShaderType::Vertex);
        let p = dxbc_container("ps_5_0", b"");
        let t = crate::dlls::d3d_shader::translate_dxbc(&p).expect("DXBC lisible");
        assert_eq!(t.shader_type, crate::dlls::d3d_shader::ShaderType::Pixel);
    }

    #[test]
    fn d3dcompile_produit_du_glsl() {
        let src = b"cbuffer C { float4 col; };\n#ifdef ROUGE\nfloat4 main() : SV_Target { return col * 2; }\n#endif\n";
        let name = b"ROUGE\0";
        let defs: [[*const u8; 2]; 2] = [[name.as_ptr(), std::ptr::null()], [std::ptr::null(); 2]];
        let mut code = std::ptr::null_mut();
        let mut errors = std::ptr::null_mut();
        let hr = unsafe {
            d3d_compile(
                src.as_ptr().cast(),
                src.len(),
                std::ptr::null(),
                defs.as_ptr().cast(),
                std::ptr::null(),
                c"main".as_ptr().cast(),
                c"ps_5_0".as_ptr().cast(),
                0,
                0,
                &mut code,
                &mut errors,
            )
        };
        assert_eq!(hr, S_OK);
        let blob = code.cast::<Blob>();
        let data = unsafe { (*blob).data.clone() };
        unsafe { blob_release(blob) };
        let s = crate::dlls::dxbc::translate(&data).expect("conteneur lisible");
        assert_eq!(s.stage, crate::dlls::dxbc::Stage::Pixel);
        assert!(s.glsl.contains("out vec4 target0"), "{}", s.glsl);
        assert_eq!(
            error_text("a.hlsl", "(3) : identifiant inconnu"),
            "a.hlsl(3,1): error X3000: identifiant inconnu\n"
        );
    }

    /// Shader généré par WPF (wpfgfx, rendu matériel D3D9) : D3DCompile en
    /// vs_2_0 / ps_2_0 doit rendre du code Direct3D 9, pas un conteneur DXBC.
    #[test]
    fn d3dcompile_profils_d3d9_pour_wpf() {
        let src = include_bytes!("d3dcompiler_wpf_exemple.hlsl");
        for (entry, target, kind) in [
            (c"VertexShaderImpl", c"vs_2_0", 0xFFFE_0200u32),
            (c"PixelShaderImpl", c"ps_2_0", 0xFFFF_0200u32),
        ] {
            let mut code = std::ptr::null_mut();
            let mut errors = std::ptr::null_mut();
            let hr = unsafe {
                d3d_compile(
                    src.as_ptr().cast(),
                    src.len(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    entry.as_ptr().cast(),
                    target.as_ptr().cast(),
                    0,
                    0,
                    &mut code,
                    &mut errors,
                )
            };
            assert_eq!(hr, S_OK, "{target:?}");
            let blob = code.cast::<Blob>();
            let data = unsafe { (*blob).data.clone() };
            unsafe { blob_release(blob) };
            let first = u32::from_le_bytes(data[..4].try_into().unwrap());
            assert_eq!(first, kind, "{target:?}");
            let last = u32::from_le_bytes(data[data.len() - 4..].try_into().unwrap());
            assert_eq!(last, 0x0000_FFFF, "fin de shader D3D9");
        }
        assert!(is_d3d9_profile("ps_3_0") && is_d3d9_profile("vs_1_1"));
        assert!(!is_d3d9_profile("ps_4_0_level_9_3") && !is_d3d9_profile("gs_4_0"));
    }
}

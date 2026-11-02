//! Suite auto-vérifiante : HTTPS des applications.
//!
//! 1. crypt32 : magasin ROOT du système, nom d'un certificat, chaîne et
//!    stratégie, conversions base64.
//! 2. advapi32 : condensat SHA-256 par la CryptoAPI.
//! 3. Schannel (secur32) : si WINLUX_TLS_PORT est défini, poignée de main
//!    TLS avec un serveur local (openssl s_server -www), requête chiffrée et
//!    réponse « HTTP/1.0 200 » déchiffrée, exactement comme curl ou .NET.
//! Code de sortie = nombre d'échecs.
#![no_std]
#![no_main]

use core::ffi::{c_char, c_void};
use core::ptr::{null, null_mut};
use winlux_testexe::*;

#[link(name = "crypt32", kind = "raw-dylib")]
extern "system" {
    fn CertOpenStore(p: usize, enc: u32, prov: usize, f: u32, para: *const c_void) -> usize;
    fn CryptQueryObject(
        ty: u32,
        obj: *const c_void,
        content: u32,
        format: u32,
        f: u32,
        enc: *mut u32,
        ctype: *mut u32,
        ftype: *mut u32,
        store: *mut usize,
        msg: *mut usize,
        ctx: *mut *const CertContext,
    ) -> i32;
    fn CertAddCertificateContextToStore(
        h: usize,
        c: *const CertContext,
        disp: u32,
        out: *mut *const CertContext,
    ) -> i32;
    fn CertCreateCertificateChainEngine(config: *const u8, out: *mut usize) -> i32;
    fn CertFreeCertificateChainEngine(e: usize);
    fn CryptProtectData(
        input: *const DataBlob,
        desc: *const u16,
        entropy: *const DataBlob,
        reserved: usize,
        prompt: usize,
        flags: u32,
        out: *mut DataBlob,
    ) -> i32;
    fn CryptUnprotectData(
        input: *const DataBlob,
        desc: *mut *mut u16,
        entropy: *const DataBlob,
        reserved: usize,
        prompt: usize,
        flags: u32,
        out: *mut DataBlob,
    ) -> i32;
    fn CertOpenSystemStoreW(prov: usize, name: *const u16) -> usize;
    fn CertCloseStore(h: usize, flags: u32) -> i32;
    fn CertEnumCertificatesInStore(h: usize, prev: *const CertContext) -> *const CertContext;
    fn CertFreeCertificateContext(c: *const CertContext) -> i32;
    fn CertGetNameStringW(
        c: *const CertContext,
        kind: u32,
        flags: u32,
        para: *const c_void,
        buf: *mut u16,
        n: u32,
    ) -> u32;
    fn CertGetCertificateChain(
        engine: usize,
        c: *const CertContext,
        time: *const c_void,
        store: usize,
        para: *const ChainPara,
        flags: u32,
        reserved: *const c_void,
        out: *mut *const ChainContext,
    ) -> i32;
    fn CertFreeCertificateChain(c: *const ChainContext);
    fn CertVerifyCertificateChainPolicy(
        policy: usize,
        chain: *const ChainContext,
        para: *const PolicyPara,
        st: *mut PolicyStatus,
    ) -> i32;
    fn CryptStringToBinaryA(
        s: *const c_char,
        n: u32,
        flags: u32,
        out: *mut u8,
        len: *mut u32,
        skip: *mut u32,
        f: *mut u32,
    ) -> i32;
    fn CryptBinaryToStringA(
        b: *const u8,
        n: u32,
        flags: u32,
        out: *mut c_char,
        len: *mut u32,
    ) -> i32;
}

#[link(name = "advapi32", kind = "raw-dylib")]
extern "system" {
    fn CryptAcquireContextW(out: *mut usize, c: *const u16, p: *const u16, t: u32, f: u32) -> i32;
    fn CryptCreateHash(prov: usize, alg: u32, key: usize, f: u32, out: *mut usize) -> i32;
    fn CryptHashData(h: usize, d: *const u8, n: u32, f: u32) -> i32;
    fn CryptGetHashParam(h: usize, p: u32, out: *mut u8, n: *mut u32, f: u32) -> i32;
    fn CryptDestroyHash(h: usize) -> i32;
    fn CryptReleaseContext(h: usize, f: u32) -> i32;
    fn CryptImportKey(prov: usize, d: *const u8, n: u32, k: usize, f: u32, out: *mut usize) -> i32;
    fn CryptEncrypt(k: usize, h: usize, fin: i32, f: u32, d: *mut u8, n: *mut u32, cap: u32)
        -> i32;
    fn CryptDecrypt(k: usize, h: usize, fin: i32, f: u32, d: *mut u8, n: *mut u32) -> i32;
    fn CryptDestroyKey(k: usize) -> i32;
}

#[link(name = "secur32", kind = "raw-dylib")]
extern "system" {
    fn AcquireCredentialsHandleW(
        principal: *const u16,
        package: *const u16,
        use_: u32,
        logon: *const c_void,
        auth: *const c_void,
        f: *const c_void,
        a: *const c_void,
        cred: *mut SecHandle,
        expiry: *mut i64,
    ) -> i32;
    fn InitializeSecurityContextW(
        cred: *const SecHandle,
        ctx: *const SecHandle,
        target: *const u16,
        req: u32,
        r1: u32,
        rep: u32,
        input: *mut SecBufferDesc,
        r2: u32,
        new_ctx: *mut SecHandle,
        output: *mut SecBufferDesc,
        attrs: *mut u32,
        expiry: *mut i64,
    ) -> i32;
    fn QueryContextAttributesW(ctx: *const SecHandle, attr: u32, out: *mut c_void) -> i32;
    fn EncryptMessage(ctx: *const SecHandle, qop: u32, msg: *mut SecBufferDesc, seq: u32) -> i32;
    fn DecryptMessage(
        ctx: *const SecHandle,
        msg: *mut SecBufferDesc,
        seq: u32,
        qop: *mut u32,
    ) -> i32;
    fn DeleteSecurityContext(ctx: *const SecHandle) -> i32;
    fn FreeCredentialsHandle(cred: *const SecHandle) -> i32;
    fn FreeContextBuffer(p: *mut c_void) -> i32;
}

#[repr(C)]
struct CertContext {
    encoding: u32,
    der: *const u8,
    der_len: u32,
    info: *const c_void,
    store: usize,
}

#[repr(C)]
struct ChainPara {
    cb: u32,
    usage_type: u32,
    usage_count: u32,
    usage: *const c_void,
}

#[repr(C)]
struct ChainContext {
    cb: u32,
    error: u32,
    info: u32,
}

#[repr(C)]
struct PolicyPara {
    cb: u32,
    flags: u32,
    extra: *const c_void,
}

#[repr(C)]
struct PolicyStatus {
    cb: u32,
    error: u32,
    chain: i32,
    element: i32,
    extra: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SecHandle {
    lower: usize,
    upper: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SecBuffer {
    cb: u32,
    ty: u32,
    pv: *mut u8,
}

#[repr(C)]
struct SecBufferDesc {
    version: u32,
    count: u32,
    buffers: *mut SecBuffer,
}

/// Chaîne C → UTF-16 dans `buf`.
fn wide<'a>(s: &[u8], buf: &'a mut [u16; 64]) -> *const u16 {
    let mut n = 0;
    for &c in s {
        if c == 0 || n + 1 >= buf.len() {
            break;
        }
        buf[n] = c as u16;
        n += 1;
    }
    buf[n] = 0;
    buf.as_ptr()
}

unsafe fn certificates() {
    out(b"\n-- crypt32 --\n");
    let mut w = [0u16; 64];
    let root = CertOpenSystemStoreW(0, wide(b"ROOT", &mut w));
    check!(root != 0, "magasin ROOT ouvert");
    let mut count = 0u32;
    let mut first: *const CertContext = null();
    let mut c = CertEnumCertificatesInStore(root, null());
    while !c.is_null() {
        if count == 0 {
            first = CertDuplicate(c);
        }
        count += 1;
        c = CertEnumCertificatesInStore(root, c);
    }
    if count == 0 {
        out("  (aucune autorité sur l'hôte : vérifications du magasin sautées)\n".as_bytes());
    } else {
        check!(true, "autorités du système énumérées");
        let mut name = [0u16; 128];
        let n = CertGetNameStringW(first, 4, 0, null(), name.as_mut_ptr(), 128);
        check!(n > 1, "nom d'une autorité (CertGetNameStringW)");
        check!(
            (*first).der_len > 100 && *(*first).der == 0x30,
            "certificat DER réel"
        );
        let para = ChainPara {
            cb: 24,
            usage_type: 0,
            usage_count: 0,
            usage: null(),
        };
        let mut chain: *const ChainContext = null();
        let ok = CertGetCertificateChain(0, first, null(), 0, &para, 0, null(), &mut chain);
        check!(ok != 0 && !chain.is_null(), "chaîne construite");
        if !chain.is_null() {
            // Racine du système : seule une date dépassée est tolérée.
            let pp = PolicyPara {
                cb: 16,
                flags: 0x7,
                extra: null(),
            };
            let mut st = PolicyStatus {
                cb: 24,
                error: 1,
                chain: 0,
                element: 0,
                extra: null(),
            };
            CertVerifyCertificateChainPolicy(1, chain, &pp, &mut st);
            check!(
                st.error == 0,
                "racine de confiance acceptée par la stratégie"
            );
            CertFreeCertificateChain(chain);
        }
        CertFreeCertificateContext(first);
    }
    CertCloseStore(root, 0);

    let mut text = [0u8; 64];
    let mut n = 64u32;
    // CRYPT_STRING_BASE64 | CRYPT_STRING_NOCRLF
    let ok = CryptBinaryToStringA(
        b"Winlux!".as_ptr(),
        7,
        0x4000_0001,
        text.as_mut_ptr().cast(),
        &mut n,
    );
    check!(
        ok != 0 && &text[..n as usize] == b"V2lubHV4IQ==",
        "CryptBinaryToStringA (base64)"
    );
    let mut bin = [0u8; 16];
    let mut m = 16u32;
    let ok = CryptStringToBinaryA(
        text.as_ptr().cast(),
        n,
        1,
        bin.as_mut_ptr(),
        &mut m,
        null_mut(),
        null_mut(),
    );
    check!(
        ok != 0 && &bin[..m as usize] == b"Winlux!",
        "CryptStringToBinaryA (base64)"
    );
}

#[allow(non_snake_case)]
unsafe fn CertDuplicate(c: *const CertContext) -> *const CertContext {
    #[link(name = "crypt32", kind = "raw-dylib")]
    extern "system" {
        fn CertDuplicateCertificateContext(c: *const CertContext) -> *const CertContext;
    }
    CertDuplicateCertificateContext(c)
}

unsafe fn hashes() {
    out(b"\n-- CryptoAPI --\n");
    let mut prov = 0usize;
    check!(
        CryptAcquireContextW(&mut prov, null(), null(), 24, 0xF000_0000) != 0,
        "CryptAcquireContextW"
    );
    let mut h = 0usize;
    check!(
        CryptCreateHash(prov, 0x800c, 0, 0, &mut h) != 0,
        "CryptCreateHash (SHA-256)"
    );
    CryptHashData(h, b"abc".as_ptr(), 3, 0);
    let mut d = [0u8; 32];
    let mut n = 32u32;
    let ok = CryptGetHashParam(h, 2, d.as_mut_ptr(), &mut n, 0);
    check!(
        ok != 0 && n == 32 && d[..4] == [0xba, 0x78, 0x16, 0xbf],
        "condensat SHA-256 exact"
    );
    CryptDestroyHash(h);

    // DES par CryptImportKey/CryptEncrypt, comme le NTLM de libcurl.
    let mut blob = [0u8; 20];
    blob[0] = 8; // PLAINTEXTKEYBLOB
    blob[1] = 2;
    blob[4..8].copy_from_slice(&0x6601u32.to_le_bytes());
    blob[8] = 8;
    blob[12..].copy_from_slice(&[0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
    let mut key = 0usize;
    check!(
        CryptImportKey(prov, blob.as_ptr(), 20, 0, 0, &mut key) != 0,
        "CryptImportKey (DES)"
    );
    let mut block = *b"Now is t";
    let mut n = 8u32;
    CryptEncrypt(key, 0, 0, 0, block.as_mut_ptr(), &mut n, 8);
    check!(
        block == [0x3f, 0xa4, 0x0e, 0x8a, 0x98, 0x4d, 0x48, 0x15],
        "CryptEncrypt DES exact"
    );
    CryptDestroyKey(key);
    // AES-256 avec bourrage, aller-retour.
    let mut blob = [0u8; 44];
    blob[0] = 8;
    blob[1] = 2;
    blob[4..8].copy_from_slice(&0x6610u32.to_le_bytes());
    blob[8] = 32;
    for (i, b) in blob[12..].iter_mut().enumerate() {
        *b = i as u8;
    }
    check!(
        CryptImportKey(prov, blob.as_ptr(), 44, 0, 0, &mut key) != 0,
        "CryptImportKey (AES-256)"
    );
    let mut buf = [0u8; 32];
    buf[..6].copy_from_slice(b"Winlux");
    let mut n = 6u32;
    let enc = CryptEncrypt(key, 0, 1, 0, buf.as_mut_ptr(), &mut n, 32);
    check!(enc != 0 && n == 16, "CryptEncrypt AES (bourrage PKCS#5)");
    let dec = CryptDecrypt(key, 0, 1, 0, buf.as_mut_ptr(), &mut n);
    check!(
        dec != 0 && n == 6 && &buf[..6] == b"Winlux",
        "CryptDecrypt AES"
    );
    CryptDestroyKey(key);
    CryptReleaseContext(prov, 0);
}

/// Vérification « à la libcurl » avec CURLOPT_CAINFO : fichier d'autorités
/// chargé par CryptQueryObject, moteur aux racines exclusives, chaîne du
/// certificat du serveur et tous ses noms DNS.
unsafe fn curl_cainfo(ctx: *const SecHandle) {
    let mut cert: *const CertContext = null();
    check!(
        QueryContextAttributesW(ctx, 0x53, (&mut cert as *mut *const CertContext).cast()) == 0
            && !cert.is_null(),
        "certificat du serveur (SECPKG_ATTR_REMOTE_CERT_CONTEXT)"
    );
    if cert.is_null() {
        return;
    }
    let mut names = [0u16; 128];
    let n = CertGetNameStringW(cert, 6, 0x0001_0002, null(), names.as_mut_ptr(), 128);
    check!(
        n >= 11 && names[..9] == "localhost".encode_utf16().collect::<Ar<9>>().0,
        "noms DNS du serveur (CERT_NAME_SEARCH_ALL_NAMES_FLAG)"
    );
    let mut path = [0u16; 300];
    path[0] = b'Z' as u16;
    path[1] = b':' as u16;
    let len = GetEnvironmentVariableW(
        "WINLUX_TLS_CA\0"
            .encode_utf16()
            .collect::<Ar<14>>()
            .0
            .as_ptr(),
        path.as_mut_ptr().add(2),
        290,
    );
    if len == 0 {
        out(b"  (WINLUX_TLS_CA absent : CURLOPT_CAINFO saute)\n");
        CertFreeCertificateContext(cert);
        return;
    }
    let store = CertOpenStore(2, 0, 0, 0x2000, null());
    let mut ca: *const CertContext = null();
    let mut content = 0u32;
    let q = CryptQueryObject(
        1,
        path.as_ptr().cast(),
        2,
        0x3fe,
        0,
        null_mut(),
        &mut content,
        null_mut(),
        null_mut(),
        null_mut(),
        &mut ca,
    );
    check!(q != 0 && content == 1, "CryptQueryObject (fichier PEM)");
    if q != 0 {
        CertAddCertificateContextToStore(store, ca, 3, null_mut());
        CertFreeCertificateContext(ca);
    }
    let mut config = [0u8; 88];
    config[0] = 88;
    config[64..72].copy_from_slice(&store.to_le_bytes());
    config[80] = 1;
    let mut engine = 0usize;
    check!(
        CertCreateCertificateChainEngine(config.as_ptr(), &mut engine) != 0,
        "CertCreateCertificateChainEngine (hExclusiveRoot)"
    );
    let para = [0u8; 32];
    let mut chain: *const ChainContext = null();
    let ok = CertGetCertificateChain(
        engine,
        cert,
        null(),
        (*cert).store,
        para.as_ptr().cast(),
        0x2000_0000,
        null(),
        &mut chain,
    );
    check!(
        ok != 0 && !chain.is_null() && (*chain).error & !1 == 0,
        "chaîne de confiance par le fichier d'autorités"
    );
    if !chain.is_null() {
        CertFreeCertificateChain(chain);
    }
    CertFreeCertificateChainEngine(engine);
    CertCloseStore(store, 0);
    CertFreeCertificateContext(cert);
}

#[repr(C)]
struct DataBlob {
    cb: u32,
    pb: *mut u8,
}

/// DPAPI : un secret protégé pour le compte se relit, pas avec une autre entropie.
unsafe fn protected_data() {
    out(b"\n-- Protection de donnees (DPAPI) --\n");
    let secret = *b"jeton d'acces";
    let input = DataBlob {
        cb: secret.len() as u32,
        pb: secret.as_ptr() as *mut u8,
    };
    let mut salt = *b"sel";
    let entropy = DataBlob {
        cb: 3,
        pb: salt.as_mut_ptr(),
    };
    let mut blob = DataBlob {
        cb: 0,
        pb: null_mut(),
    };
    check!(
        CryptProtectData(&input, null(), &entropy, 0, 0, 0, &mut blob) != 0 && blob.cb > 13,
        "CryptProtectData"
    );
    let mut back = DataBlob {
        cb: 0,
        pb: null_mut(),
    };
    let ok = CryptUnprotectData(&blob, null_mut(), &entropy, 0, 0, 0, &mut back);
    check!(
        ok != 0 && back.cb == 13 && core::slice::from_raw_parts(back.pb, 13) == secret,
        "CryptUnprotectData rend le secret"
    );
    check!(
        CryptUnprotectData(&blob, null_mut(), null(), 0, 0, 0, &mut back) == 0,
        "autre entropie refusée"
    );
}

/// Petit tableau de taille fixe rempli depuis un itérateur (no_std).
struct Ar<const N: usize>([u16; N]);
impl<const N: usize> core::iter::FromIterator<u16> for Ar<N> {
    fn from_iter<I: IntoIterator<Item = u16>>(it: I) -> Self {
        let mut a = [0u16; N];
        for (d, s) in a.iter_mut().zip(it) {
            *d = s;
        }
        Ar(a)
    }
}

/// Port décimal de WINLUX_TLS_PORT, ou 0.
unsafe fn tls_port() -> u16 {
    let mut v = [0u8; 16];
    let n =
        GetEnvironmentVariableA(c"WINLUX_TLS_PORT".as_ptr(), v.as_mut_ptr().cast(), 16) as usize;
    let mut p: u32 = 0;
    for &c in &v[..n.min(16)] {
        if !c.is_ascii_digit() {
            return 0;
        }
        p = p * 10 + (c - b'0') as u32;
    }
    p as u16
}

static mut NET: [u8; 32768] = [0; 32768];

unsafe fn schannel() {
    out(b"\n-- Schannel --\n");
    let port = tls_port();
    if port == 0 {
        out(b"  (WINLUX_TLS_PORT absent : poignee de main sautee)\n");
        return;
    }
    let mut wsa = [0u8; 512];
    WSAStartup(0x0202, wsa.as_mut_ptr());
    let s = socket(2, 1, 6);
    let mut addr = [0u8; 16];
    addr[0] = 2;
    addr[2..4].copy_from_slice(&port.to_be_bytes());
    addr[4..8].copy_from_slice(&inet_addr(c"127.0.0.1".as_ptr()).to_ne_bytes());
    check!(
        connect(s, addr.as_ptr(), 16) == 0,
        "connexion TCP au serveur local"
    );

    // SCHANNEL_CRED version 4, validation manuelle (certificat auto-signé).
    let mut auth = [0u8; 80];
    auth[0] = 4;
    auth[72] = 0x8;
    let mut w = [0u16; 64];
    let mut cred = SecHandle { lower: 0, upper: 0 };
    let r = AcquireCredentialsHandleW(
        null(),
        wide(b"Microsoft Unified Security Protocol Provider", &mut w),
        2,
        null(),
        auth.as_ptr().cast(),
        null(),
        null(),
        &mut cred,
        null_mut(),
    );
    check!(r == 0, "AcquireCredentialsHandleW (Schannel)");
    let mut host = [0u16; 64];
    let target = wide(b"localhost", &mut host);
    let net = &raw mut NET;
    let mut have = 0usize;
    let mut ctx = SecHandle { lower: 0, upper: 0 };
    let mut first = true;
    let mut status;
    let mut rounds = 0;
    loop {
        rounds += 1;
        let mut ob = [SecBuffer {
            cb: 0,
            ty: 2,
            pv: null_mut(),
        }];
        let mut od = SecBufferDesc {
            version: 0,
            count: 1,
            buffers: ob.as_mut_ptr(),
        };
        let mut ib = [
            SecBuffer {
                cb: have as u32,
                ty: 2,
                pv: (*net).as_mut_ptr(),
            },
            SecBuffer {
                cb: 0,
                ty: 0,
                pv: null_mut(),
            },
        ];
        let mut id = SecBufferDesc {
            version: 0,
            count: 2,
            buffers: ib.as_mut_ptr(),
        };
        let mut attrs = 0u32;
        // ISC_REQ_ALLOCATE_MEMORY | ISC_REQ_STREAM | CONFIDENTIALITY
        status = InitializeSecurityContextW(
            &cred,
            if first { null() } else { &ctx },
            target,
            0x100 | 0x8000 | 0x10,
            0,
            0,
            if first { null_mut() } else { &mut id },
            0,
            &mut ctx,
            &mut od,
            &mut attrs,
            null_mut(),
        );
        first = false;
        if ob[0].cb > 0 && !ob[0].pv.is_null() {
            send(s, ob[0].pv, ob[0].cb as i32, 0);
            FreeContextBuffer(ob[0].pv.cast());
        }
        let extra = if ib[1].ty == 5 { ib[1].cb as usize } else { 0 };
        if status == 0 {
            if extra > 0 {
                core::ptr::copy(
                    (*net).as_ptr().add(have - extra),
                    (*net).as_mut_ptr(),
                    extra,
                );
            }
            have = extra;
            break;
        }
        if status != 0x0009_0312 && status != 0x8009_0318u32 as i32 || rounds > 20 {
            break;
        }
        if status == 0x0009_0312 {
            if extra > 0 {
                core::ptr::copy(
                    (*net).as_ptr().add(have - extra),
                    (*net).as_mut_ptr(),
                    extra,
                );
            }
            have = extra;
            if extra > 0 {
                continue;
            }
        }
        let n = recv(
            s,
            (*net).as_mut_ptr().add(have),
            ((*net).len() - have) as i32,
            0,
        );
        if n <= 0 {
            break;
        }
        have += n as usize;
    }
    check!(status == 0, "poignée de main TLS terminée");
    if status != 0 {
        closesocket(s);
        return;
    }
    let mut info = [0u32; 7];
    QueryContextAttributesW(&ctx, 0x5a, info.as_mut_ptr().cast());
    check!(
        info[0] == 0x800 || info[0] == 0x2000,
        "TLS 1.2 ou 1.3 négocié"
    );
    let mut sizes = [0u32; 5];
    check!(
        QueryContextAttributesW(&ctx, 4, sizes.as_mut_ptr().cast()) == 0,
        "tailles du flux"
    );
    curl_cainfo(&ctx);

    let req = b"GET / HTTP/1.0\r\n\r\n";
    let mut msg = [0u8; 512];
    let hdr = sizes[0] as usize;
    msg[hdr..hdr + req.len()].copy_from_slice(req);
    let p = msg.as_mut_ptr();
    let mut b = [
        SecBuffer {
            cb: sizes[0],
            ty: 7,
            pv: p,
        },
        SecBuffer {
            cb: req.len() as u32,
            ty: 1,
            pv: p.add(hdr),
        },
        SecBuffer {
            cb: sizes[1],
            ty: 6,
            pv: p.add(hdr + req.len()),
        },
        SecBuffer {
            cb: 0,
            ty: 0,
            pv: null_mut(),
        },
    ];
    let mut d = SecBufferDesc {
        version: 0,
        count: 4,
        buffers: b.as_mut_ptr(),
    };
    check!(EncryptMessage(&ctx, 0, &mut d, 0) == 0, "EncryptMessage");
    send(s, p, (b[0].cb + b[1].cb + b[2].cb) as i32, 0);

    let mut got200 = false;
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > 200 {
            break;
        }
        if have == 0 {
            let n = recv(s, (*net).as_mut_ptr(), (*net).len() as i32, 0);
            if n <= 0 {
                break;
            }
            have = n as usize;
        }
        let mut b = [
            SecBuffer {
                cb: have as u32,
                ty: 1,
                pv: (*net).as_mut_ptr(),
            },
            SecBuffer {
                cb: 0,
                ty: 0,
                pv: null_mut(),
            },
            SecBuffer {
                cb: 0,
                ty: 0,
                pv: null_mut(),
            },
            SecBuffer {
                cb: 0,
                ty: 0,
                pv: null_mut(),
            },
        ];
        let mut d = SecBufferDesc {
            version: 0,
            count: 4,
            buffers: b.as_mut_ptr(),
        };
        let r = DecryptMessage(&ctx, &mut d, 0, null_mut());
        if r == 0x8009_0318u32 as i32 {
            let n = recv(
                s,
                (*net).as_mut_ptr().add(have),
                ((*net).len() - have) as i32,
                0,
            );
            if n <= 0 {
                break;
            }
            have += n as usize;
            continue;
        }
        if r != 0 && r != 0x0009_0317 {
            break;
        }
        if b[1].ty == 1 && b[1].cb >= 12 {
            let t = core::slice::from_raw_parts(b[1].pv, 12);
            if t == b"HTTP/1.0 200" {
                got200 = true;
            }
        }
        let extra = b
            .iter()
            .find(|x| x.ty == 5)
            .map(|x| x.cb as usize)
            .unwrap_or(0);
        if extra > 0 {
            core::ptr::copy(
                (*net).as_ptr().add(have - extra),
                (*net).as_mut_ptr(),
                extra,
            );
        }
        have = extra;
        if r == 0x0009_0317 || got200 {
            break;
        }
    }
    check!(got200, "réponse HTTPS déchiffrée (HTTP/1.0 200)");
    DeleteSecurityContext(&ctx);
    FreeCredentialsHandle(&cred);
    closesocket(s);
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    out(b"Winlux - suite HTTPS (crypt32, CryptoAPI, Schannel)\n");
    unsafe {
        certificates();
        hashes();
        protected_data();
        schannel();
        if FAILURES == 0 {
            out(b"\nTous les tests HTTPS passent.\n");
        }
    }
    finish()
}

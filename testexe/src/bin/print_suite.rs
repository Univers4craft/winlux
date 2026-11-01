//! Test PE du système d'impression : inventaire des imprimantes (winspool),
//! impression GDI vers l'imprimante PDF intégrée, flux brut `WritePrinter`
//! et boîte `PrintDlgW` en mode « imprimante par défaut ». Sans CUPS, seule
//! l'imprimante virtuelle PDF existe ; le test reste valable avec CUPS.

#![no_std]
#![no_main]

use winlux_testexe::*;

/// Chaîne ASCII → UTF-16 terminée par un zéro (taille fixe `N`).
fn w<const N: usize>(s: &str) -> [u16; N] {
    let mut out = [0u16; N];
    for (i, b) in s.bytes().take(N - 1).enumerate() {
        out[i] = u16::from(b);
    }
    out
}

/// Lit le début d'un fichier (chemin Windows ANSI).
unsafe fn head(path: &core::ffi::CStr, buf: &mut [u8]) -> usize {
    let h = CreateFileA(
        path.as_ptr(),
        0x8000_0000, // GENERIC_READ
        1,
        core::ptr::null_mut(),
        3, // OPEN_EXISTING
        0,
        core::ptr::null_mut(),
    );
    if h as isize == -1 {
        return 0;
    }
    let mut n = 0u32;
    ReadFile(
        h,
        buf.as_mut_ptr(),
        buf.len() as u32,
        &mut n,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    n as usize
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        // Inventaire : taille requise puis remplissage (PRINTER_INFO_4W).
        let mut needed = 0u32;
        let mut count = 0u32;
        let first = EnumPrintersW(
            6,
            core::ptr::null(),
            4,
            core::ptr::null_mut(),
            0,
            &mut needed,
            &mut count,
        );
        check!(first == 0 && needed >= 24, "EnumPrintersW taille requise");
        let mut buf = [0u64; 512];
        let ok = EnumPrintersW(
            6,
            core::ptr::null(),
            4,
            buf.as_mut_ptr().cast(),
            (buf.len() * 8) as u32,
            &mut needed,
            &mut count,
        );
        check!(ok == 1 && count >= 1, "EnumPrintersW liste");
        let pdf: [u16; 32] = w("Imprimer en PDF (Winlux)");
        let mut found = false;
        for i in 0..count as usize {
            let name = *(buf.as_ptr().cast::<u8>().add(i * 24) as *const *const u16);
            let mut n = 0;
            while *name.add(n) != 0 {
                n += 1;
            }
            if core::slice::from_raw_parts(name, n) == &pdf[..n] && pdf[n] == 0 {
                found = true;
            }
        }
        check!(found, "imprimante PDF intégrée présente");

        let mut size = 0u32;
        let r = GetDefaultPrinterW(core::ptr::null_mut(), &mut size);
        check!(r == 0 && size > 1, "GetDefaultPrinterW taille");
        let papers = DeviceCapabilitiesW(
            pdf.as_ptr(),
            core::ptr::null(),
            2,
            core::ptr::null_mut(),
            core::ptr::null(),
        );
        check!(papers >= 2, "DeviceCapabilitiesW DC_PAPERS");
        let dm_size = DocumentPropertiesW(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            pdf.as_ptr(),
            core::ptr::null_mut(),
            core::ptr::null(),
            0,
        );
        check!(dm_size == 220, "DocumentPropertiesW taille DEVMODEW");

        // Impression GDI vers un fichier PDF.
        let driver: [u16; 9] = w("WINSPOOL");
        let dc = CreateDCW(
            driver.as_ptr(),
            pdf.as_ptr(),
            core::ptr::null(),
            core::ptr::null(),
        );
        check!(!dc.is_null(), "CreateDCW imprimante");
        check!(
            GetDeviceCaps(dc, 88) == 300 && GetDeviceCaps(dc, 8) == 2480,
            "GetDeviceCaps 300 ppp, A4"
        );
        let out = c"Z:\\tmp\\winlux-print-suite.pdf";
        DeleteFileA(out.as_ptr());
        let out_w: [u16; 40] = w("Z:\\tmp\\winlux-print-suite.pdf");
        let title: [u16; 16] = w("Suite Winlux");
        let info = DocInfoW {
            size: core::mem::size_of::<DocInfoW>() as i32,
            doc_name: title.as_ptr(),
            output: out_w.as_ptr(),
            datatype: core::ptr::null(),
            flags: 0,
        };
        check!(StartDocW(dc, &info) > 0, "StartDocW");
        check!(StartPage(dc) > 0, "StartPage");
        let font = CreateFontA(-42, 0, 0, 0, 700, 0, 0, 0, 0, 0, 0, 0, 0, c"Arial".as_ptr());
        SelectObject(dc, font);
        let text = c"Bonjour imprimante";
        TextOutA(dc, 300, 300, text.as_ptr(), 18);
        Ellipse(dc, 300, 600, 900, 1000);
        check!(EndPage(dc) > 0, "EndPage");
        check!(EndDoc(dc) > 0, "EndDoc");
        DeleteDC(dc);
        let mut pdf_bytes = [0u8; 4096];
        let n = head(out, &mut pdf_bytes);
        let pdf_bytes = &pdf_bytes[..n];
        check!(pdf_bytes.starts_with(b"%PDF-1."), "fichier PDF écrit");
        check!(
            contains(pdf_bytes, b"(Bonjour imprimante) Tj"),
            "texte présent dans le PDF"
        );
        check!(
            contains(pdf_bytes, b"/BaseFont /Helvetica-Bold"),
            "police PDF standard"
        );
        DeleteFileA(out.as_ptr());

        // Flux brut : OpenPrinter / StartDocPrinter / WritePrinter.
        let mut hp: Handle = core::ptr::null_mut();
        check!(
            OpenPrinterW(pdf.as_ptr(), &mut hp, core::ptr::null()) == 1,
            "OpenPrinterW"
        );
        let raw = c"Z:\\tmp\\winlux-print-suite.prn";
        DeleteFileA(raw.as_ptr());
        let raw_w: [u16; 40] = w("Z:\\tmp\\winlux-print-suite.prn");
        let doc: [u16; 8] = w("Brut");
        let datatype: [u16; 4] = w("RAW");
        let info1: [*const u16; 3] = [doc.as_ptr(), raw_w.as_ptr(), datatype.as_ptr()];
        check!(
            StartDocPrinterW(hp, 1, info1.as_ptr().cast()) > 0,
            "StartDocPrinterW"
        );
        StartPagePrinter(hp);
        let data = b"\x1bE donnees brutes";
        let mut written = 0u32;
        check!(
            WritePrinter(hp, data.as_ptr(), data.len() as u32, &mut written) == 1
                && written as usize == data.len(),
            "WritePrinter"
        );
        EndPagePrinter(hp);
        check!(EndDocPrinter(hp) == 1, "EndDocPrinter");
        check!(ClosePrinter(hp) == 1, "ClosePrinter");
        let mut raw_bytes = [0u8; 64];
        let n = head(raw, &mut raw_bytes);
        check!(&raw_bytes[..n] == data, "flux brut enregistré");
        DeleteFileA(raw.as_ptr());

        // PrintDlgW sans interface : imprimante par défaut et DC.
        let mut pd = [0u8; 120];
        pd[..4].copy_from_slice(&120u32.to_le_bytes());
        pd[40..44].copy_from_slice(&(0x400u32 | 0x100).to_le_bytes());
        check!(
            PrintDlgW(pd.as_mut_ptr()) == 1,
            "PrintDlgW PD_RETURNDEFAULT"
        );
        let hdc = usize::from_le_bytes([
            pd[32], pd[33], pd[34], pd[35], pd[36], pd[37], pd[38], pd[39],
        ]) as Handle;
        check!(
            !hdc.is_null() && GetDeviceCaps(hdc, 2) == 2,
            "PrintDlgW renvoie un DC imprimante"
        );
        DeleteDC(hdc);
        pd[..4].copy_from_slice(&1u32.to_le_bytes());
        check!(
            PrintDlgW(pd.as_mut_ptr()) == 0 && CommDlgExtendedError() == 1,
            "PrintDlgW taille invalide"
        );
        // Essai manuel de la boîte Windows 11 : WINLUX_PRINT_SUITE_DIALOG=1.
        if GetEnvironmentVariableA(
            c"WINLUX_PRINT_SUITE_DIALOG".as_ptr(),
            core::ptr::null_mut(),
            0,
        ) > 0
        {
            let mut pd = [0u8; 120];
            pd[..4].copy_from_slice(&120u32.to_le_bytes());
            pd[40..44].copy_from_slice(&0x100u32.to_le_bytes()); // PD_RETURNDC
            pd[48..50].copy_from_slice(&1u16.to_le_bytes()); // nMinPage
            pd[50..52].copy_from_slice(&12u16.to_le_bytes()); // nMaxPage
            let shown = PrintDlgW(pd.as_mut_ptr());
            winlux_testexe::out(if shown == 1 {
                b"  boite : Imprimer\n".as_slice()
            } else {
                b"  boite : Annuler\n".as_slice()
            });
        }
        finish()
    }
}

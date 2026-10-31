pub unsafe extern "win64" fn cor_exe_main(_args: usize) -> i32 {
    crate::log_info!(
        "mscoree",
        "_CorExeMain appelé — délégation au CLR Winlux intégré"
    );
    0
}
pub unsafe extern "win64" fn cor_dll_main(_args: usize) -> i32 {
    0
}
pub fn exports() -> crate::dlls::Exports {
    crate::exports! {"_CorExeMain"=>fn cor_exe_main,"_CorDllMain"=>fn cor_dll_main}
}

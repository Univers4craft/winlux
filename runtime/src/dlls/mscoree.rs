pub unsafe extern "win64" fn cor_exe_main(_args:usize)->i32{
    eprintln!("winlux: cet exécutable .NET nécessite un CLR; mscoree.dll n'est pas encore implémentée");
    crate::dlls::kernel32::flush_and_exit(0xC0000135u32 as i32)
}
pub unsafe extern "win64" fn cor_dll_main(_args:usize)->i32{0}
pub fn exports()->crate::dlls::Exports{crate::exports!{"_CorExeMain"=>fn cor_exe_main,"_CorDllMain"=>fn cor_dll_main}}

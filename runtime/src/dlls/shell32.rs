pub unsafe extern "win64" fn sh_get_folder_path_a(_owner:usize,_csidl:i32,_token:usize,_flags:u32,path:*mut u8)->i32{if path.is_null(){return -1;}crate::dlls::put_a(&std::env::var("HOME").unwrap_or_else(|_|"/".into()),path,4096);0}
pub unsafe extern "win64" fn shell_execute_ex_a(_info:*mut u8)->i32{0}
pub fn exports()->crate::dlls::Exports{crate::exports!{"SHGetFolderPathA"=>fn sh_get_folder_path_a,"ShellExecuteExA"=>fn shell_execute_ex_a}}

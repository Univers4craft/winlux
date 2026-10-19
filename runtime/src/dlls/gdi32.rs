pub extern "win64" fn get_stock_object(index:i32)->usize{if index>=0{(index as usize)+1}else{0}}
pub extern "win64" fn delete_object(_obj:usize)->i32{1}
pub extern "win64" fn create_compatible_dc(_dc:usize)->usize{1}
pub extern "win64" fn delete_dc(_dc:usize)->i32{1}
pub extern "win64" fn get_device_caps(_dc:usize,index:i32)->i32{match index{8=>1920,10=>1080,90=>96,_=>0}}
pub fn exports()->crate::dlls::Exports{crate::exports!{"GetStockObject"=>fn get_stock_object,"DeleteObject"=>fn delete_object,"CreateCompatibleDC"=>fn create_compatible_dc,"DeleteDC"=>fn delete_dc,"GetDeviceCaps"=>fn get_device_caps}}

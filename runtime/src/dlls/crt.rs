//! Fonctions de la CRT Microsoft (msvcrt.dll / ucrtbase.dll) : chaînes,
//! conversions, classification de caractères, mathématiques et tri.
//!
//! Sous Windows, `long` fait 32 bits même en 64 bits : `atol`, `strtol`,
//! `labs` et `_ltoa` utilisent donc `i32`, pas le `c_long` de la plateforme.

use std::ffi::c_void;

// ---------- Chaînes étroites (char) ----------

pub unsafe extern "win64" fn strcpy(d:*mut u8,s:*const u8)->*mut u8{let mut i=0;loop{let c=*s.add(i);*d.add(i)=c;if c==0{break;}i+=1;}d}
pub unsafe extern "win64" fn strncpy(d:*mut u8,s:*const u8,n:usize)->*mut u8{let mut i=0;while i<n{let c=*s.add(i);if c==0{break;}*d.add(i)=c;i+=1;}while i<n{*d.add(i)=0;i+=1;}d}
pub unsafe extern "win64" fn strcat(d:*mut u8,s:*const u8)->*mut u8{let base=crate::printf::narrow_len(d);strcpy(d.add(base),s);d}
pub unsafe extern "win64" fn strncat(d:*mut u8,s:*const u8,n:usize)->*mut u8{let base=crate::printf::narrow_len(d);let mut i=0;while i<n{let c=*s.add(i);if c==0{break;}*d.add(base+i)=c;i+=1;}*d.add(base+i)=0;d}
pub unsafe extern "win64" fn strncmp(a:*const u8,b:*const u8,n:usize)->i32{let mut i=0;while i<n{let(x,y)=(*a.add(i),*b.add(i));if x!=y{return x as i32-y as i32;}if x==0{return 0;}i+=1;}0}
pub unsafe extern "win64" fn strchr(s:*const u8,c:i32)->*mut u8{let target=c as u8;let mut i=0;loop{let x=*s.add(i);if x==target{return s.add(i) as *mut u8;}if x==0{return std::ptr::null_mut();}i+=1;}}
pub unsafe extern "win64" fn strrchr(s:*const u8,c:i32)->*mut u8{let target=c as u8;let mut last=std::ptr::null_mut();let mut i=0;loop{let x=*s.add(i);if x==target{last=s.add(i) as *mut u8;}if x==0{break;}i+=1;}last}
pub unsafe extern "win64" fn strstr(h:*const u8,n:*const u8)->*mut u8{if *n==0{return h as *mut u8;}let mut i=0;loop{if *h.add(i)==0{return std::ptr::null_mut();}let mut j=0;while *n.add(j)!=0&&*h.add(i+j)==*n.add(j){j+=1;}if *n.add(j)==0{return h.add(i) as *mut u8;}i+=1;}}
pub unsafe extern "win64" fn strnlen(s:*const u8,n:usize)->usize{let mut i=0;while i<n&&*s.add(i)!=0{i+=1;}i}
pub unsafe extern "win64" fn strdup(s:*const u8)->*mut u8{let n=crate::printf::narrow_len(s);let p=libc::malloc(n+1) as *mut u8;if p.is_null(){return p;}std::ptr::copy_nonoverlapping(s,p,n);*p.add(n)=0;p}
pub unsafe extern "win64" fn stricmp(a:*const u8,b:*const u8)->i32{let mut i=0;loop{let x=(*a.add(i)).to_ascii_lowercase();let y=(*b.add(i)).to_ascii_lowercase();if x!=y{return x as i32-y as i32;}if x==0{return 0;}i+=1;}}
pub unsafe extern "win64" fn strnicmp(a:*const u8,b:*const u8,n:usize)->i32{let mut i=0;while i<n{let x=(*a.add(i)).to_ascii_lowercase();let y=(*b.add(i)).to_ascii_lowercase();if x!=y{return x as i32-y as i32;}if x==0{return 0;}i+=1;}0}
pub unsafe extern "win64" fn strlwr(s:*mut u8)->*mut u8{let mut i=0;loop{let c=*s.add(i);if c==0{break;}*s.add(i)=c.to_ascii_lowercase();i+=1;}s}
pub unsafe extern "win64" fn strupr(s:*mut u8)->*mut u8{let mut i=0;loop{let c=*s.add(i);if c==0{break;}*s.add(i)=c.to_ascii_uppercase();i+=1;}s}

unsafe fn is_delim(c:u8,delim:*const u8)->bool{let mut i=0;loop{let d=*delim.add(i);if d==0{return false;}if d==c{return true;}i+=1;}}
pub unsafe extern "win64" fn strtok_s(s:*mut u8,delim:*const u8,context:*mut *mut u8)->*mut u8{
    if context.is_null(){return std::ptr::null_mut();}
    let mut cur=if s.is_null(){*context}else{s};
    if cur.is_null(){return std::ptr::null_mut();}
    while *cur!=0&&is_delim(*cur,delim){cur=cur.add(1);}
    if *cur==0{*context=std::ptr::null_mut();return std::ptr::null_mut();}
    let start=cur;
    while *cur!=0&&!is_delim(*cur,delim){cur=cur.add(1);}
    if *cur==0{*context=std::ptr::null_mut();}else{*cur=0;*context=cur.add(1);}
    start
}

// ---------- Chaînes larges (wchar_t) ----------

pub unsafe extern "win64" fn wcslen(s:*const u16)->usize{crate::printf::wide_len(s)}
pub unsafe extern "win64" fn wcscpy(d:*mut u16,s:*const u16)->*mut u16{let mut i=0;loop{let c=*s.add(i);*d.add(i)=c;if c==0{break;}i+=1;}d}
pub unsafe extern "win64" fn wcsncpy(d:*mut u16,s:*const u16,n:usize)->*mut u16{let mut i=0;while i<n{let c=*s.add(i);if c==0{break;}*d.add(i)=c;i+=1;}while i<n{*d.add(i)=0;i+=1;}d}
pub unsafe extern "win64" fn wcscat(d:*mut u16,s:*const u16)->*mut u16{let base=crate::printf::wide_len(d);wcscpy(d.add(base),s);d}
pub unsafe extern "win64" fn wcscmp(a:*const u16,b:*const u16)->i32{let mut i=0;loop{let(x,y)=(*a.add(i),*b.add(i));if x!=y{return x as i32-y as i32;}if x==0{return 0;}i+=1;}}
pub unsafe extern "win64" fn wcsncmp(a:*const u16,b:*const u16,n:usize)->i32{let mut i=0;while i<n{let(x,y)=(*a.add(i),*b.add(i));if x!=y{return x as i32-y as i32;}if x==0{return 0;}i+=1;}0}
pub unsafe extern "win64" fn wcschr(s:*const u16,c:u16)->*mut u16{let mut i=0;loop{let x=*s.add(i);if x==c{return s.add(i) as *mut u16;}if x==0{return std::ptr::null_mut();}i+=1;}}
pub unsafe extern "win64" fn wcsrchr(s:*const u16,c:u16)->*mut u16{let mut last=std::ptr::null_mut();let mut i=0;loop{let x=*s.add(i);if x==c{last=s.add(i) as *mut u16;}if x==0{break;}i+=1;}last}
pub unsafe extern "win64" fn wcsstr(h:*const u16,n:*const u16)->*mut u16{if *n==0{return h as *mut u16;}let mut i=0;loop{if *h.add(i)==0{return std::ptr::null_mut();}let mut j=0;while *n.add(j)!=0&&*h.add(i+j)==*n.add(j){j+=1;}if *n.add(j)==0{return h.add(i) as *mut u16;}i+=1;}}

// ---------- Conversions numériques ----------

/// Analyse un entier et renvoie (valeur absolue, négatif, pointeur de fin).
unsafe fn parse_int(s:*const u8,base:i32)->(u64,bool,*const u8){
    let mut p=s;
    while *p==b' '||*p==b'\t'||*p==b'\n'||*p==b'\r'||*p==0x0b||*p==0x0c{p=p.add(1);}
    let mut neg=false;
    if *p==b'+'{p=p.add(1);}else if *p==b'-'{neg=true;p=p.add(1);}
    let mut radix=if base==0{10}else{base};
    if(radix==16||base==0)&&*p==b'0'&&(*p.add(1)==b'x'||*p.add(1)==b'X'){radix=16;p=p.add(2);}
    else if base==0&&*p==b'0'{radix=8;}
    let start=p;
    let mut value:u64=0;
    loop{
        let c=*p;
        let d=match c{b'0'..=b'9'=>(c-b'0')as u32,b'a'..=b'z'=>(c-b'a')as u32+10,b'A'..=b'Z'=>(c-b'A')as u32+10,_=>break};
        if d>=radix as u32{break;}
        value=value.wrapping_mul(radix as u64).wrapping_add(d as u64);
        p=p.add(1);
    }
    if p==start{(0,false,s)}else{(value,neg,p)}
}
unsafe fn set_end(end:*mut *mut u8,p:*const u8){if !end.is_null(){*end=p as *mut u8;}}
pub unsafe extern "win64" fn atoi(s:*const u8)->i32{let(v,n,_)=parse_int(s,10);if n{-(v as i64)as i32}else{v as i32}}
pub unsafe extern "win64" fn atol(s:*const u8)->i32{atoi(s)}
pub unsafe extern "win64" fn atoll(s:*const u8)->i64{let(v,n,_)=parse_int(s,10);if n{-(v as i64)}else{v as i64}}
pub unsafe extern "win64" fn ato64(s:*const u8)->i64{atoll(s)}
pub unsafe extern "win64" fn strtol(s:*const u8,end:*mut *mut u8,base:i32)->i32{let(v,n,p)=parse_int(s,base);set_end(end,p);if n{-(v as i64)as i32}else{v as i32}}
pub unsafe extern "win64" fn strtoul(s:*const u8,end:*mut *mut u8,base:i32)->u32{let(v,_,p)=parse_int(s,base);set_end(end,p);v as u32}
pub unsafe extern "win64" fn strtoll(s:*const u8,end:*mut *mut u8,base:i32)->i64{let(v,n,p)=parse_int(s,base);set_end(end,p);if n{-(v as i64)}else{v as i64}}
pub unsafe extern "win64" fn strtoull(s:*const u8,end:*mut *mut u8,base:i32)->u64{let(v,_,p)=parse_int(s,base);set_end(end,p);v}
pub unsafe extern "win64" fn strtod(s:*const u8,end:*mut *mut u8)->f64{
    let mut p=s;
    while *p==b' '||*p==b'\t'||*p==b'\n'||*p==b'\r'{p=p.add(1);}
    let start=p;
    if *p==b'+'||*p==b'-'{p=p.add(1);}
    let mut seen=false;
    while (*p).is_ascii_digit(){p=p.add(1);seen=true;}
    if *p==b'.'{p=p.add(1);while (*p).is_ascii_digit(){p=p.add(1);seen=true;}}
    if seen&&(*p==b'e'||*p==b'E'){let mut q=p.add(1);if *q==b'+'||*q==b'-'{q=q.add(1);}if (*q).is_ascii_digit(){p=q;while (*p).is_ascii_digit(){p=p.add(1);}}}
    if !seen{set_end(end,s);return 0.0;}
    let n=(p as usize)-(start as usize);
    let text=std::str::from_utf8_unchecked(std::slice::from_raw_parts(start,n));
    set_end(end,p);
    text.parse().unwrap_or(0.0)
}

unsafe fn itoa_impl(value:u64,negative:bool,buf:*mut u8,radix:i32)->*mut u8{
    if buf.is_null(){return buf;}
    let r=if(2..=36).contains(&radix){radix as u64}else{10};
    let digits=b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut tmp=[0u8;72];
    let mut i=tmp.len();
    let mut v=value;
    if v==0{i-=1;tmp[i]=b'0';}
    while v>0{i-=1;tmp[i]=digits[(v%r)as usize];v/=r;}
    if negative{i-=1;tmp[i]=b'-';}
    let len=tmp.len()-i;
    std::ptr::copy_nonoverlapping(tmp.as_ptr().add(i),buf,len);
    *buf.add(len)=0;
    buf
}
pub unsafe extern "win64" fn itoa(value:i32,buf:*mut u8,radix:i32)->*mut u8{itoa_impl(value.unsigned_abs()as u64,value<0,buf,radix)}
pub unsafe extern "win64" fn i64toa(value:i64,buf:*mut u8,radix:i32)->*mut u8{itoa_impl(value.unsigned_abs(),value<0,buf,radix)}
pub unsafe extern "win64" fn ui64toa(value:u64,buf:*mut u8,radix:i32)->*mut u8{itoa_impl(value,false,buf,radix)}
pub unsafe extern "win64" fn ultoa(value:u32,buf:*mut u8,radix:i32)->*mut u8{itoa_impl(value as u64,false,buf,radix)}

// ---------- Classification de caractères ----------

fn ascii(c:i32)->Option<u8>{if(0..=255).contains(&c){Some(c as u8)}else{None}}
pub extern "win64" fn isspace(c:i32)->i32{let b=match ascii(c){Some(b)=>b,None=>return 0};match b{b' '|b'\t'|b'\n'|b'\r'|0x0b|0x0c=>1,_=>0}}
pub extern "win64" fn isalpha(c:i32)->i32{let b=match ascii(c){Some(b)=>b,None=>return 0};((b as char).is_ascii_alphabetic())as i32}
pub extern "win64" fn isdigit(c:i32)->i32{let b=match ascii(c){Some(b)=>b,None=>return 0};((b as char).is_ascii_digit())as i32}
pub extern "win64" fn isupper(c:i32)->i32{let b=match ascii(c){Some(b)=>b,None=>return 0};((b as char).is_ascii_uppercase())as i32}
pub extern "win64" fn islower(c:i32)->i32{let b=match ascii(c){Some(b)=>b,None=>return 0};((b as char).is_ascii_lowercase())as i32}
pub extern "win64" fn toupper(c:i32)->i32{if(0..=255).contains(&c){(c as u8).to_ascii_uppercase()as i32}else{c}}
pub extern "win64" fn tolower(c:i32)->i32{if(0..=255).contains(&c){(c as u8).to_ascii_lowercase()as i32}else{c}}
pub extern "win64" fn isalnum(c:i32)->i32{if isalpha(c)!=0||isdigit(c)!=0{1}else{0}}
pub extern "win64" fn isxdigit(c:i32)->i32{let b=match ascii(c){Some(b)=>b,None=>return 0};((b as char).is_ascii_hexdigit())as i32}
pub extern "win64" fn ispunct(c:i32)->i32{let b=match ascii(c){Some(b)=>b,None=>return 0};((b as char).is_ascii_punctuation())as i32}
pub extern "win64" fn isprint(c:i32)->i32{let b=match ascii(c){Some(b)=>b,None=>return 0};((b as char).is_ascii_graphic()||b==b' ')as i32}
pub extern "win64" fn iscntrl(c:i32)->i32{let b=match ascii(c){Some(b)=>b,None=>return 0};((b as char).is_ascii_control())as i32}
pub extern "win64" fn isgraph(c:i32)->i32{let b=match ascii(c){Some(b)=>b,None=>return 0};((b as char).is_ascii_graphic())as i32}

// ---------- Mathématiques ----------

pub extern "win64" fn sqrt(x:f64)->f64{x.sqrt()}
pub extern "win64" fn fabs(x:f64)->f64{x.abs()}
pub extern "win64" fn sin(x:f64)->f64{x.sin()}
pub extern "win64" fn cos(x:f64)->f64{x.cos()}
pub extern "win64" fn tan(x:f64)->f64{x.tan()}
pub extern "win64" fn asin(x:f64)->f64{x.asin()}
pub extern "win64" fn acos(x:f64)->f64{x.acos()}
pub extern "win64" fn atan(x:f64)->f64{x.atan()}
pub extern "win64" fn atan2(y:f64,x:f64)->f64{y.atan2(x)}
pub extern "win64" fn exp(x:f64)->f64{x.exp()}
pub extern "win64" fn log(x:f64)->f64{x.ln()}
pub extern "win64" fn log10(x:f64)->f64{x.log10()}
pub extern "win64" fn pow(x:f64,y:f64)->f64{x.powf(y)}
pub extern "win64" fn ceil(x:f64)->f64{x.ceil()}
pub extern "win64" fn floor(x:f64)->f64{x.floor()}
pub extern "win64" fn fmod(x:f64,y:f64)->f64{x%y}
pub extern "win64" fn hypot(x:f64,y:f64)->f64{x.hypot(y)}
pub extern "win64" fn sinh(x:f64)->f64{x.sinh()}
pub extern "win64" fn cosh(x:f64)->f64{x.cosh()}
pub extern "win64" fn tanh(x:f64)->f64{x.tanh()}

pub extern "win64" fn crt_abs(v:i32)->i32{v.wrapping_abs()}
pub extern "win64" fn crt_labs(v:i32)->i32{v.wrapping_abs()}
pub extern "win64" fn crt_abs64(v:i64)->i64{v.wrapping_abs()}

// ---------- Tri et recherche ----------

unsafe extern "win64" fn qsort(base:*mut c_void,num:usize,width:usize,compare:usize){
    if base.is_null()||width==0||num<2||compare==0{return;}
    let cmp:extern "win64" fn(*const c_void,*const c_void)->i32=std::mem::transmute(compare);
    let bytes=std::slice::from_raw_parts_mut(base as *mut u8,num*width);
    let mut order:Vec<usize>=(0..num).collect();
    order.sort_by(|&a,&b|{
        let pa=bytes.as_ptr().add(a*width) as *const c_void;
        let pb=bytes.as_ptr().add(b*width) as *const c_void;
        cmp(pa,pb).cmp(&0)
    });
    let snapshot=bytes.to_vec();
    for(i,&src) in order.iter().enumerate(){std::ptr::copy_nonoverlapping(snapshot.as_ptr().add(src*width),bytes.as_mut_ptr().add(i*width),width);}
}
unsafe extern "win64" fn bsearch(key:*const c_void,base:*const c_void,num:usize,width:usize,compare:usize)->*mut c_void{
    if key.is_null()||base.is_null()||width==0||compare==0{return std::ptr::null_mut();}
    let cmp:extern "win64" fn(*const c_void,*const c_void)->i32=std::mem::transmute(compare);
    let bytes=std::slice::from_raw_parts(base as *const u8,num*width);
    let(mut lo,mut hi)=(0usize,num);
    while lo<hi{
        let mid=lo+(hi-lo)/2;
        let elem=bytes.as_ptr().add(mid*width) as *const c_void;
        match cmp(key,elem).cmp(&0){
            std::cmp::Ordering::Less=>hi=mid,
            std::cmp::Ordering::Greater=>lo=mid+1,
            std::cmp::Ordering::Equal=>return bytes.as_ptr().add(mid*width) as *mut c_void,
        }
    }
    std::ptr::null_mut()
}

// ---------- Formatage limité par la taille du tampon ----------

unsafe fn write_limited(dst:*mut u8,size:usize,bytes:&[u8])->(bool,usize){
    if !dst.is_null()&&size>0{
        let n=bytes.len().min(size-1);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(),dst,n);
        *dst.add(n)=0;
    }
    (bytes.len()>=size,bytes.len())
}
unsafe extern "win64" fn imp_snprintf(args:*const u64)->i32{
    let mut v=crate::thunks::VaList::new(args);
    let dst=v.next_u64() as *mut u8;
    let size=v.next_u64() as usize;
    let fmt=v.next_ptr::<u8>();
    let b=crate::printf::format(std::slice::from_raw_parts(fmt,crate::printf::narrow_len(fmt)),&mut v,crate::printf::MSVCRT);
    let(truncated,len)=write_limited(dst,size,&b);
    if truncated{-1}else{len as i32}
}
unsafe extern "win64" fn imp_snprintf_c99(args:*const u64)->i32{
    let mut v=crate::thunks::VaList::new(args);
    let dst=v.next_u64() as *mut u8;
    let size=v.next_u64() as usize;
    let fmt=v.next_ptr::<u8>();
    let b=crate::printf::format(std::slice::from_raw_parts(fmt,crate::printf::narrow_len(fmt)),&mut v,crate::printf::MSVCRT);
    let(_,len)=write_limited(dst,size,&b);
    len as i32
}
unsafe extern "win64" fn imp_snprintf_s(args:*const u64)->i32{
    let mut v=crate::thunks::VaList::new(args);
    let dst=v.next_u64() as *mut u8;
    let size=v.next_u64() as usize;
    let count=v.next_u64() as usize;
    let fmt=v.next_ptr::<u8>();
    let mut b=crate::printf::format(std::slice::from_raw_parts(fmt,crate::printf::narrow_len(fmt)),&mut v,crate::printf::MSVCRT);
    if count<b.len(){b.truncate(count);}
    let(truncated,len)=write_limited(dst,size,&b);
    if truncated&&size<=count{-1}else{len as i32}
}
unsafe extern "win64" fn imp_vsnprintf(args:*const u64)->i32{
    let dst=*args as *mut u8;
    let size=*args.add(1) as usize;
    let fmt=*args.add(2) as *const u8;
    let va=*args.add(3) as *const u64;
    let mut v=crate::thunks::VaList::new(va);
    let b=crate::printf::format(std::slice::from_raw_parts(fmt,crate::printf::narrow_len(fmt)),&mut v,crate::printf::MSVCRT);
    let(truncated,len)=write_limited(dst,size,&b);
    if truncated{-1}else{len as i32}
}
unsafe extern "win64" fn imp_vsnprintf_c99(args:*const u64)->i32{
    let dst=*args as *mut u8;
    let size=*args.add(1) as usize;
    let fmt=*args.add(2) as *const u8;
    let va=*args.add(3) as *const u64;
    let mut v=crate::thunks::VaList::new(va);
    let b=crate::printf::format(std::slice::from_raw_parts(fmt,crate::printf::narrow_len(fmt)),&mut v,crate::printf::MSVCRT);
    let(_,len)=write_limited(dst,size,&b);
    len as i32
}
crate::varargs_thunk!(snprintf_thunk => imp_snprintf);
crate::varargs_thunk!(snprintf_c99_thunk => imp_snprintf_c99);
crate::varargs_thunk!(snprintf_s_thunk => imp_snprintf_s);
crate::varargs_thunk!(vsnprintf_thunk => imp_vsnprintf);
crate::varargs_thunk!(vsnprintf_c99_thunk => imp_vsnprintf_c99);

// ---------- Variantes larges (wchar_t) ----------

/// Formate une chaîne de format large en UTF-16 via le moteur étroit.
unsafe fn format_wide(fmt:*const u16,va:&mut crate::thunks::VaList)->Vec<u16>{
    let narrow={
        let n=crate::printf::wide_len(fmt);
        let text=String::from_utf16_lossy(std::slice::from_raw_parts(fmt,n));
        text.into_bytes()
    };
    let opts=crate::printf::Opts{wide:true,..crate::printf::MSVCRT};
    let out=crate::printf::format(&narrow,va,opts);
    String::from_utf8_lossy(&out).encode_utf16().collect()
}
unsafe fn write_limited_w(dst:*mut u16,size:usize,w:&[u16])->(bool,usize){
    if !dst.is_null()&&size>0{
        let n=w.len().min(size-1);
        std::ptr::copy_nonoverlapping(w.as_ptr(),dst,n);
        *dst.add(n)=0;
    }
    (w.len()>=size,w.len())
}
unsafe fn itow_impl(value:u64,negative:bool,buf:*mut u16,radix:i32)->*mut u16{
    if buf.is_null(){return buf;}
    let r=if(2..=36).contains(&radix){radix as u64}else{10};
    let digits=b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut tmp=[0u16;72];
    let mut i=tmp.len();
    let mut v=value;
    if v==0{i-=1;tmp[i]=b'0' as u16;}
    while v>0{i-=1;tmp[i]=digits[(v%r)as usize]as u16;v/=r;}
    if negative{i-=1;tmp[i]=b'-' as u16;}
    let len=tmp.len()-i;
    std::ptr::copy_nonoverlapping(tmp.as_ptr().add(i),buf,len);
    *buf.add(len)=0;
    buf
}
pub unsafe extern "win64" fn itow(value:i32,buf:*mut u16,radix:i32)->*mut u16{itow_impl(value.unsigned_abs()as u64,value<0,buf,radix)}
pub unsafe extern "win64" fn i64tow(value:i64,buf:*mut u16,radix:i32)->*mut u16{itow_impl(value.unsigned_abs(),value<0,buf,radix)}
pub unsafe extern "win64" fn ui64tow(value:u64,buf:*mut u16,radix:i32)->*mut u16{itow_impl(value,false,buf,radix)}
pub unsafe extern "win64" fn ultow(value:u32,buf:*mut u16,radix:i32)->*mut u16{itow_impl(value as u64,false,buf,radix)}
unsafe extern "win64" fn imp_snwprintf(args:*const u64)->i32{
    let mut v=crate::thunks::VaList::new(args);
    let dst=v.next_u64() as *mut u16;
    let size=v.next_u64() as usize;
    let fmt=v.next_ptr::<u16>();
    let w=format_wide(fmt,&mut v);
    let(truncated,len)=write_limited_w(dst,size,&w);
    if truncated{-1}else{len as i32}
}
unsafe extern "win64" fn imp_snwprintf_c99(args:*const u64)->i32{
    let mut v=crate::thunks::VaList::new(args);
    let dst=v.next_u64() as *mut u16;
    let size=v.next_u64() as usize;
    let fmt=v.next_ptr::<u16>();
    let w=format_wide(fmt,&mut v);
    let(_,len)=write_limited_w(dst,size,&w);
    len as i32
}
unsafe extern "win64" fn imp_snwprintf_s(args:*const u64)->i32{
    let mut v=crate::thunks::VaList::new(args);
    let dst=v.next_u64() as *mut u16;
    let size=v.next_u64() as usize;
    let count=v.next_u64() as usize;
    let fmt=v.next_ptr::<u16>();
    let mut w=format_wide(fmt,&mut v);
    if count<w.len(){w.truncate(count);}
    let(truncated,len)=write_limited_w(dst,size,&w);
    if truncated&&size<=count{-1}else{len as i32}
}
unsafe extern "win64" fn imp_vsnwprintf(args:*const u64)->i32{
    let dst=*args as *mut u16;
    let size=*args.add(1) as usize;
    let fmt=*args.add(2) as *const u16;
    let va=*args.add(3) as *const u64;
    let mut v=crate::thunks::VaList::new(va);
    let w=format_wide(fmt,&mut v);
    let(truncated,len)=write_limited_w(dst,size,&w);
    if truncated{-1}else{len as i32}
}
crate::varargs_thunk!(snwprintf_thunk => imp_snwprintf);
crate::varargs_thunk!(snwprintf_c99_thunk => imp_snwprintf_c99);
crate::varargs_thunk!(snwprintf_s_thunk => imp_snwprintf_s);
crate::varargs_thunk!(vsnwprintf_thunk => imp_vsnwprintf);

pub fn exports()->crate::dlls::Exports{
    crate::exports!{
        "strcpy"=>fn strcpy,"strncpy"=>fn strncpy,"strcat"=>fn strcat,"strncat"=>fn strncat,
        "strncmp"=>fn strncmp,"strchr"=>fn strchr,"strrchr"=>fn strrchr,"strstr"=>fn strstr,
        "strnlen"=>fn strnlen,"strdup"=>fn strdup,"_strdup"=>fn strdup,
        "stricmp"=>fn stricmp,"_stricmp"=>fn stricmp,"_strcmpi"=>fn stricmp,
        "strnicmp"=>fn strnicmp,"_strnicmp"=>fn strnicmp,"_strncmpi"=>fn strnicmp,
        "strlwr"=>fn strlwr,"_strlwr"=>fn strlwr,"strupr"=>fn strupr,"_strupr"=>fn strupr,
        "strtok_s"=>fn strtok_s,
        "wcslen"=>fn wcslen,"wcscpy"=>fn wcscpy,"wcsncpy"=>fn wcsncpy,"wcscat"=>fn wcscat,
        "wcscmp"=>fn wcscmp,"wcsncmp"=>fn wcsncmp,"wcschr"=>fn wcschr,"wcsrchr"=>fn wcsrchr,"wcsstr"=>fn wcsstr,
        "atoi"=>fn atoi,"atol"=>fn atol,"atoll"=>fn atoll,"_atoi64"=>fn ato64,
        "strtol"=>fn strtol,"strtoul"=>fn strtoul,"strtoll"=>fn strtoll,"strtoull"=>fn strtoull,"strtod"=>fn strtod,
        "_itoa"=>fn itoa,"_ltoa"=>fn itoa,"_i64toa"=>fn i64toa,"_ui64toa"=>fn ui64toa,"_ultoa"=>fn ultoa,
        "isalpha"=>fn isalpha,"_isalpha"=>fn isalpha,"isdigit"=>fn isdigit,"_isdigit"=>fn isdigit,
        "isalnum"=>fn isalnum,"_isalnum"=>fn isalnum,"isspace"=>fn isspace,"_isspace"=>fn isspace,
        "isupper"=>fn isupper,"_isupper"=>fn isupper,"islower"=>fn islower,"_islower"=>fn islower,
        "isxdigit"=>fn isxdigit,"_isxdigit"=>fn isxdigit,"ispunct"=>fn ispunct,"_ispunct"=>fn ispunct,
        "isprint"=>fn isprint,"_isprint"=>fn isprint,"iscntrl"=>fn iscntrl,"_iscntrl"=>fn iscntrl,
        "isgraph"=>fn isgraph,"_isgraph"=>fn isgraph,
        "toupper"=>fn toupper,"_toupper"=>fn toupper,"tolower"=>fn tolower,"_tolower"=>fn tolower,
        "sqrt"=>fn sqrt,"fabs"=>fn fabs,"sin"=>fn sin,"cos"=>fn cos,"tan"=>fn tan,
        "asin"=>fn asin,"acos"=>fn acos,"atan"=>fn atan,"atan2"=>fn atan2,"exp"=>fn exp,
        "log"=>fn log,"log10"=>fn log10,"pow"=>fn pow,"ceil"=>fn ceil,"floor"=>fn floor,
        "fmod"=>fn fmod,"hypot"=>fn hypot,"sinh"=>fn sinh,"cosh"=>fn cosh,"tanh"=>fn tanh,
        "abs"=>fn crt_abs,"labs"=>fn crt_labs,"_abs64"=>fn crt_abs64,
        "qsort"=>fn qsort,"bsearch"=>fn bsearch,
        "_snprintf"=>fn snprintf_thunk,"snprintf"=>fn snprintf_c99_thunk,"_snprintf_s"=>fn snprintf_s_thunk,
        "_vsnprintf"=>fn vsnprintf_thunk,"vsnprintf"=>fn vsnprintf_c99_thunk,
        "_itow"=>fn itow,"_ltow"=>fn itow,"_i64tow"=>fn i64tow,"_ui64tow"=>fn ui64tow,"_ultow"=>fn ultow,
        "_snwprintf"=>fn snwprintf_thunk,"swprintf"=>fn snwprintf_c99_thunk,"_snwprintf_s"=>fn snwprintf_s_thunk,
        "_vsnwprintf"=>fn vsnwprintf_thunk,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cs(s:&str)->Vec<u8>{let mut v=s.as_bytes().to_vec();v.push(0);v}

    #[test]
    fn chaines_etouites_copient_comparent_et_cherchent(){
        unsafe{
            let mut buf=[0u8;32];
            strcpy(buf.as_mut_ptr(),cs("abc").as_ptr());
            strcat(buf.as_mut_ptr(),cs("def").as_ptr());
            assert_eq!(&buf[..6],b"abcdef");
            assert_eq!(strlen_like(buf.as_ptr()),6);
            assert_eq!(strncmp(cs("abcdef").as_ptr(),cs("abcxyz").as_ptr(),3),0);
            assert!(strncmp(cs("abc").as_ptr(),cs("abd").as_ptr(),3)<0);
            let p=strchr(buf.as_ptr(),b'd' as i32);
            assert_eq!(*p,b'd');
            let q=strstr(buf.as_ptr(),cs("cde").as_ptr());
            assert_eq!(*q,b'c');
            assert!(strstr(buf.as_ptr(),cs("zz").as_ptr()).is_null());
            assert_eq!(strrchr(buf.as_ptr(),b'a' as i32)as usize, buf.as_ptr() as usize);
            let dup=strdup(buf.as_ptr());
            assert_eq!(std::ffi::CStr::from_ptr(dup as *const i8).to_bytes(),b"abcdef");
            libc::free(dup as *mut c_void);            assert_eq!(stricmp(cs("AbC").as_ptr(),cs("aBc").as_ptr()),0);
            strlwr(buf.as_mut_ptr());
            assert_eq!(&buf[..3],b"abc");
            strupr(buf.as_mut_ptr());
            assert_eq!(&buf[..3],b"ABC");
            let mut nbuf=[0u8;4];
            strncpy(nbuf.as_mut_ptr(),cs("xy").as_ptr(),4);
            assert_eq!(&nbuf,b"xy\0\0");
        }
    }
    unsafe fn strlen_like(p:*const u8)->usize{crate::printf::narrow_len(p)}

    #[test]
    fn strtok_s_decoupe_avec_contexte(){
        unsafe{
            let mut text=cs("un,deux;;trois");
            let delim=cs(",;");
            let mut ctx=std::ptr::null_mut();
            let mut parts=Vec::new();
            let mut tok=strtok_s(text.as_mut_ptr(),delim.as_ptr(),&mut ctx);
            while !tok.is_null(){parts.push(std::ffi::CStr::from_ptr(tok as *const i8).to_str().unwrap().to_string());tok=strtok_s(std::ptr::null_mut(),delim.as_ptr(),&mut ctx);}
            assert_eq!(parts,vec!["un","deux","trois"]);
        }
    }

    #[test]
    fn conversions_numeriques_respectent_base_et_fin(){
        unsafe{
            let a=cs("  -42abc");let b=cs("9223372036854775807");
            assert_eq!(atoi(a.as_ptr()),-42);
            assert_eq!(atoll(b.as_ptr()),i64::MAX);
            let mut end=std::ptr::null_mut();
            let hex=cs("0x1f");assert_eq!(strtol(hex.as_ptr(),&mut end,0),31);assert_eq!(*end,b'\0');
            let oct=cs("777");assert_eq!(strtol(oct.as_ptr(),&mut end,8),511);
            let f=cs("ff");assert_eq!(strtoul(f.as_ptr(),&mut end,16),255);
            let neg=cs("-900000000000");assert_eq!(strtoll(neg.as_ptr(),&mut end,10),-900_000_000_000);
            let dbl=cs("3.5e2 reste");
            let d=strtod(dbl.as_ptr(),&mut end);
            assert!((d-350.0).abs()<1e-9);
            assert_eq!(*end,b' ');
            let none=cs("pas un nombre");
            assert_eq!(strtod(none.as_ptr(),&mut end),0.0);
            assert_eq!(*end,b'p');

            let mut ibuf=[0u8;24];
            itoa(-255,ibuf.as_mut_ptr(),10);
            assert_eq!(std::ffi::CStr::from_ptr(ibuf.as_ptr() as *const i8).to_bytes(),b"-255");
            itoa(255,ibuf.as_mut_ptr(),16);
            assert_eq!(std::ffi::CStr::from_ptr(ibuf.as_ptr() as *const i8).to_bytes(),b"ff");
            i64toa(i64::MIN,ibuf.as_mut_ptr(),10);
            assert_eq!(std::ffi::CStr::from_ptr(ibuf.as_ptr() as *const i8).to_bytes(),i64::MIN.to_string().as_bytes());
        }
    }

    #[test]
    fn classification_et_casse_des_caracteres(){
        assert_eq!(isalpha(b'A' as i32),1);
        assert_eq!(isdigit(b'7' as i32),1);
        assert_eq!(isalnum(b'z' as i32),1);
        assert_eq!(isspace(b' ' as i32),1);
        assert_eq!(isspace(b'x' as i32),0);
        assert_eq!(isupper(b'Q' as i32),1);
        assert_eq!(islower(b'q' as i32),1);
        assert_eq!(isxdigit(b'F' as i32),1);
        assert_eq!(ispunct(b',' as i32),1);
        assert_eq!(iscntrl(0x01),1);
        assert_eq!(isprint(0x20),1);
        assert_eq!(isgraph(0x20),0);
        assert_eq!(toupper(b'a' as i32),b'A' as i32);
        assert_eq!(tolower(b'Z' as i32),b'z' as i32);
    }

    #[test]
    fn mathematiques_de_base(){
        assert!((sqrt(9.0)-3.0).abs()<1e-12);
        assert!((pow(2.0,10.0)-1024.0).abs()<1e-9);
        assert!((fabs(-2.5)-2.5).abs()<1e-12);
        assert!((floor(2.9)-2.0).abs()<1e-12);
        assert!((ceil(2.1)-3.0).abs()<1e-12);
        assert!((atan2(1.0,1.0)-std::f64::consts::FRAC_PI_4).abs()<1e-12);
        assert!((log(std::f64::consts::E)-1.0).abs()<1e-12);
        assert_eq!(crt_abs(-7),7);
        assert_eq!(crt_abs64(-9_000_000_000),9_000_000_000);
        assert_eq!(crt_labs(i32::MIN.wrapping_neg()),i32::MIN.wrapping_neg());
    }

    extern "win64" fn cmp_i32(a:*const c_void,b:*const c_void)->i32{
        let x=unsafe{*(a as *const i32)};
        let y=unsafe{*(b as *const i32)};
        match x.cmp(&y){std::cmp::Ordering::Less=>-1,std::cmp::Ordering::Equal=>0,std::cmp::Ordering::Greater=>1}
    }

    #[test]
    fn formatage_et_conversion_larges(){
        unsafe{
            let fmt:Vec<u16>="%s-%d".encode_utf16().chain(std::iter::once(0)).collect();
            let arg:Vec<u16>="ok".encode_utf16().chain(std::iter::once(0)).collect();
            let args=[arg.as_ptr() as u64,7u64];
            let mut v=crate::thunks::VaList::new(args.as_ptr());
            let out=format_wide(fmt.as_ptr(),&mut v);
            assert_eq!(String::from_utf16(&out).unwrap(),"ok-7");

            let mut wb=[0u16;24];
            itow(-255,wb.as_mut_ptr(),10);
            let end=wb.iter().position(|&c|c==0).unwrap();
            assert_eq!(String::from_utf16(&wb[..end]).unwrap(),"-255");
            i64tow(i64::MIN,wb.as_mut_ptr(),10);
            let end=wb.iter().position(|&c|c==0).unwrap();
            assert_eq!(String::from_utf16(&wb[..end]).unwrap(),i64::MIN.to_string());
        }
    }

    #[test]
    fn qsort_et_bsearch_utilisent_le_comparateur_win64(){
        unsafe{
            let mut data=[5i32,1,4,2,3];
            qsort(data.as_mut_ptr() as *mut c_void,data.len(),std::mem::size_of::<i32>(),cmp_i32 as *const () as usize);
            assert_eq!(data,[1,2,3,4,5]);
            let key=4i32;
            let found=bsearch(&key as *const i32 as *const c_void,data.as_ptr() as *const c_void,data.len(),std::mem::size_of::<i32>(),cmp_i32 as *const () as usize);
            assert_eq!(*(found as *const i32),4);
            let missing=9i32;
            assert!(bsearch(&missing as *const i32 as *const c_void,data.as_ptr() as *const c_void,data.len(),std::mem::size_of::<i32>(),cmp_i32 as *const () as usize).is_null());
        }
    }
}

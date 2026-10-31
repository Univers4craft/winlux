#![allow(dead_code)]
//! Qualité et stabilisation : crash handler amélioré, journalisation
//! structurée et sauvegarde/restauration de session.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Journal structuré
// ---------------------------------------------------------------------------

const JOURNAL_RING_CAP: usize = 1000;
const FLUSH_THRESHOLD: usize = 100;
const FLUSH_INTERVAL_MS: u64 = 5_000;

pub struct JournalEntry {
    pub timestamp_ms: u64,
    pub level: String,
    pub component: String,
    pub message: String,
}

struct JournalState {
    ring: VecDeque<JournalEntry>,
    last_flush_ms: u64,
    file: Option<std::fs::File>,
}

static JOURNAL: Mutex<Option<JournalState>> = Mutex::new(None);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn journal_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("WINLUX_JOURNAL") {
        return std::path::PathBuf::from(p);
    }
    let pid = unsafe { libc::getpid() };
    std::path::PathBuf::from(format!("/tmp/winlux-journal-{pid}.log"))
}

fn init_journal() {
    let path = journal_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    let mut guard = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(JournalState {
        ring: VecDeque::with_capacity(JOURNAL_RING_CAP),
        last_flush_ms: now_ms(),
        file,
    });
}

pub fn journal(level: &str, component: &str, msg: &str) {
    let ts = now_ms();
    let entry = JournalEntry {
        timestamp_ms: ts,
        level: level.to_string(),
        component: component.to_string(),
        message: msg.to_string(),
    };
    let mut guard = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    let state = match guard.as_mut() {
        Some(s) => s,
        None => return,
    };
    state.ring.push_back(entry);
    if state.ring.len() > JOURNAL_RING_CAP {
        state.ring.pop_front();
    }
    let should_flush =
        state.ring.len() >= FLUSH_THRESHOLD || (ts - state.last_flush_ms) >= FLUSH_INTERVAL_MS;
    if should_flush {
        flush_locked(state);
    }
}

fn flush_locked(state: &mut JournalState) {
    let file = match state.file.as_mut() {
        Some(f) => f,
        None => return,
    };
    for entry in state.ring.iter() {
        let _ = writeln!(
            file,
            "{} [{}] {}: {}",
            entry.timestamp_ms, entry.level, entry.component, entry.message
        );
    }
    let _ = file.flush();
    state.ring.clear();
    state.last_flush_ms = now_ms();
}

pub fn flush_journal() {
    let mut guard = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = guard.as_mut() {
        flush_locked(state);
    }
}

// ---------------------------------------------------------------------------
// Crash handler amélioré
// ---------------------------------------------------------------------------

fn crash_dir() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("WINLUX_CRASH_DIR") {
        return std::path::PathBuf::from(p);
    }
    std::path::PathBuf::from("/tmp/winlux-crashes")
}

struct CrashFrame {
    rip: u64,
    rbp: u64,
}

unsafe fn walk_stack(initial_rbp: u64, initial_rip: u64) -> Vec<CrashFrame> {
    let mut frames = Vec::with_capacity(64);
    frames.push(CrashFrame {
        rip: initial_rip,
        rbp: initial_rbp,
    });
    let mut rbp = initial_rbp;
    for _ in 0..63 {
        if rbp == 0 || !rbp.is_multiple_of(8) {
            break;
        }
        let next_rbp = *(rbp as *const u64);
        let ret_addr = *((rbp + 8) as *const u64);
        if ret_addr == 0 {
            break;
        }
        frames.push(CrashFrame {
            rip: ret_addr,
            rbp: next_rbp,
        });
        rbp = next_rbp;
        if rbp == 0 {
            break;
        }
    }
    frames
}

unsafe fn write_crash_fd(fd: i32, s: &[u8]) {
    let _ = libc::write(fd, s.as_ptr() as *const libc::c_void, s.len());
}

unsafe fn write_hex_fd(fd: i32, mut n: u64) {
    let mut buf = [b'0'; 16];
    for i in (0..16).rev() {
        buf[i] = b"0123456789abcdef"[(n & 0xf) as usize];
        n >>= 4;
    }
    write_crash_fd(fd, &buf);
}

unsafe fn write_dec_fd(fd: i32, mut n: u64) {
    if n == 0 {
        write_crash_fd(fd, b"0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    write_crash_fd(fd, &buf[i..]);
}

extern "C" fn quality_crash_handler(sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
    unsafe {
        let uc = ctx as *mut libc::ucontext_t;
        let rip = (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] as u64;
        let rsp = (*uc).uc_mcontext.gregs[libc::REG_RSP as usize] as u64;
        let rbp = (*uc).uc_mcontext.gregs[libc::REG_RBP as usize] as u64;
        let rax = (*uc).uc_mcontext.gregs[libc::REG_RAX as usize] as u64;
        let rbx = (*uc).uc_mcontext.gregs[libc::REG_RBX as usize] as u64;
        let rcx = (*uc).uc_mcontext.gregs[libc::REG_RCX as usize] as u64;
        let rdx = (*uc).uc_mcontext.gregs[libc::REG_RDX as usize] as u64;
        let rsi = (*uc).uc_mcontext.gregs[libc::REG_RSI as usize] as u64;
        let rdi = (*uc).uc_mcontext.gregs[libc::REG_RDI as usize] as u64;
        let addr = (*info).si_addr() as u64;

        let exception_code: u32 = match sig {
            libc::SIGSEGV | libc::SIGBUS => 0xC000_0005,
            libc::SIGILL => 0xC000_001D,
            libc::SIGTRAP => 0x8000_0003,
            libc::SIGFPE => 0xC000_0094,
            _ => 0xC000_0001,
        };

        // Write minidump-like crash file
        let dir = crash_dir();
        let mut path_buf = [0u8; 256];
        let dir_bytes = dir.to_str().unwrap_or("/tmp/winlux-crashes").as_bytes();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let pid = libc::getpid() as u64;

        // Build path: dir/crash-{ts}-{pid}.dump
        let copy_len = dir_bytes.len().min(200);
        path_buf[..copy_len].copy_from_slice(&dir_bytes[..copy_len]);
        let mut pos = copy_len;
        path_buf[pos] = b'/';
        pos += 1;
        // "crash-"
        path_buf[pos..pos + 6].copy_from_slice(b"crash-");
        pos += 6;
        // timestamp as decimal
        let mut dec = [0u8; 20];
        let mut dlen = 0;
        let mut t = ts;
        if t == 0 {
            dec[0] = b'0';
            dlen = 1;
        } else {
            let mut di = 20;
            while t > 0 {
                di -= 1;
                dec[di] = b'0' + (t % 10) as u8;
                t /= 10;
                dlen += 1;
            }
            dec.copy_within(20 - dlen..20, 0);
        }
        path_buf[pos..pos + dlen].copy_from_slice(&dec[..dlen]);
        pos += dlen;
        path_buf[pos] = b'-';
        pos += 1;
        // pid as decimal
        let mut p = pid;
        dlen = 0;
        if p == 0 {
            dec[0] = b'0';
            dlen = 1;
        } else {
            let mut di = 20;
            while p > 0 {
                di -= 1;
                dec[di] = b'0' + (p % 10) as u8;
                p /= 10;
                dlen += 1;
            }
            dec.copy_within(20 - dlen..20, 0);
        }
        path_buf[pos..pos + dlen].copy_from_slice(&dec[..dlen]);
        pos += dlen;
        path_buf[pos..pos + 5].copy_from_slice(b".dump");
        pos += 5;
        path_buf[pos] = 0;

        // Ensure directory exists (best-effort, may fail in signal handler)
        let _ = libc::mkdir(dir_bytes.as_ptr() as *const i8, 0o755);

        let fd = libc::open(
            path_buf.as_ptr() as *const i8,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            0o644,
        );

        if fd >= 0 {
            write_crash_fd(fd, b"WINLUX CRASH DUMP\n");
            write_crash_fd(fd, b"timestamp: ");
            write_dec_fd(fd, ts);
            write_crash_fd(fd, b"\nexception: 0x");
            write_hex_fd(fd, exception_code as u64);
            write_crash_fd(fd, b"\nsignal: ");
            write_dec_fd(fd, sig as u64);
            write_crash_fd(fd, b"\n\nRegisters:\n");
            write_crash_fd(fd, b"  rip=0x");
            write_hex_fd(fd, rip);
            write_crash_fd(fd, b"  rsp=0x");
            write_hex_fd(fd, rsp);
            write_crash_fd(fd, b"  rbp=0x");
            write_hex_fd(fd, rbp);
            write_crash_fd(fd, b"\n  rax=0x");
            write_hex_fd(fd, rax);
            write_crash_fd(fd, b"  rbx=0x");
            write_hex_fd(fd, rbx);
            write_crash_fd(fd, b"  rcx=0x");
            write_hex_fd(fd, rcx);
            write_crash_fd(fd, b"\n  rdx=0x");
            write_hex_fd(fd, rdx);
            write_crash_fd(fd, b"  rsi=0x");
            write_hex_fd(fd, rsi);
            write_crash_fd(fd, b"  rdi=0x");
            write_hex_fd(fd, rdi);
            write_crash_fd(fd, b"\n  fault_addr=0x");
            write_hex_fd(fd, addr);
            write_crash_fd(fd, b"\n\nStack trace:\n");

            let frames = walk_stack(rbp, rip);
            for (i, frame) in frames.iter().enumerate() {
                write_crash_fd(fd, b"  #");
                write_dec_fd(fd, i as u64);
                write_crash_fd(fd, b"  rip=0x");
                write_hex_fd(fd, frame.rip);
                write_crash_fd(fd, b"  rbp=0x");
                write_hex_fd(fd, frame.rbp);
                write_crash_fd(fd, b"\n");
            }

            write_crash_fd(fd, b"\nLoaded modules:\n");
            if let Ok(guard) = crate::loader::MODULES.try_lock() {
                for m in guard.iter() {
                    write_crash_fd(fd, b"  0x");
                    write_hex_fd(fd, m.base as u64);
                    write_crash_fd(fd, b" +0x");
                    write_hex_fd(fd, m.size as u64);
                    write_crash_fd(fd, b"  ");
                    write_crash_fd(fd, m.name.as_bytes());
                    write_crash_fd(fd, b"\n");
                }
            }

            libc::close(fd);
        }

        // Also print to stderr
        let stderr = 2;
        write_crash_fd(stderr, b"\nwinlux: crash dump written to ");
        write_crash_fd(stderr, &path_buf[..pos]);
        write_crash_fd(stderr, b"\n");

        // Flush journal before exit
        // NOTE: We cannot safely take a Mutex in a signal handler if the
        // interrupted thread already held it.  Try-lock avoids deadlocking;
        // if the lock is held we skip the flush rather than hang.
        if let Ok(mut guard) = JOURNAL.try_lock() {
            if let Some(state) = guard.as_mut() {
                flush_locked(state);
            }
        }

        libc::_exit(exception_code as i32);
    }
}

fn install_crash_handler() {
    unsafe {
        let size = 64 * 1024;
        let stack = libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        if stack == libc::MAP_FAILED {
            return;
        }
        let ss = libc::stack_t {
            ss_sp: stack,
            ss_flags: 0,
            ss_size: size,
        };
        libc::sigaltstack(&ss, std::ptr::null_mut());
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = quality_crash_handler as *const () as usize;
        sa.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        for s in [
            libc::SIGSEGV,
            libc::SIGBUS,
            libc::SIGILL,
            libc::SIGFPE,
            libc::SIGTRAP,
        ] {
            libc::sigaction(s, &sa, std::ptr::null_mut());
        }
    }
}

// ---------------------------------------------------------------------------
// Checkpoint sauvegarde / restauration
// ---------------------------------------------------------------------------

const CHECKPOINT_MAGIC: &[u8; 4] = b"WLCK";
const CHECKPOINT_VERSION: u32 = 1;

const SECTION_MODULES: &[u8; 4] = b"MODS";
const SECTION_CUSTOM: &[u8; 4] = b"CUST";

pub struct ModuleRecord {
    pub name: String,
    pub base: u64,
    pub size: u64,
}

pub struct CheckpointData {
    pub modules: Vec<ModuleRecord>,
    pub custom_sections: Vec<(String, Vec<u8>)>,
}

fn checkpoint_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("WINLUX_CHECKPOINT") {
        return std::path::PathBuf::from(p);
    }
    let pid = unsafe { libc::getpid() };
    std::path::PathBuf::from(format!("/tmp/winlux-checkpoint-{pid}.bin"))
}

fn write_u32(w: &mut impl Write, v: u32) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn write_u64(w: &mut impl Write, v: u64) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn write_str(w: &mut impl Write, s: &str) -> std::io::Result<()> {
    write_u32(w, s.len() as u32)?;
    w.write_all(s.as_bytes())
}

fn read_u32(r: &mut impl Read) -> std::io::Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(r: &mut impl Read) -> std::io::Result<u64> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_str(r: &mut impl Read) -> std::io::Result<String> {
    let len = read_u32(r)? as usize;
    if len > 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "string too long in checkpoint",
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

pub fn save_checkpoint() -> std::io::Result<()> {
    let path = checkpoint_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(&path)?;

    // Header
    f.write_all(CHECKPOINT_MAGIC)?;
    write_u32(&mut f, CHECKPOINT_VERSION)?;

    // Modules section
    let modules = crate::loader::list_modules();
    f.write_all(SECTION_MODULES)?;
    let mut section_buf = Vec::new();
    write_u32(&mut section_buf, modules.len() as u32)?;
    for m in &modules {
        write_str(&mut section_buf, &m.name)?;
        write_u64(&mut section_buf, m.base as u64)?;
        write_u64(&mut section_buf, m.size as u64)?;
    }
    write_u32(&mut f, section_buf.len() as u32)?;
    f.write_all(&section_buf)?;

    f.flush()?;
    journal(
        "info",
        "quality",
        &format!("checkpoint saved to {}", path.display()),
    );
    Ok(())
}

pub fn restore_checkpoint() -> std::io::Result<CheckpointData> {
    let path = checkpoint_path();
    let mut f = std::fs::File::open(&path)?;

    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)?;
    if &magic != CHECKPOINT_MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "bad checkpoint magic",
        ));
    }
    let version = read_u32(&mut f)?;
    if version != CHECKPOINT_VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unsupported checkpoint version",
        ));
    }

    let mut data = CheckpointData {
        modules: Vec::new(),
        custom_sections: Vec::new(),
    };

    loop {
        let mut tag = [0u8; 4];
        match f.read_exact(&mut tag) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
        let section_len = read_u32(&mut f)? as usize;
        let mut section_data = vec![0u8; section_len];
        f.read_exact(&mut section_data)?;
        let mut cursor = std::io::Cursor::new(&section_data);

        if &tag == SECTION_MODULES {
            let count = read_u32(&mut cursor)? as usize;
            for _ in 0..count {
                let name = read_str(&mut cursor)?;
                let base = read_u64(&mut cursor)?;
                let size = read_u64(&mut cursor)?;
                data.modules.push(ModuleRecord { name, base, size });
            }
        } else if &tag == SECTION_CUSTOM {
            let name = read_str(&mut cursor)?;
            let mut payload = Vec::new();
            cursor.read_to_end(&mut payload)?;
            data.custom_sections.push((name, payload));
        }
    }

    journal(
        "info",
        "quality",
        &format!("checkpoint restored from {}", path.display()),
    );
    Ok(data)
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

pub fn init() {
    init_journal();
    install_crash_handler();
    journal("info", "quality", "quality module initialized");
}

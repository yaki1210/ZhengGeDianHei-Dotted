//! Current-user Windows font installation backend.
//!
//! The backend deliberately owns only files whose names start with
//! `ZhengGeDianHei16-` in the per-user fonts directory.  It never sweeps
//! system fonts or removes unrelated registry values.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FAMILY: &str = "ZhengGeDianHei 16";
const REG_VALUE: &str = "ZhengGeDianHei 16 (TrueType)";
const MARKER_NAME: &str = ".zgd16-active";
const FONT_PREFIX: &str = "ZhengGeDianHei16-";

/// Pixel shape used by a font variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Dots,
    Squares,
}

/// A selectable font variant.  Valid combinations are Dots 70/80/90 and
/// Squares 70/80/100 (100 is the original solid-square font).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variant {
    pub shape: Shape,
    pub density: u8,
}

impl Variant {
    pub const fn dots(density: u8) -> Self {
        Self { shape: Shape::Dots, density }
    }

    pub const fn squares(density: u8) -> Self {
        Self { shape: Shape::Squares, density }
    }

    pub fn is_valid(self) -> bool {
        match self.shape {
            Shape::Dots => matches!(self.density, 70 | 80 | 90),
            Shape::Squares => matches!(self.density, 70 | 80 | 100),
        }
    }

    pub fn key(self) -> String {
        match self.shape {
            Shape::Dots => format!("dots{}", self.density),
            Shape::Squares if self.density == 100 => "original".to_owned(),
            Shape::Squares => format!("squares{}", self.density),
        }
    }

    pub fn source_filename(self, halfwidth: bool) -> String {
        let stem = match self.shape {
            Shape::Dots => format!("Dots{}", self.density),
            Shape::Squares if self.density == 100 => "Original".to_owned(),
            Shape::Squares => format!("Squares{}", self.density),
        };
        if halfwidth {
            format!("{FONT_PREFIX}{stem}-HW.ttf")
        } else {
            format!("{FONT_PREFIX}{stem}.ttf")
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "original" => Some(Self::squares(100)),
            "dots70" => Some(Self::dots(70)),
            "dots80" => Some(Self::dots(80)),
            "dots90" => Some(Self::dots(90)),
            "squares70" => Some(Self::squares(70)),
            "squares80" => Some(Self::squares(80)),
            _ => None,
        }
    }
}

/// Whether the selected font is full-width or terminal-compatible half-width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidthMode {
    Full,
    Half,
}

impl WidthMode {
    fn as_marker(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Half => "half",
        }
    }

    fn from_marker(value: &str) -> Option<Self> {
        match value {
            "full" => Some(Self::Full),
            "half" => Some(Self::Half),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveFont {
    pub variant: Variant,
    pub width: WidthMode,
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallState {
    NotInstalled,
    Installed(ActiveFont),
    /// The registry points at a file that is not one of this backend's
    /// managed files, or the marker/file hash no longer matches.
    Mismatch { registry_path: Option<PathBuf>, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontStatus {
    pub state: InstallState,
    pub registry_path: Option<PathBuf>,
    pub marker_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallReport {
    pub active: ActiveFont,
    pub replaced: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisableReport {
    pub removed_files: Vec<PathBuf>,
    pub removed_registry_value: bool,
}

#[derive(Debug)]
pub enum BackendError {
    InvalidVariant(Variant),
    MissingSource(PathBuf),
    Io(io::Error),
    Registry(String),
    Win32 { operation: &'static str, code: u32 },
    Unsupported,
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVariant(v) => write!(f, "invalid font variant: {:?}", v),
            Self::MissingSource(p) => write!(f, "font source does not exist: {}", p.display()),
            Self::Io(e) => e.fmt(f),
            Self::Registry(e) => write!(f, "registry error: {e}"),
            Self::Win32 { operation, code } => write!(f, "{operation} failed (Win32 error {code})"),
            Self::Unsupported => write!(f, "font installation is only supported on Windows"),
        }
    }
}

impl std::error::Error for BackendError {}
impl From<io::Error> for BackendError {
    fn from(value: io::Error) -> Self { Self::Io(value) }
}

/// Filesystem and registry locations used by the backend.
#[derive(Clone, Debug)]
pub struct FontBackend {
    pub fonts_dir: PathBuf,
    pub install_dir: PathBuf,
}

impl FontBackend {
    /// Construct a backend using a bundled `fonts` directory.
    pub fn new(fonts_dir: impl Into<PathBuf>) -> Self {
        Self { fonts_dir: fonts_dir.into(), install_dir: default_install_dir() }
    }

    pub fn with_install_dir(fonts_dir: impl Into<PathBuf>, install_dir: impl Into<PathBuf>) -> Self {
        Self { fonts_dir: fonts_dir.into(), install_dir: install_dir.into() }
    }

    pub fn marker_path(&self) -> PathBuf { self.install_dir.join(MARKER_NAME) }

    /// Install one managed variant for the current user.  The source is copied
    /// through a temporary file before replacing the managed destination.
    pub fn install(&self, variant: Variant, width: WidthMode) -> Result<InstallReport, BackendError> {
        if !variant.is_valid() { return Err(BackendError::InvalidVariant(variant)); }
        let source = self.fonts_dir.join(variant.source_filename(width == WidthMode::Half));
        if !source.is_file() { return Err(BackendError::MissingSource(source)); }

        #[cfg(not(windows))]
        { let _ = (variant, width, source); return Err(BackendError::Unsupported); }

        #[cfg(windows)]
        {
            fs::create_dir_all(&self.install_dir)?;
            let destination = self.install_dir.join(format!("{FONT_PREFIX}{}.ttf", variant.key()));
            let temp = self.install_dir.join(format!(".{FONT_PREFIX}{}.tmp", variant.key()));
            let previous = self.managed_registry_path()?;
            fs::copy(&source, &temp)?;
            let hash = sha256_file(&temp)?;
            if destination.exists() {
                unsafe { remove_font_resource(&destination); }
                let _ = fs::remove_file(&destination);
            }
            fs::rename(&temp, &destination)?;
            unsafe { if add_font_resource(&destination) == 0 { return Err(last_win32("AddFontResourceW")); } }
            set_registry_value(&destination)?;
            write_marker(&self.marker_path(), variant, width, &hash, &destination)?;
            // Drop the previous managed variant only after the replacement is
            // registered successfully.  An unrelated registry path is never
            // touched.
            if let Some(old) = previous.as_ref() {
                let old_owned = old != &destination
                    && old.starts_with(&self.install_dir)
                    && old.file_name().and_then(|n| n.to_str())
                        .map_or(false, |n| n.starts_with(FONT_PREFIX));
                if old_owned {
                    unsafe { remove_font_resource(old); }
                    let _ = fs::remove_file(old);
                }
            }
            broadcast_font_change();
            Ok(InstallReport { active: ActiveFont { variant, width, path: destination, sha256: hash }, replaced: previous })
        }
    }

    /// Remove only this backend's managed font registration and files.
    pub fn disable(&self) -> Result<DisableReport, BackendError> {
        #[cfg(not(windows))]
        { return Err(BackendError::Unsupported); }

        #[cfg(windows)]
        {
            let mut removed_files = Vec::new();
            let mut owns_registry_value = false;
            if let Some(path) = self.managed_registry_path()? {
                owns_registry_value = path.starts_with(&self.install_dir)
                    && path.file_name().and_then(|n| n.to_str())
                        .map_or(false, |n| n.starts_with(FONT_PREFIX));
                unsafe { remove_font_resource(&path); }
                if owns_registry_value {
                    if fs::remove_file(&path).is_ok() { removed_files.push(path); }
                }
            }
            let mut removed_registry_value = false;
            if owns_registry_value && registry_value_exists()? {
                delete_registry_value()?;
                removed_registry_value = true;
            }
            if self.install_dir.is_dir() {
                for entry in fs::read_dir(&self.install_dir)? {
                    let path = entry?.path();
                    if path.file_name().and_then(|n| n.to_str()).map_or(false, |n| n.starts_with(FONT_PREFIX) && n.ends_with(".ttf")) {
                        unsafe { remove_font_resource(&path); }
                        if fs::remove_file(&path).is_ok() { removed_files.push(path); }
                    }
                }
            }
            let _ = fs::remove_file(self.marker_path());
            broadcast_font_change();
            Ok(DisableReport { removed_files, removed_registry_value })
        }
    }

    pub fn check(&self) -> Result<FontStatus, BackendError> {
        let marker_path = self.marker_path();
        #[cfg(not(windows))]
        { return Ok(FontStatus { state: InstallState::NotInstalled, registry_path: None, marker_path }); }

        #[cfg(windows)]
        {
            let registry_path = self.managed_registry_path()?;
            let Some(path) = registry_path.clone() else {
                return Ok(FontStatus { state: InstallState::NotInstalled, registry_path, marker_path });
            };
            let marker = read_marker(&marker_path);
            if !path.starts_with(&self.install_dir) || !path.is_file() {
                return Ok(FontStatus { state: InstallState::Mismatch { registry_path: Some(path), reason: "registry path is not a managed existing font".into() }, registry_path, marker_path });
            }
            let hash = sha256_file(&path)?;
            let Some((variant, width, expected_hash)) = marker else {
                return Ok(FontStatus { state: InstallState::Mismatch { registry_path: Some(path), reason: "active marker is missing or invalid".into() }, registry_path, marker_path });
            };
            if expected_hash != hash {
                return Ok(FontStatus { state: InstallState::Mismatch { registry_path: Some(path), reason: "font hash differs from active marker".into() }, registry_path, marker_path });
            }
            Ok(FontStatus { state: InstallState::Installed(ActiveFont { variant, width, path, sha256: hash }), registry_path, marker_path })
        }
    }
}

pub fn default_install_dir() -> PathBuf {
    #[cfg(windows)]
    { std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")) .join("Microsoft/Windows/Fonts") }
    #[cfg(not(windows))]
    { PathBuf::from(".zgd16-fonts") }
}

fn sha256_file(path: &Path) -> Result<String, BackendError> {
    let bytes = fs::read(path)?;
    Ok(sha256(&bytes))
}

// Small dependency-free SHA-256 implementation for marker integrity checks.
fn sha256(input: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
        0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
        0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
        0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
        0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
        0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
        0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
        0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
    ];
    let mut h = [0x6a09e667u32,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19];
    let bit_len = (input.len() as u64) * 8;
    let mut data = input.to_vec(); data.push(0x80);
    while data.len() % 64 != 56 { data.push(0); }
    data.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in data.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 { w[i] = u32::from_be_bytes([chunk[i*4],chunk[i*4+1],chunk[i*4+2],chunk[i*4+3]]); }
        for i in 16..64 { let s0 = w[i-15].rotate_right(7)^w[i-15].rotate_right(18)^(w[i-15]>>3); let s1 = w[i-2].rotate_right(17)^w[i-2].rotate_right(19)^(w[i-2]>>10); w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1); }
        let (mut a,mut b,mut c,mut d,mut e,mut f,mut g,mut hh)=(h[0],h[1],h[2],h[3],h[4],h[5],h[6],h[7]);
        for i in 0..64 { let s1=e.rotate_right(6)^e.rotate_right(11)^e.rotate_right(25); let ch=(e&f)^((!e)&g); let t1=hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]); let s0=a.rotate_right(2)^a.rotate_right(13)^a.rotate_right(22); let maj=(a&b)^(a&c)^(b&c); let t2=s0.wrapping_add(maj); hh=g;g=f;f=e;e=d.wrapping_add(t1);d=c;c=b;b=a;a=t1.wrapping_add(t2); }
        h[0]=h[0].wrapping_add(a);h[1]=h[1].wrapping_add(b);h[2]=h[2].wrapping_add(c);h[3]=h[3].wrapping_add(d);h[4]=h[4].wrapping_add(e);h[5]=h[5].wrapping_add(f);h[6]=h[6].wrapping_add(g);h[7]=h[7].wrapping_add(hh);
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

impl FontBackend {
    #[cfg(windows)]
    fn managed_registry_path(&self) -> Result<Option<PathBuf>, BackendError> {
        let Some(value) = registry_read_value()? else { return Ok(None); };
        let path = PathBuf::from(value);
        if path.is_absolute() { Ok(Some(path)) } else { Ok(Some(self.install_dir.join(path))) }
    }

    #[cfg(not(windows))]
    fn managed_registry_path(&self) -> Result<Option<PathBuf>, BackendError> {
        Ok(None)
    }
}

#[cfg(windows)]
fn write_marker(path: &Path, variant: Variant, width: WidthMode, hash: &str, destination: &Path) -> Result<(), BackendError> {
    let body = format!("key={}\nwidth={}\nsha256={}\npath={}\n", variant.key(), width.as_marker(), hash, destination.display());
    let tmp = path.with_extension("tmp"); fs::write(&tmp, body)?; fs::rename(tmp, path)?; Ok(())
}

#[cfg(windows)]
fn read_marker(path: &Path) -> Option<(Variant, WidthMode, String)> {
    let text = fs::read_to_string(path).ok()?; let mut key=None; let mut width=None; let mut hash=None;
    for line in text.lines() { let (k,v)=line.split_once('=')?; match k { "key"=>key=Variant::from_key(v), "width"=>width=WidthMode::from_marker(v), "sha256"=>hash=Some(v.to_owned()), _=>{} } }
    Some((key?, width?, hash?))
}

#[cfg(not(windows))]
fn read_marker(_: &Path) -> Option<(Variant, WidthMode, String)> { None }

#[cfg(windows)]
fn last_win32(operation: &'static str) -> BackendError { BackendError::Win32 { operation, code: unsafe { std::io::Error::last_os_error().raw_os_error().unwrap_or(1) as u32 } } }

#[cfg(windows)]
fn broadcast_font_change() { unsafe { SendMessageTimeoutW(HWND_BROADCAST, WM_FONTCHANGE, 0, 0, SMTO_ABORTIFHUNG, 2000, std::ptr::null_mut()); } }

#[cfg(not(windows))]
fn broadcast_font_change() {}

#[cfg(windows)]
fn registry_value_exists() -> Result<bool, BackendError> { Ok(registry_read_value()?.is_some()) }
#[cfg(windows)]
fn set_registry_value(path: &Path) -> Result<(), BackendError> { registry_write_value(&path.to_string_lossy()) }
#[cfg(windows)]
fn delete_registry_value() -> Result<(), BackendError> { registry_delete_value() }

#[cfg(windows)]
use std::ffi::c_void;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
const HKEY_CURRENT_USER: *mut c_void = 0x80000001usize as *mut c_void;
#[cfg(windows)]
const KEY_READ: u32 = 0x20019;
#[cfg(windows)]
const KEY_WRITE: u32 = 0x20006;
#[cfg(windows)]
const REG_SZ: u32 = 1;
#[cfg(windows)]
const REG_KEY: &str = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts";
#[cfg(windows)]
const HWND_BROADCAST: *mut c_void = 0xffffusize as *mut c_void;
#[cfg(windows)]
const WM_FONTCHANGE: u32 = 0x001d;
#[cfg(windows)]
const SMTO_ABORTIFHUNG: u32 = 0x0002;

#[cfg(windows)]
#[link(name="gdi32")] extern "system" { fn AddFontResourceW(path: *const u16) -> i32; fn RemoveFontResourceW(path: *const u16) -> i32; }
#[cfg(windows)]
#[link(name="user32")] extern "system" { fn SendMessageTimeoutW(hwnd:*mut c_void,msg:u32,wparam:usize,lparam:isize,flags:u32,timeout:u32,result:*mut usize)->usize; }
#[cfg(windows)]
#[link(name="advapi32")] extern "system" { fn RegOpenKeyExW(hkey:*mut c_void,subkey:*const u16,options:u32,sam:u32,result:*mut *mut c_void)->i32; fn RegCreateKeyExW(hkey:*mut c_void,subkey:*const u16,reserved:u32,class:*mut u16,options:u32,sam:u32,security:*mut c_void,result:*mut *mut c_void,disposition:*mut u32)->i32; fn RegQueryValueExW(key:*mut c_void,name:*const u16,reserved:*mut u32,kind:*mut u32,data:*mut u8,size:*mut u32)->i32; fn RegSetValueExW(key:*mut c_void,name:*const u16,reserved:u32,kind:u32,data:*const u8,size:u32)->i32; fn RegDeleteValueW(key:*mut c_void,name:*const u16)->i32; fn RegCloseKey(key:*mut c_void)->i32; }

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> { std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect() }
#[cfg(windows)]
fn registry_read_value() -> Result<Option<String>, BackendError> { unsafe { let mut key=std::ptr::null_mut(); let rc=RegOpenKeyExW(HKEY_CURRENT_USER,wide(REG_KEY).as_ptr(),0,KEY_READ,&mut key); if rc!=0 { return Ok(None); } let mut kind=0; let mut size=0; let n=wide(REG_VALUE); let rc=RegQueryValueExW(key,n.as_ptr(),std::ptr::null_mut(),&mut kind,std::ptr::null_mut(),&mut size); if rc!=0 || kind!=REG_SZ { RegCloseKey(key); return Ok(None); } let mut buf=vec![0u8;size as usize]; let rc=RegQueryValueExW(key,n.as_ptr(),std::ptr::null_mut(),&mut kind,buf.as_mut_ptr(),&mut size); RegCloseKey(key); if rc!=0 { return Err(BackendError::Win32{operation:"RegQueryValueExW",code:rc as u32}); } let words=std::slice::from_raw_parts(buf.as_ptr() as *const u16,size as usize/2); Ok(Some(String::from_utf16_lossy(words).trim_end_matches('\0').to_owned())) } }
#[cfg(windows)]
fn registry_write_value(value:&str)->Result<(),BackendError>{unsafe{let mut key=std::ptr::null_mut();let mut disp=0;let rc=RegCreateKeyExW(HKEY_CURRENT_USER,wide(REG_KEY).as_ptr(),0,std::ptr::null_mut(),0,KEY_WRITE,std::ptr::null_mut(),&mut key,&mut disp);if rc!=0{return Err(BackendError::Win32{operation:"RegCreateKeyExW",code:rc as u32});}let mut data=wide(value);let rc=RegSetValueExW(key,wide(REG_VALUE).as_ptr(),0,REG_SZ,data.as_mut_ptr() as *const u8,(data.len()*2) as u32);RegCloseKey(key);if rc!=0{Err(BackendError::Win32{operation:"RegSetValueExW",code:rc as u32})}else{Ok(())}}}
#[cfg(windows)]
fn registry_delete_value()->Result<(),BackendError>{unsafe{let mut key=std::ptr::null_mut();let rc=RegOpenKeyExW(HKEY_CURRENT_USER,wide(REG_KEY).as_ptr(),0,KEY_WRITE,&mut key);if rc!=0{return Ok(());}let rc=RegDeleteValueW(key,wide(REG_VALUE).as_ptr());RegCloseKey(key);if rc!=0 && rc!=2{Err(BackendError::Win32{operation:"RegDeleteValueW",code:rc as u32})}else{Ok(())}}}
#[cfg(windows)]
unsafe fn add_font_resource(path:&Path)->i32{AddFontResourceW(wide(&path.to_string_lossy()).as_ptr())}
#[cfg(windows)]
unsafe fn remove_font_resource(path:&Path)->i32{RemoveFontResourceW(wide(&path.to_string_lossy()).as_ptr())}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn variant_names_are_stable(){ assert_eq!(Variant::dots(70).source_filename(false),"ZhengGeDianHei16-Dots70.ttf"); assert_eq!(Variant::squares(100).source_filename(true),"ZhengGeDianHei16-Original-HW.ttf"); assert!(!Variant::dots(100).is_valid()); }
    #[test] fn sha256_known_vector(){ assert_eq!(sha256(b"abc"),"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"); }
}

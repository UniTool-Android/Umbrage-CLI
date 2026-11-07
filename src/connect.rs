//! Device discovery and connection.
//!
//! Mirrors the GUI's `connect_on_worker` sequence, minus every QML concern:
//! find and open a port, build a `Device` with the DA/auth/preloader blobs,
//! handshake with `init()`, then drop into DA mode.

use std::ffi::c_char;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::Parser;

use penumbra_mtk::port::{ConnectionType, MtkPort, PortBackend, PortType};
use penumbra_mtk::{Device, DeviceBuilder};

use crate::repo;

/// `Device` borrows the DA/auth/preloader blobs for its whole lifetime, and
/// the CLI hands the device around behind a concrete `'static` type. Leaking
/// the buffers is the same trick the GUI uses, and it is free here: the
/// process runs one command and exits.
fn leak(bytes: Vec<u8>) -> &'static [u8] {
    Box::leak(bytes.into_boxed_slice())
}

fn read_blob(path: &Path) -> Result<&'static [u8]> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if bytes.is_empty() {
        bail!("{} is empty", path.display());
    }
    Ok(leak(bytes))
}

#[derive(Parser, Debug, Clone)]
pub struct ConnectOpts {
    /// DA (Download Agent) file
    #[arg(long, global = true)]
    pub da: Option<PathBuf>,

    /// Auth/SBC file, required on DAA-protected devices
    #[arg(long, global = true)]
    pub auth: Option<PathBuf>,

    /// Preloader file
    #[arg(long, global = true)]
    pub preloader: Option<PathBuf>,

    /// USB backend: auto, libusb, or adbify (Android app-owned USB session).
    /// The nusb and serialport backends are feature-gated off in this build.
    #[arg(long, global = true, default_value = "auto")]
    pub backend: String,

    /// Restrict discovery to this USB vendor ID (hex or decimal)
    #[arg(long, global = true)]
    pub vid: Option<String>,

    /// Restrict discovery to this USB product ID (hex or decimal)
    #[arg(long, global = true)]
    pub pid: Option<String>,

    /// Seconds to keep waiting for a device to appear
    #[arg(long, global = true, default_value_t = 15)]
    pub wait: u64,

    /// Use the online template repository for this device's
    /// da/auth/preloader instead of passing them by hand
    ///
    /// Any file you do pass explicitly (--da/--auth/--preloader) still wins,
    /// so a template can supply just the pieces you left out.
    #[arg(long, global = true)]
    pub template: Option<String>,

    /// Template version id (default: the repository's recommended one)
    #[arg(long, global = true, requires = "template")]
    pub template_version: Option<u32>,

    /// Re-download template files even if a verified copy is cached
    #[arg(long, global = true, requires = "template")]
    pub refresh_template: bool,
}


pub enum RuntimePort {
    Native(PortType),
    #[cfg(target_os = "android")]
    Adbify(crate::android_ipc::AndroidIpcPort),
}

impl MtkPort for RuntimePort {
    fn open(&mut self) -> penumbra_mtk::error::Result<()> { match self { Self::Native(p) => p.open(), #[cfg(target_os = "android")] Self::Adbify(p) => p.open() } }
    fn close(&mut self) -> penumbra_mtk::error::Result<()> { match self { Self::Native(p) => p.close(), #[cfg(target_os = "android")] Self::Adbify(p) => p.close() } }
    fn reenumerate(&mut self, v: u16, p: u16) -> penumbra_mtk::error::Result<()> { match self { Self::Native(x) => x.reenumerate(v,p), #[cfg(target_os = "android")] Self::Adbify(x) => x.reenumerate(v,p) } }
    fn read_exact(&mut self, b: &mut [u8]) -> penumbra_mtk::error::Result<usize> { match self { Self::Native(x) => x.read_exact(b), #[cfg(target_os = "android")] Self::Adbify(x) => x.read_exact(b) } }
    fn write_all(&mut self, b: &[u8]) -> penumbra_mtk::error::Result<()> { match self { Self::Native(x) => x.write_all(b), #[cfg(target_os = "android")] Self::Adbify(x) => x.write_all(b) } }
    fn flush(&mut self) -> penumbra_mtk::error::Result<()> { match self { Self::Native(x) => x.flush(), #[cfg(target_os = "android")] Self::Adbify(x) => x.flush() } }
    fn get_baudrate(&self) -> u32 { match self { Self::Native(x) => x.get_baudrate(), #[cfg(target_os = "android")] Self::Adbify(x) => x.get_baudrate() } }
    fn get_port_name(&self) -> String { match self { Self::Native(x) => x.get_port_name(), #[cfg(target_os = "android")] Self::Adbify(x) => x.get_port_name() } }
    fn set_timeout(&mut self, t: Duration) -> penumbra_mtk::error::Result<()> { match self { Self::Native(x) => x.set_timeout(t), #[cfg(target_os = "android")] Self::Adbify(x) => x.set_timeout(t) } }
    fn get_timeout(&self) -> Duration { match self { Self::Native(x) => x.get_timeout(), #[cfg(target_os = "android")] Self::Adbify(x) => x.get_timeout() } }
    fn connection_type(&self) -> ConnectionType { match self { Self::Native(x) => x.connection_type(), #[cfg(target_os = "android")] Self::Adbify(x) => x.connection_type() } }
    fn set_connection_type(&mut self, t: ConnectionType) -> penumbra_mtk::error::Result<()> { match self { Self::Native(x) => x.set_connection_type(t), #[cfg(target_os = "android")] Self::Adbify(x) => x.set_connection_type(t) } }
    fn ctrl_out(&mut self, rt: u8, r: u8, v: u16, i: u16, d: &[u8]) -> penumbra_mtk::error::Result<()> { match self { Self::Native(x) => x.ctrl_out(rt,r,v,i,d), #[cfg(target_os = "android")] Self::Adbify(x) => x.ctrl_out(rt,r,v,i,d) } }
    fn ctrl_in(&mut self, rt: u8, r: u8, v: u16, i: u16, n: usize) -> penumbra_mtk::error::Result<Vec<u8>> { match self { Self::Native(x) => x.ctrl_in(rt,r,v,i,n), #[cfg(target_os = "android")] Self::Adbify(x) => x.ctrl_in(rt,r,v,i,n) } }
}

pub struct DeviceHandle {
    pub device: Device<'static, RuntimePort>,
}

impl ConnectOpts {
    fn backend(&self) -> Result<PortBackend> {
        match self.backend.to_ascii_lowercase().as_str() {
            "auto" => Ok(PortBackend::Auto),
            "libusb" => Ok(PortBackend::Libusb),
            "adbify" => Ok(PortBackend::Auto),
            other => bail!(
                "unknown backend {other:?}; supported backends: auto, libusb, adbify"
            ),
        }
    }

    fn vid_pid(&self) -> Result<(Option<u16>, Option<u16>)> {
        let vid = match &self.vid {
            Some(v) => Some(crate::numbers::parse_u64(v)? as u16),
            None => None,
        };
        let pid = match &self.pid {
            Some(v) => Some(crate::numbers::parse_u64(v)? as u16),
            None => None,
        };
        Ok((vid, pid))
    }
}

/// Poll for a device until one appears or the deadline passes.
fn find_port(opts: &ConnectOpts) -> Result<RuntimePort> {
    let backend = opts.backend()?;
    let backend_name = opts.backend.to_ascii_lowercase();
    let (vid, pid) = opts.vid_pid()?;
    let deadline = Instant::now() + Duration::from_secs(opts.wait);
    let mut waited = false;
    let mut last_bridge_error = String::new();

    loop {
        #[cfg(target_os = "android")]
        if backend_name == "auto" || backend_name == "adbify" {
            match crate::android_ipc::AndroidIpcPort::connect_from_env() {
                Ok(mut port) => {
                    let vid_matches = vid.map_or(true, |expected| expected == port.vendor_id());
                    let pid_matches = pid.map_or(true, |expected| expected == port.product_id());
                    if !vid_matches || !pid_matches {
                        last_bridge_error = format!(
                            "active Adbify device is {:04X}:{:04X}, which does not match the requested VID/PID",
                            port.vendor_id(),
                            port.product_id()
                        );
                    } else {
                        port.open().context("failed to initialize Adbify USB transport")?;
                        return Ok(RuntimePort::Adbify(port));
                    }
                }
                Err(error) => last_bridge_error = error.to_string(),
            }
        }

        #[cfg(not(target_os = "android"))]
        let _ = (&backend_name, &last_bridge_error);

        #[cfg(not(target_os = "android"))]
        if let Some(port) = PortType::find_and_open(vid, pid, backend)
            .context("USB enumeration failed")?
        {
            return Ok(RuntimePort::Native(port));
        }

        #[cfg(target_os = "android")]
        if backend_name == "libusb" {
            if let Some(port) = PortType::find_and_open(vid, pid, backend)
                .context("USB enumeration failed")?
            {
                return Ok(RuntimePort::Native(port));
            }
        }

        if Instant::now() >= deadline {
            #[cfg(target_os = "android")]
            if backend_name == "auto" || backend_name == "adbify" {
                bail!(
                    "no active Adbify USB session after {}s; select the MTK device in Adbify and grant USB permission first ({})",
                    opts.wait,
                    last_bridge_error
                );
            }
            bail!(
                "no MTK device found after {}s (is it plugged in, powered off, and in BROM/preloader mode?)",
                opts.wait
            );
        }
        if !waited {
            eprintln!("waiting for a device to appear...");
            waited = true;
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// Connect and complete the BROM/preloader handshake.
///
/// The returned device is *not* yet in DA mode; call [`enter_da`] before any
/// operation that touches storage. `info` deliberately skips it so it can
/// still identify a chip that refuses DA upload.
pub fn connect(opts: &ConnectOpts) -> Result<DeviceHandle> {
    // Read explicitly supplied blobs first, before any USB work, so a missing
    // or bad --da/--auth/--preloader path is reported as such rather than
    // being masked by "no device found".
    let da = match &opts.da {
        Some(p) => Some(read_blob(p)?),
        None => None,
    };
    let auth = match &opts.auth {
        Some(p) => Some(read_blob(p)?),
        None => None,
    };
    let preloader = match &opts.preloader {
        Some(p) => Some(read_blob(p)?),
        None => None,
    };

    // --template fills in whichever of the three the user left out. An
    // explicit file always wins over the template, matching the GUI, where
    // hand-picked files survive applying a template. If all three were given
    // explicitly there is nothing to fetch, so skip the network entirely.
    let template = match (&opts.template, da.is_none() || auth.is_none() || preloader.is_none()) {
        (Some(_), false) => {
            eprintln!("note: --template ignored (all of --da/--auth/--preloader given)");
            None
        }
        (Some(c), true) => {
            Some(repo::resolve(c, opts.template_version, opts.refresh_template)?)
        }
        (None, _) => None,
    };

    let da = pick(da, template.as_ref().and_then(|t| t.get("da")))?;
    let auth = pick(auth, template.as_ref().and_then(|t| t.get("auth")))?;
    let preloader = pick(preloader, template.as_ref().and_then(|t| t.get("preloader")))?;

    let port = find_port(opts)?;

    let mut builder = DeviceBuilder::new(port);
    if let Some(d) = da {
        builder = builder.with_da_data(d);
    }
    if let Some(a) = auth {
        builder = builder.with_auth(a);
    }
    if let Some(p) = preloader {
        builder = builder.with_preloader(p);
    }

    let mut device = builder.build().context("failed to build device")?;
    device.init().context("handshake failed")?;

    Ok(DeviceHandle { device })
}

/// Prefer an explicitly supplied blob; fall back to the template's copy.
fn pick(
    explicit: Option<&'static [u8]>,
    templated: Option<&Path>,
) -> Result<Option<&'static [u8]>> {
    match (explicit, templated) {
        (Some(b), _) => Ok(Some(b)),
        (None, Some(p)) => Ok(Some(read_blob(p)?)),
        (None, None) => Ok(None),
    }
}

/// Upload the DA and enable the partition/bootstrapping commands.
pub fn enter_da(handle: &mut DeviceHandle) -> Result<()> {
    handle
        .device
        .enter_da_mode()
        .context("failed to enter DA mode (check --da/--auth/--preloader)")
}

/// Build `ConnectOpts` from raw C strings (for the native library API).
///
/// Empty or NULL strings are treated as "not provided". `wait_secs` of 0
/// means a short default wait (the same as the CLI default of 15 seconds).
pub fn parse_connect_opts(
    da: *const c_char,
    auth: *const c_char,
    preloader: *const c_char,
    wait_secs: u64,
) -> Result<ConnectOpts> {
    fn opt_path(p: *const c_char) -> Option<PathBuf> {
        if p.is_null() {
            return None;
        }
        let cstr = unsafe { std::ffi::CStr::from_ptr(p) };
        let s = cstr.to_str().ok()?;
        if s.is_empty() {
            return None;
        }
        Some(PathBuf::from(s))
    }

    Ok(ConnectOpts {
        da: opt_path(da),
        auth: opt_path(auth),
        preloader: opt_path(preloader),
        backend: "auto".to_string(),
        vid: None,
        pid: None,
        wait: if wait_secs == 0 { 15 } else { wait_secs },
        template: None,
        template_version: None,
        refresh_template: false,
    })
}

// ----------------------------------------------------------- tests
#[cfg(test)]
mod tests {
    use super::parse_connect_opts;

    #[test]
    fn empty_paths_are_none() {
        let opts = parse_connect_opts(
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
        .unwrap();
        assert!(opts.da.is_none());
        assert!(opts.auth.is_none());
        assert!(opts.preloader.is_none());
        assert_eq!(opts.wait, 15);
    }
}

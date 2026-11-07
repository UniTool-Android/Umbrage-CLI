//! Adbify USB-session IPC transport.
//!
//! The Android app owns UsbDeviceConnection and serves this protocol over a
//! private filesystem Unix-domain socket. The terminal child process connects
//! here; it never opens USBFS or asks Android for USB permission itself.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use penumbra_mtk::error::{ConnectionError, Error as PenumbraError, Result as PenumbraResult};
use penumbra_mtk::port::{ConnectionType, MtkPort};

const MAGIC: &[u8; 4] = b"UMBR";
const PROTOCOL_VERSION: u8 = 1;
const MAX_TRANSFER: usize = 1024 * 1024;
const TRANSFER_CHUNK: usize = 16 * 1024;

pub struct AndroidIpcPort {
    stream: UnixStream,
    connection_type: ConnectionType,
    port_name: String,
    timeout: Duration,
}

impl AndroidIpcPort {
    pub fn connect_from_env() -> PenumbraResult<Self> {
        let path = std::env::var("ADBIFY_USB_SOCKET").map_err(|_| {
            PenumbraError::Connection(ConnectionError::OpenFailed(
                "ADBIFY_USB_SOCKET is not set; connect an MTK device in Adbify first".into(),
            ))
        })?;
        Self::connect(Path::new(&path))
    }

    pub fn connect(path: &Path) -> PenumbraResult<Self> {
        let mut stream = UnixStream::connect(path).map_err(|e| {
            PenumbraError::Connection(ConnectionError::OpenFailed(format!(
                "cannot connect to Adbify USB bridge at {}: {e}",
                path.display()
            )))
        })?;

        let mut hello = [0u8; 9];
        stream.read_exact(&mut hello).map_err(io_error)?;
        if &hello[..4] != MAGIC || hello[8] != PROTOCOL_VERSION {
            return Err(PenumbraError::Connection(ConnectionError::OpenFailed(
                "invalid Adbify USB bridge handshake".into(),
            )));
        }

        let vid = u16::from_be_bytes([hello[4], hello[5]]);
        let pid = u16::from_be_bytes([hello[6], hello[7]]);
        let connection_type = match hello[8 - 1] {
            0 => ConnectionType::Brom,
            1 => ConnectionType::Preloader,
            2 => ConnectionType::Da,
            _ => {
                return Err(PenumbraError::Connection(ConnectionError::OpenFailed(
                    "unsupported connection type from Adbify bridge".into(),
                )));
            }
        };

        Ok(Self {
            stream,
            connection_type,
            port_name: format!("USB {vid:04X}:{pid:04X} (Adbify IPC)"),
            timeout: Duration::from_millis(1000),
        })
    }

    fn set_io_timeout(&self) -> PenumbraResult<()> {
        self.stream.set_read_timeout(Some(self.timeout)).map_err(io_error)?;
        self.stream.set_write_timeout(Some(self.timeout)).map_err(io_error)?;
        Ok(())
    }

    fn read_response_header(&mut self) -> PenumbraResult<(i32, usize)> {
        let mut header = [0u8; 8];
        self.stream.read_exact(&mut header).map_err(io_error)?;
        let status = i32::from_be_bytes(header[..4].try_into().unwrap());
        let length = u32::from_be_bytes(header[4..].try_into().unwrap()) as usize;
        if length > MAX_TRANSFER {
            return Err(PenumbraError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Adbify bridge returned an oversized USB transfer",
            )));
        }
        if status != 0 {
            return Err(PenumbraError::Io(std::io::Error::other(format!(
                "Adbify USB transfer failed (status {status})"
            ))));
        }
        Ok((status, length))
    }

    fn bulk_read_chunk(&mut self, output: &mut [u8]) -> PenumbraResult<usize> {
        self.set_io_timeout()?;
        self.stream.write_all(&[1]).map_err(io_error)?;
        self.stream
            .write_all(&(output.len() as u32).to_be_bytes())
            .map_err(io_error)?;
        self.stream
            .write_all(&(self.timeout.as_millis().min(u32::MAX as u128) as u32).to_be_bytes())
            .map_err(io_error)?;
        let (_, length) = self.read_response_header()?;
        if length > output.len() {
            return Err(PenumbraError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Adbify bridge returned more bytes than requested",
            )));
        }
        self.stream.read_exact(&mut output[..length]).map_err(io_error)?;
        Ok(length)
    }

    fn bulk_write_chunk(&mut self, input: &[u8]) -> PenumbraResult<usize> {
        self.set_io_timeout()?;
        self.stream.write_all(&[2]).map_err(io_error)?;
        self.stream
            .write_all(&(self.timeout.as_millis().min(u32::MAX as u128) as u32).to_be_bytes())
            .map_err(io_error)?;
        self.stream
            .write_all(&(input.len() as u32).to_be_bytes())
            .map_err(io_error)?;
        self.stream.write_all(input).map_err(io_error)?;
        let mut response = [0u8; 8];
        self.stream.read_exact(&mut response).map_err(io_error)?;
        let status = i32::from_be_bytes(response[..4].try_into().unwrap());
        let written = u32::from_be_bytes(response[4..].try_into().unwrap()) as usize;
        if status != 0 {
            return Err(PenumbraError::Io(std::io::Error::other(format!(
                "Adbify bulk OUT failed (status {status})"
            ))));
        }
        if written > input.len() {
            return Err(PenumbraError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Adbify bridge reported an invalid bulk OUT length",
            )));
        }
        Ok(written)
    }

    fn write_control_header(
        &mut self,
        opcode: u8,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        length: usize,
    ) -> PenumbraResult<()> {
        self.set_io_timeout()?;
        self.stream.write_all(&[opcode, request_type, request]).map_err(io_error)?;
        self.stream.write_all(&value.to_be_bytes()).map_err(io_error)?;
        self.stream.write_all(&index.to_be_bytes()).map_err(io_error)?;
        self.stream
            .write_all(&(self.timeout.as_millis().min(u32::MAX as u128) as u32).to_be_bytes())
            .map_err(io_error)?;
        self.stream
            .write_all(&(length as u32).to_be_bytes())
            .map_err(io_error)?;
        Ok(())
    }
}

impl MtkPort for AndroidIpcPort {
    fn open(&mut self) -> PenumbraResult<()> {
        Ok(())
    }

    fn close(&mut self) -> PenumbraResult<()> {
        self.stream.shutdown(std::net::Shutdown::Both).map_err(io_error)
    }

    fn reenumerate(&mut self, _vid: u16, _pid: u16) -> PenumbraResult<()> {
        Err(PenumbraError::Connection(ConnectionError::OpenFailed(
            "USB re-enumeration requires a new Adbify permission/session".into(),
        )))
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> PenumbraResult<usize> {
        let mut total = 0;
        while total < buf.len() {
            let end = (total + TRANSFER_CHUNK).min(buf.len());
            let n = self.bulk_read_chunk(&mut buf[total..end])?;
            if n == 0 {
                return Err(PenumbraError::Timeout);
            }
            total += n;
        }
        Ok(total)
    }

    fn write_all(&mut self, buf: &[u8]) -> PenumbraResult<()> {
        let mut total = 0;
        while total < buf.len() {
            let end = (total + TRANSFER_CHUNK).min(buf.len());
            let n = self.bulk_write_chunk(&buf[total..end])?;
            if n == 0 {
                return Err(PenumbraError::Timeout);
            }
            total += n;
        }
        Ok(())
    }

    fn flush(&mut self) -> PenumbraResult<()> {
        Ok(())
    }

    fn get_baudrate(&self) -> u32 {
        0
    }

    fn get_port_name(&self) -> String {
        self.port_name.clone()
    }

    fn set_timeout(&mut self, timeout: Duration) -> PenumbraResult<()> {
        self.timeout = timeout;
        Ok(())
    }

    fn get_timeout(&self) -> Duration {
        self.timeout
    }

    fn connection_type(&self) -> ConnectionType {
        self.connection_type
    }

    fn set_connection_type(&mut self, connection_type: ConnectionType) -> PenumbraResult<()> {
        self.connection_type = connection_type;
        Ok(())
    }

    fn ctrl_out(
        &mut self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> PenumbraResult<()> {
        if data.len() > MAX_TRANSFER {
            return Err(PenumbraError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "control OUT payload exceeds bridge limit",
            )));
        }
        self.write_control_header(4, request_type, request, value, index, data.len())?;
        self.stream.write_all(data).map_err(io_error)?;
        let (status, _) = self.read_response_header()?;
        debug_assert_eq!(status, 0);
        Ok(())
    }

    fn ctrl_in(
        &mut self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        len: usize,
    ) -> PenumbraResult<Vec<u8>> {
        if len > MAX_TRANSFER {
            return Err(PenumbraError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "control IN request exceeds bridge limit",
            )));
        }
        self.write_control_header(3, request_type, request, value, index, len)?;
        let (_, actual) = self.read_response_header()?;
        let mut data = vec![0u8; actual];
        self.stream.read_exact(&mut data).map_err(io_error)?;
        Ok(data)
    }
}

fn io_error(error: std::io::Error) -> PenumbraError {
    PenumbraError::Io(error)
}

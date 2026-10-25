//! Bounded, versioned relay wire protocol. Length excludes the four-byte prefix.
use std::fmt;

pub const VERSION: u16 = 1;
pub const MAX_FRAME_LEN: usize = 64 * 1024;
pub const MAX_DATAGRAM_PAYLOAD: usize = MAX_FRAME_LEN - 17;
pub const MAX_STREAM_PAYLOAD: usize = MAX_FRAME_LEN - 9;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VAddr {
    pub ip: [u8; 4],
    pub port: u16,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Proto {
    Tcp,
    Udp,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    pub kind: String,
    pub port: u16,
    pub txt: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Hello {
        version: u16,
        name: String,
    },
    Welcome {
        ip: [u8; 4],
        name: String,
    },
    Error {
        message: String,
    },
    Register {
        id: u32,
        service: Service,
    },
    Unregister {
        id: u32,
    },
    Listed {
        owner: [u8; 4],
        id: u32,
        service: Service,
    },
    Unlisted {
        owner: [u8; 4],
        id: u32,
    },
    Datagram {
        src: VAddr,
        dst: VAddr,
        payload: Vec<u8>,
    },
    StreamOpen {
        stream: u32,
        src: VAddr,
        dst: VAddr,
    },
    StreamAccept {
        stream: u32,
    },
    StreamData {
        stream: u32,
        payload: Vec<u8>,
    },
    StreamClose {
        stream: u32,
    },
    Ping,
    Pong,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtoError(pub &'static str);
impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ProtoError {}

pub fn sanitise_name(raw: &str) -> String {
    let mut name: String = raw.chars().filter(|c| !c.is_control()).collect();
    truncate_utf8(&mut name, 63);
    if name.trim().is_empty() {
        "Player".into()
    } else {
        name
    }
}
pub fn truncate_utf8(value: &mut String, max: usize) {
    if value.len() > max {
        let mut end = max;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
}

/// Append one frame. Callers must supply fields fitting the documented bounds.
/// Use `try_encode` when fields can originate outside the relay's validated input.
pub fn encode(frame: &Frame, out: &mut Vec<u8>) {
    try_encode(frame, out).expect("frame exceeds protocol bounds");
}
/// On error, `out` is unchanged.
pub fn try_encode(frame: &Frame, out: &mut Vec<u8>) -> Result<(), ProtoError> {
    let mut body = Vec::new();
    fn string(out: &mut Vec<u8>, value: &str) -> Result<(), ProtoError> {
        let len = u16::try_from(value.len()).map_err(|_| ProtoError("string too long"))?;
        out.extend(len.to_be_bytes());
        out.extend(value.as_bytes());
        Ok(())
    }
    fn blob(out: &mut Vec<u8>, value: &[u8]) -> Result<(), ProtoError> {
        if value.len() > MAX_FRAME_LEN {
            return Err(ProtoError("payload too long"));
        }
        out.extend((value.len() as u32).to_be_bytes());
        out.extend(value);
        Ok(())
    }
    fn addr(out: &mut Vec<u8>, value: &VAddr) {
        out.extend(value.ip);
        out.extend(value.port.to_be_bytes());
    }
    fn service(out: &mut Vec<u8>, s: &Service) -> Result<(), ProtoError> {
        string(out, &s.name)?;
        string(out, &s.kind)?;
        out.extend(s.port.to_be_bytes());
        blob(out, &s.txt)
    }
    match frame {
        Frame::Hello { version, name } => {
            body.push(1);
            body.extend(version.to_be_bytes());
            string(&mut body, name)?;
        }
        Frame::Welcome { ip, name } => {
            body.push(2);
            body.extend(ip);
            string(&mut body, name)?;
        }
        Frame::Error { message } => {
            body.push(3);
            string(&mut body, message)?;
        }
        Frame::Register { id, service: s } => {
            body.push(4);
            body.extend(id.to_be_bytes());
            service(&mut body, s)?;
        }
        Frame::Unregister { id } => {
            body.push(5);
            body.extend(id.to_be_bytes());
        }
        Frame::Listed {
            owner,
            id,
            service: s,
        } => {
            body.push(6);
            body.extend(owner);
            body.extend(id.to_be_bytes());
            service(&mut body, s)?;
        }
        Frame::Unlisted { owner, id } => {
            body.push(7);
            body.extend(owner);
            body.extend(id.to_be_bytes());
        }
        Frame::Datagram { src, dst, payload } => {
            body.push(8);
            addr(&mut body, src);
            addr(&mut body, dst);
            blob(&mut body, payload)?;
        }
        Frame::StreamOpen { stream, src, dst } => {
            body.push(9);
            body.extend(stream.to_be_bytes());
            addr(&mut body, src);
            addr(&mut body, dst);
        }
        Frame::StreamAccept { stream } => {
            body.push(10);
            body.extend(stream.to_be_bytes());
        }
        Frame::StreamData { stream, payload } => {
            body.push(11);
            body.extend(stream.to_be_bytes());
            blob(&mut body, payload)?;
        }
        Frame::StreamClose { stream } => {
            body.push(12);
            body.extend(stream.to_be_bytes());
        }
        Frame::Ping => body.push(13),
        Frame::Pong => body.push(14),
    }
    if body.len() > MAX_FRAME_LEN {
        return Err(ProtoError("frame too large"));
    }
    out.extend((body.len() as u32).to_be_bytes());
    out.extend(body);
    Ok(())
}

struct Cursor<'a>(&'a [u8]);
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ProtoError> {
        if n > self.0.len() {
            return Err(ProtoError("truncated field"));
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u16(&mut self) -> Result<u16, ProtoError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, ProtoError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn ip(&mut self) -> Result<[u8; 4], ProtoError> {
        Ok(self.take(4)?.try_into().unwrap())
    }
    fn string(&mut self) -> Result<String, ProtoError> {
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| ProtoError("invalid UTF-8"))
    }
    fn blob(&mut self) -> Result<Vec<u8>, ProtoError> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn addr(&mut self) -> Result<VAddr, ProtoError> {
        Ok(VAddr {
            ip: self.ip()?,
            port: self.u16()?,
        })
    }
    fn service(&mut self) -> Result<Service, ProtoError> {
        Ok(Service {
            name: self.string()?,
            kind: self.string()?,
            port: self.u16()?,
            txt: self.blob()?,
        })
    }
}
pub fn decode(buf: &[u8]) -> Result<Option<(Frame, usize)>, ProtoError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let n = u32::from_be_bytes(buf[..4].try_into().unwrap()) as usize;
    if n == 0 || n > MAX_FRAME_LEN {
        return Err(ProtoError("invalid frame length"));
    }
    if buf.len() < n + 4 {
        return Ok(None);
    }
    let mut c = Cursor(&buf[4..4 + n]);
    let frame = match c.take(1)?[0] {
        1 => Frame::Hello {
            version: c.u16()?,
            name: c.string()?,
        },
        2 => Frame::Welcome {
            ip: c.ip()?,
            name: c.string()?,
        },
        3 => Frame::Error {
            message: c.string()?,
        },
        4 => Frame::Register {
            id: c.u32()?,
            service: c.service()?,
        },
        5 => Frame::Unregister { id: c.u32()? },
        6 => Frame::Listed {
            owner: c.ip()?,
            id: c.u32()?,
            service: c.service()?,
        },
        7 => Frame::Unlisted {
            owner: c.ip()?,
            id: c.u32()?,
        },
        8 => Frame::Datagram {
            src: c.addr()?,
            dst: c.addr()?,
            payload: c.blob()?,
        },
        9 => Frame::StreamOpen {
            stream: c.u32()?,
            src: c.addr()?,
            dst: c.addr()?,
        },
        10 => Frame::StreamAccept { stream: c.u32()? },
        11 => Frame::StreamData {
            stream: c.u32()?,
            payload: c.blob()?,
        },
        12 => Frame::StreamClose { stream: c.u32()? },
        13 => Frame::Ping,
        14 => Frame::Pong,
        _ => return Err(ProtoError("unknown frame tag")),
    };
    if !c.0.is_empty() {
        return Err(ProtoError("trailing frame bytes"));
    }
    Ok(Some((frame, n + 4)))
}

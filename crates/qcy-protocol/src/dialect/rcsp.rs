//! JieLi RCSP framing, as used on QCY's service `0xA002`.
//!
//! ```text
//! +----------+-------+--------+------------+-----------------------------+------+
//! | FE DC BA | flags | opcode | len (BE16) | body (len bytes)            |  EF  |
//! +----------+-------+--------+------------+-----------------------------+------+
//! flags: bit 7 = command (1) / response (0), bit 6 = response wanted
//! body:  command  = sn, params…
//!        response = status, sn, params…
//! ```
//!
//! No checksum. Notifications may split or merge frames, so incoming data
//! goes through a [`Reassembler`].
//!
//! Only the opcodes in [`Opcode`] can be built. In particular the firmware
//! update opcodes (`0xE1`..`0xE8`), file transfer, reboot and MD5 cannot be
//! expressed at all.

use crate::ProtocolError;

const HEADER: [u8; 3] = [0xFE, 0xDC, 0xBA];
const TRAILER: u8 = 0xEF;
/// Largest body the JieLi SDK accepts.
const MAX_BODY: usize = 530;

const FLAG_COMMAND: u8 = 0x80;
const FLAG_WANTS_RESPONSE: u8 = 0x40;

/// The RCSP opcodes this crate sends or understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Opcode {
    /// `0x03`: device ("target") information, LTV attributes.
    TargetInfo,
    /// `0x07`: get system info (EQ, …).
    GetSysInfo,
    /// `0x08`: set system info.
    SetSysInfo,
    /// `0x09`: system info changed (sent by the device).
    SysInfoChanged,
    /// `0xC0`: set an earbud ("ADV") attribute.
    SetAdv,
    /// `0xC1`: get earbud attributes by bit mask.
    GetAdv,
    /// `0xC2`: earbud status notification (sent by the device).
    AdvNotify,
    /// `0xC4`: the device asks the app to do something (re-read, …).
    AdvRequest,
    /// `0xC5`: system operation (only 1 restore settings, 2 clear pairing).
    SystemOp,
    /// `0xFF`: vendor (QCY) command carrying one QCY TLV.
    Custom,
}

impl Opcode {
    pub const fn code(self) -> u8 {
        match self {
            Opcode::TargetInfo => 0x03,
            Opcode::GetSysInfo => 0x07,
            Opcode::SetSysInfo => 0x08,
            Opcode::SysInfoChanged => 0x09,
            Opcode::SetAdv => 0xC0,
            Opcode::GetAdv => 0xC1,
            Opcode::AdvNotify => 0xC2,
            Opcode::AdvRequest => 0xC4,
            Opcode::SystemOp => 0xC5,
            Opcode::Custom => 0xFF,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        [
            Opcode::TargetInfo,
            Opcode::GetSysInfo,
            Opcode::SetSysInfo,
            Opcode::SysInfoChanged,
            Opcode::SetAdv,
            Opcode::GetAdv,
            Opcode::AdvNotify,
            Opcode::AdvRequest,
            Opcode::SystemOp,
            Opcode::Custom,
        ]
        .into_iter()
        .find(|o| o.code() == code)
    }
}

/// A decoded frame from the earbuds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Raw opcode (frames with opcodes this crate does not know are kept).
    pub opcode: u8,
    /// `true` for device-originated commands, `false` for responses.
    pub is_command: bool,
    /// Response status (0 = success); 0 for commands.
    pub status: u8,
    pub sn: u8,
    pub params: Vec<u8>,
}

impl Frame {
    pub fn op(&self) -> Option<Opcode> {
        Opcode::from_code(self.opcode)
    }
}

/// Builds a command that wants a response (flags `0xC0`, like QCY's app).
pub fn command(op: Opcode, sn: u8, params: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if op == Opcode::SystemOp && !matches!(params, [1] | [2]) {
        // Only "restore settings" and "clear pairing"; other system
        // operations may reset or reboot the device.
        return Err(ProtocolError::InvalidValue(format!(
            "refusing RCSP system operation {params:02x?}"
        )));
    }
    let body = 1 + params.len();
    if body > MAX_BODY {
        return Err(ProtocolError::PayloadTooLong(body));
    }
    let mut out = Vec::with_capacity(8 + body);
    out.extend_from_slice(&HEADER);
    out.push(FLAG_COMMAND | FLAG_WANTS_RESPONSE);
    out.push(op.code());
    out.extend_from_slice(&(body as u16).to_be_bytes());
    out.push(sn);
    out.extend_from_slice(params);
    out.push(TRAILER);
    Ok(out)
}

/// Collects notification bytes and yields complete frames.
#[derive(Debug, Default)]
pub struct Reassembler {
    buf: Vec<u8>,
}

impl Reassembler {
    /// Appends received bytes and returns every complete frame.
    pub fn push(&mut self, data: &[u8]) -> Vec<Frame> {
        self.buf.extend_from_slice(data);
        let mut frames = Vec::new();
        loop {
            // Resynchronise on the header, dropping garbage before it.
            let Some(start) = self.buf.windows(3).position(|w| w == HEADER) else {
                // Keep a possible partial header at the end.
                let keep = self.buf.len().min(2);
                self.buf.drain(..self.buf.len() - keep);
                break;
            };
            self.buf.drain(..start);
            if self.buf.len() < 7 {
                break;
            }
            let len = usize::from(u16::from_be_bytes([self.buf[5], self.buf[6]]));
            if len > MAX_BODY {
                self.buf.drain(..1);
                continue;
            }
            let total = 7 + len + 1;
            if self.buf.len() < total {
                break;
            }
            if self.buf[total - 1] != TRAILER || len == 0 {
                self.buf.drain(..1);
                continue;
            }
            let flags = self.buf[3];
            let opcode = self.buf[4];
            let body = &self.buf[7..7 + len];
            let is_command = flags & FLAG_COMMAND != 0;
            let frame = if is_command {
                Frame {
                    opcode,
                    is_command,
                    status: 0,
                    sn: body[0],
                    params: body[1..].to_vec(),
                }
            } else if body.len() >= 2 {
                Frame {
                    opcode,
                    is_command,
                    status: body[0],
                    sn: body[1],
                    params: body[2..].to_vec(),
                }
            } else {
                self.buf.drain(..total);
                continue;
            };
            frames.push(frame);
            self.buf.drain(..total);
        }
        frames
    }
}

/// Splits `len type data…` records (`len` counts type + data).
pub fn ltv(mut data: &[u8]) -> Vec<(u8, &[u8])> {
    let mut out = Vec::new();
    while let [len, rest @ ..] = data {
        let len = usize::from(*len);
        if len == 0 || rest.len() < len {
            break;
        }
        out.push((rest[0], &rest[1..len]));
        data = &rest[len..];
    }
    out
}

/// Builds one `len type data…` record.
pub fn ltv_record(kind: u8, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + data.len());
    out.push((1 + data.len()) as u8);
    out.push(kind);
    out.extend_from_slice(data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_documented_frames() {
        // jieli.md §2.5
        assert_eq!(
            command(Opcode::TargetInfo, 0, &[0xFF, 0xFF, 0xFF, 0xFF, 0x00]).unwrap(),
            [
                0xFE, 0xDC, 0xBA, 0xC0, 0x03, 0x00, 0x06, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0xEF
            ]
        );
        assert_eq!(
            command(Opcode::Custom, 2, &[0xFE, 0x01, 0x2F]).unwrap(),
            [
                0xFE, 0xDC, 0xBA, 0xC0, 0xFF, 0x00, 0x04, 0x02, 0xFE, 0x01, 0x2F, 0xEF
            ]
        );
        assert_eq!(
            command(Opcode::SetAdv, 1, &ltv_record(7, &[0x65, 0x29, 0xA1, 0xB0])).unwrap(),
            [
                0xFE, 0xDC, 0xBA, 0xC0, 0xC0, 0x00, 0x07, 0x01, 0x05, 0x07, 0x65, 0x29, 0xA1, 0xB0,
                0xEF
            ]
        );
    }

    #[test]
    fn refuses_dangerous_system_operations() {
        assert!(command(Opcode::SystemOp, 0, &[3]).is_err());
        assert!(command(Opcode::SystemOp, 0, &[1]).is_ok());
    }

    #[test]
    fn reassembles_split_and_merged_frames() {
        let resp = [
            0xFE, 0xDC, 0xBA, 0x00, 0xFF, 0x00, 0x07, 0x00, 0x02, 0x2F, 0x03, 0x5A, 0x5C, 0x64,
            0xEF,
        ];
        let mut r = Reassembler::default();
        assert!(r.push(&resp[..5]).is_empty());
        let frames = r.push(&resp[5..]);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].params, [0x2F, 0x03, 0x5A, 0x5C, 0x64]);
        assert_eq!(frames[0].sn, 2);
        assert!(!frames[0].is_command);

        let mut two = vec![0x00, 0x13];
        two.extend_from_slice(&resp);
        two.extend_from_slice(&resp);
        assert_eq!(r.push(&two).len(), 2);
    }

    #[test]
    fn ltv_records() {
        let data = [2, 5, 1, 3, 8, 0xAA, 0xBB];
        assert_eq!(ltv(&data), vec![(5, &[1][..]), (8, &[0xAA, 0xBB][..])]);
        assert_eq!(ltv_record(8, &[1]), [2, 8, 1]);
    }
}

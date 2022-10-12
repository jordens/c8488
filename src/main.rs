use log::{debug, warn};
use std::str;
use thiserror::Error;
use std::fs::File;
use std::io::prelude::*;

#[derive(Error, Debug)]
enum AssemblerError {
    #[error("Invalid buffer size")]
    Buffer,
    #[error("Invalid buffer data")]
    Format,
    #[error("String conversion")]
    Utf8Error {
        #[from]
        source: std::str::Utf8Error,
    },
    #[error("Message was complete")]
    Complete,
}

#[derive(Default)]
struct Assembler {
    data: String,
    typ: u8,
    length: u8,
    current: u8,
}

impl Assembler {
    pub fn complete(&self) -> bool {
        self.typ != 0 && self.current == self.length
    }

    pub fn finish(self) -> (u8, String) {
        (self.typ, self.data)
    }

    pub fn push(&mut self, buf: &[u8]) -> Result<(), AssemblerError> {
        if buf.len() != 64 {
            return Err(AssemblerError::Buffer);
        }
        let typ = buf[0];
        let _history_length = u16::from_be_bytes(buf[1..3].try_into().unwrap());
        let _history_index = u16::from_be_bytes(buf[3..5].try_into().unwrap());
        let length = buf[5] >> 4;
        let index = buf[5] & 0xf;
        let payload = &buf[7..61][..buf[6] as usize];
        let _crc = u16::from_be_bytes(buf[61..63].try_into().unwrap());
        let end = buf[63];
        if self.typ == 0 {
            self.typ = typ;
            self.length = length;
        }
        if self.current >= self.length {
            Err(AssemblerError::Complete)
        } else if (self.typ, self.length, self.current + 1, 0xfd) != (typ, length, index, end) {
            Err(AssemblerError::Format)
        } else {
            let payload = str::from_utf8(payload)?;
            debug!("payload: {}", payload);
            self.data.push_str(payload);
            self.current += 1;
            Ok(())
        }
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("c8488=warn"))
        .init();

    let mut dev = File::open("/dev/hidraw0")?;

    let mut buf = [0u8; 64];
    let mut assembler = Assembler::default();

    loop {
        let len = dev.read(&mut buf)?;
        debug!("frame: {:X?}", &buf[..len]);
        if match assembler.push(&buf[..len]) {
            Err(AssemblerError::Complete) => true,
            Err(err) => {
                warn!("assembler error `{:?}`, resetting", err);
                true
            }
            Ok(_) => false,
        } {
            assembler = Assembler::default();
        }
        if assembler.complete() {
            let (typ, msg) = assembler.finish();
            assembler = Assembler::default();
            match typ {
                // human-readable message, SI units
                0xfe => println!("{}", msg),
                // urlencode imperial units
                0xfb => println!("{}", msg),
                // slash-separated rest-style, SI units
                0xf1 => println!("{}", msg),
                _ => warn!("unknown message type {}", typ),
            };
        }
    }
}

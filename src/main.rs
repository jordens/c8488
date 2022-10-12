use clap::Parser;
use log::{debug, warn};
use std::fs::File;
use std::io::prelude::*;
use std::str;
use thiserror::Error;

#[derive(Error, Debug)]
enum MessageError {
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
struct Message {
    data: String,
    typ: u8,
    length: u8,
    current: u8,
}

impl Message {
    pub fn complete(&self) -> bool {
        self.typ != 0 && self.current == self.length
    }

    pub fn finish(self) -> (u8, String) {
        (self.typ, self.data)
    }

    pub fn push(&mut self, buf: &[u8]) -> Result<(), MessageError> {
        if buf.len() != 64 {
            return Err(MessageError::Buffer);
        }
        let msg_type = buf[0];
        let _history_length = u16::from_be_bytes(buf[1..3].try_into().unwrap());
        let _history_index = u16::from_be_bytes(buf[3..5].try_into().unwrap());
        let msg_length = buf[5] >> 4;
        let msg_index = buf[5] & 0xf;
        let payload_length = buf[6] as usize;
        let payload = &buf[7..61][..payload_length];
        let _crc = u16::from_be_bytes(buf[61..63].try_into().unwrap());
        let end = buf[63];
        if self.typ == 0 {
            self.typ = msg_type;
            self.length = msg_length;
        }
        if self.current >= self.length {
            Err(MessageError::Complete)
        } else if (self.typ, self.length, self.current + 1, 0xfd)
            != (msg_type, msg_length, msg_index, end)
        {
            Err(MessageError::Format)
        } else {
            let payload = str::from_utf8(payload)?;
            debug!("payload: {payload}");
            self.data.push_str(payload);
            self.current += 1;
            Ok(())
        }
    }
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, default_value = "/dev/hidraw0")]
    device: String,
}

fn main() -> anyhow::Result<()> {
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("c8488=warn"))
        .init();

    let cli = Args::parse();

    let mut dev = File::open(cli.device)?;

    let mut buf = [0u8; 64];
    let mut msg = Message::default();

    loop {
        let len = dev.read(&mut buf)?;
        debug!("frame: {:X?}", &buf[..len]);
        if match msg.push(&buf[..len]) {
            Err(MessageError::Complete) => true,
            Err(err) => {
                warn!("assembler error `{err:?}`, resetting");
                true
            }
            Ok(_) => false,
        } {
            msg = Message::default();
        }
        if msg.complete() {
            let (typ, body) = msg.finish();
            msg = Message::default();
            match typ {
                // human-readable message, SI units
                0xfe => println!("{body}"),
                // urlencode imperial units
                0xfb => println!("{body}"),
                // slash-separated rest-style, SI units
                0xf1 => println!("{body}"),
                _ => warn!("unknown message type {typ}: {body}"),
            };
        }
    }
}

use chrono::{Datelike, Local, Timelike};
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
    Utf8Error(#[from] std::str::Utf8Error),
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

fn idb(msg: &str, station: &str) -> String {
    let mut s = String::new();
    s.push_str("weather,station=");
    s.push_str(station);
    s.push(' ');
    for (value, key) in msg.split(' ').zip([
        "channel",
        "_date",
        "_time",
        "indoor_temp",
        "indoor_humidity",
        "temp",     // outdoor
        "humidity", // outdoor
        "rain",     // rain mm/d
        "rate",     // rain mm/h
        "wind",     // wind mean km/h
        "gust",     // wind gusts km/h
        "dir",      // wind direction
        "wind_octant",
        "pressure",
        "pressure_local",
        "uv_index",
        "dew", // outdoor
        "outdoor_heat_index",
        "sensor1_temp",
        "sensor1_humidity",
        "sensor2_temp",
        "sensor2_humidity",
        "sensor3_temp",
        "sensor3_humidity",
        "sensor4_temp",
        "sensor4_humidity",
        "sensor5_temp",
        "sensor5_humidity",
        "sensor6_temp",
        "sensor6_humidity",
        "sensor7_temp",
        "sensor7_humidity",
    ]) {
        if key.starts_with('_') || value.chars().all(|s| "-.".contains(s)) {
            continue;
        }
        s.push_str(key);
        s.push('=');
        if key.ends_with("octant") {
            s.push('"');
        }
        s.push_str(value);
        if key.ends_with("octant") {
            s.push('"');
        }
        s.push(',');
    }
    s.remove(s.len() - 1);
    s
}

fn main() -> anyhow::Result<()> {
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("c8488=warn"))
        .init();

    let mut args = pico_args::Arguments::from_env();
    let mut dev = File::options().read(true).write(true).open(
        args.opt_value_from_str("--device")?
            .unwrap_or_else(|| "/dev/hidraw0".to_string()),
    )?;
    if args.contains("--datetime") {
        let dt = Local::now();
        let mut buf = [
            0xfc,
            0x08,
            (dt.year() - 2000) as _,
            dt.month() as _,
            dt.day() as _,
            0x00,
            0x00,
            0xfd,
        ];
        dev.write_all(&buf)?;
        buf[1..5].copy_from_slice(&[0x09, dt.hour() as _, dt.minute() as _, dt.second() as _]);
        dev.write_all(&buf)?;
    }
    let station: String = args
        .opt_value_from_str("--station")?
        .unwrap_or_else(|| "c8488".to_string());
    let socket = std::net::UdpSocket::bind(
        args.opt_value_from_str("--bind")?
            .unwrap_or_else(|| "0.0.0.0:0".to_string()),
    )?;
    let target: Option<std::net::SocketAddr> = args.opt_value_from_str("--target")?;
    let every: u32 = args.opt_value_from_str("--every")?.unwrap_or(0);

    let mut buf = [0u8; 64];
    let mut msg = Message::default();

    let mut i = 0;
    loop {
        let len = dev.read(&mut buf)?;
        debug!("frame: {:X?}", &buf[..len]);
        if match msg.push(&buf[..len]) {
            Err(MessageError::Complete) => true,
            Err(MessageError::Buffer) => Err(MessageError::Buffer)?,
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
                0xfe => {
                    if i > 0 {
                        i -= 1;
                    } else {
                        i = every;
                        let s = idb(&body, &station);
                        println!("{}", s);
                        if let Some(t) = target.as_ref() {
                            socket.send_to(s.as_bytes(), t)?;
                        }
                    }
                }
                // urlencode imperial units
                // 0xfb => println!("{body}"),
                // slash-separated rest-style, SI units
                // 0xf1 => println!("{body}"),
                _ => warn!("unknown message type {typ}: {body}"),
            };
        }
    }
}

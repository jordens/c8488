use chrono::NaiveDateTime;
use log::{debug, warn};
use std::fs::File;
use std::io::prelude::*;
use std::str::{self, FromStr};
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

#[derive(Error, Debug)]
enum ReadingsError {
    #[error("Message too short")]
    Short,
    #[error("Message data invalid")]
    Invalid,
    #[error("String conversion")]
    Parse,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
struct Sensor {
    temperature: Option<f32>,
    humidity: Option<f32>,
}
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
#[allow(dead_code)]
struct Readings {
    _unknown0: u8, // version? battery?
    datetime: chrono::NaiveDateTime,
    indoor: Sensor,
    outdoor: Sensor,
    rain_day: Option<f32>,
    rain_hour: Option<f32>,
    wind_speed: Option<f32>,
    wind_speed_gust: Option<f32>,
    wind_direction: Option<f32>,
    wind_octant: Option<String>,
    pressure_rel: Option<f32>,
    pressure_abs: Option<f32>,
    uv_index: Option<u8>,
    dewpoint: Option<f32>,
    _unknown1: Option<f32>,
    other: [Sensor; 7],
}

fn pop<'a, T: FromStr, I: IntoIterator<Item = &'a str>>(
    msg: &mut I,
) -> Result<Option<T>, ReadingsError> {
    let part = msg.into_iter().next().ok_or(ReadingsError::Short)?;
    if part.chars().all(|s| "-.".contains(s)) {
        Ok(None)
    } else {
        Ok(Some(part.parse().or(Err(ReadingsError::Parse))?))
    }
}

impl TryFrom<&str> for Readings {
    type Error = ReadingsError;

    fn try_from(msg: &str) -> Result<Self, Self::Error> {
        let mut msg = msg.split(' ');
        Ok(Self {
            _unknown0: pop(&mut msg)?.ok_or(ReadingsError::Invalid)?,
            datetime: NaiveDateTime::parse_from_str(
                &[
                    msg.next().ok_or(ReadingsError::Short)?,
                    msg.next().ok_or(ReadingsError::Short)?,
                ]
                .join(" "),
                "%Y-%m-%d %H:%M",
            )
            .or(Err(ReadingsError::Parse))?,
            indoor: Sensor {
                temperature: pop(&mut msg)?,
                humidity: pop(&mut msg)?,
            },
            outdoor: Sensor {
                temperature: pop(&mut msg)?,
                humidity: pop(&mut msg)?,
            },
            rain_day: pop(&mut msg)?,
            rain_hour: pop(&mut msg)?,
            wind_speed: pop(&mut msg)?,
            wind_speed_gust: pop(&mut msg)?,
            wind_direction: pop(&mut msg)?,
            wind_octant: msg.next().map(|s| s.to_string()),
            pressure_rel: pop(&mut msg)?,
            pressure_abs: pop(&mut msg)?,
            uv_index: pop(&mut msg)?,
            dewpoint: pop(&mut msg)?,
            _unknown1: pop(&mut msg)?,
            other: core::array::from_fn(|_| Sensor {
                temperature: pop(&mut msg)?,
                humidity: pop(&mut msg)?,
            }),
        })
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("c8488=warn"))
        .init();

    let mut args = pico_args::Arguments::from_env();
    let mut dev = File::open(
        args.opt_value_from_str("--device")?
            .unwrap_or_else(|| "/dev/hidraw0".to_string()),
    )?;

    let mut buf = [0u8; 64];
    let mut msg = Message::default();

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
                    println!("{body}");
                    println!("{:?}", Readings::try_from(body.as_str())?);
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

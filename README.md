# Minimal PC weather station to influxdb bridge

## Hardware

* Bresser 5-in-1 PC (7002571)
* CCLEL C8488 clones
* VID=1941 PID=8021

Very likely also:

* Youshiko YC9388
* Bresser PC 6 in 1
* Garni 935PC
* Ventus W835

Original protocol deciphering work from [`weewx-ws6in1`](https://github.com/BobAtchley/weewx-ws6in1/blob/a969571c2e59ff8a739f16a95ff7404f00e822d2/bin/user/ws6in1.py) with additions and simplifications.

## Mechanism

* Uses `hidraw` kernel driver to access the weather station (load that module and ensure it binds the device, also ensure RW access to the respective `hidraw` character device node)
* Does not need libusb or hidapi
* Configures time and date so that rain quantity reset time is correct
* Optionally exports reports receveived to influxdb line protocol and sends via UDP

## Compile

E.g. for openwrt on mips architectures:

* Install `cross`
* Run `cross +nightly build --release --target mips-unknown-linux-musl -Z build-std=std,panic_abort -Z build-std-features=panic_immediate_abort`
* Get a small-ish (~ 100 kB) binary

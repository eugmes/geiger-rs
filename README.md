# Geiger Counter Firmware Written in Rust

This is Rust firmware for a geiger counter is based on ATTiny2313 MCU.

Rust code author: Ievgenii Meshcheriakov <eugen@debian.org>.

This code (including most of the documentation) is based on original C code
for MightyOhm Geiger counter.

## Build

The firmware requires Rust compiler from
[nightly channel](https://rust-lang.github.io/rustup/concepts/channels.html).

```
$ rustup toolchain install nightly
$ rustup default nightly
$ cargo build --release
```

Cargo can be used to build the firmware:

```
$ cargo build --release
```

The firmware can then be flashed to the Geiger counter either by using
[avrdude](https://www.nongnu.org/avrdude/) directly, or by `cargo-avrdude`
crate:

```
$ cargo install cargo-avrdude
$ cargo avrdude --release
```

The original code description is following below.

## Geiger Counter with Serial Data Reporting

Description: This is the firmware for the mightyohm.com Geiger Counter.
There is more information at http://mightyohm.com/geiger

* Author: Jeff Keyzer
* Company: MightyOhm Engineering
* Website: http://mightyohm.com/
* Contact: jeff \<at\> mightyohm.com

This firmware controls the ATtiny2313 AVR microcontroller on board the Geiger
Counter kit.

When an impulse from the GM tube is detected, the firmware flashes the LED and
produces a short beep on the piezo speaker. It also outputs an active-high pulse
(default 100us) on the PULSE pin.

A pushbutton on the PCB can be used to mute the beep.

A running average of the detected counts per second (CPS), counts per minute
(CPM), and equivalent dose (uSv/hr) is output on the serial port once per
second. The dose is based on information collected from the web, and may not be
accurate.

The serial port is configured for BAUD baud, 8-N-1 (default 9600).

The data is reported in comma separated value (CSV) format:

```
CPS, #####, CPM, #####, uSv/hr, ###.##, SLOW|FAST|INST
```

There are three modes. Normally, the sample period is LONG_PERIOD (default 60
seconds). This is SLOW averaging mode. If the last five measured counts exceed a
preset threshold, the sample period switches to SHORT_PERIOD seconds (default 5
seconds). This is FAST mode, and is more responsive but less accurate. Finally,
if CPS > 255, we report CPS*60 and switch to INST mode, since we can't store
data in the (8-bit) sample buffer. This behavior could be customized to suit a
particular logging application.

The largest CPS value that can be displayed is 65535, but the largest value that
can be stored in the sample buffer is 255.

### WARNING

This Geiger Counter kit is for **EDUCATIONAL PURPOSES ONLY**.  Don't even think
about using it to monitor radiation in life-threatening situations, or in any
environment where you may expose yourself to dangerous levels of radiation.
Don't rely on the collected data to be an accurate measure of radiation
exposure! Be safe!

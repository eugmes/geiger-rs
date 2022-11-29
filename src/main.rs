#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]

use avr_device::interrupt::{self, CriticalSection, Mutex};
use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
};
use nano_fmt::{NanoDisplay, NanoWrite};
use panic_halt as _;

use attiny_hal as hal;
use hal::{
    pac::{EXINT, TC0},
    port::Pin,
    port::{
        mode::{Input, Output, PullUp},
        PinOps, PD3, PD6,
    },
    prelude::*,
};

mod led;
mod nano_fmt;
mod ring_buffer;
mod usart;

/// Board clock rate.
type DefaultClock = hal::clock::MHz8;

type Baudrate = usart::Baudrate<DefaultClock>;
type Delay = hal::delay::Delay<DefaultClock>;

/// UART baud rate.
const BAUDRATE: u32 = 9600;

/// Width of the PULSE output (in microseconds).
const PULSE_WIDTH: u8 = 100;

const SHORT_PERIOD: u8 = 5;
const LONG_PERIOD: usize = 60;

/// CPM threshold for fast averaging mode.
const THRESHOLD: u16 = 1000;

/// Data that is shared by multiple tasks.
struct SharedData {
    /// Number of GM events that has occurred.
    count: u16,
    /// GM counts per minute in slow mode.
    slow_cpm: u16,
    /// GM counts per minute in fast mode.
    fast_cpm: u16,
    /// GM counts per second, updated once a second.
    cps: u16,
    /// Flag used to mute beeper.
    no_beep: bool,
    /// Overflow flag.
    overflow: bool,
    /// Flag for ISR to tell main loop if a GM event has occurred.
    event_flag: bool,
    /// Flag that tells main loop when 1 second has passed.
    tick: bool,
}

impl SharedData {
    pub const fn new() -> Self {
        Self {
            count: 0,
            slow_cpm: 0,
            fast_cpm: 0,
            cps: 0,
            no_beep: false,
            overflow: false,
            event_flag: false,
            tick: false,
        }
    }
}

static SHARED_DATA: Mutex<UnsafeCell<SharedData>> = Mutex::new(UnsafeCell::new(SharedData::new()));

// TODO: Find a way to get rid of configs
static mut PULSE: MaybeUninit<Pin<Output, PD6>> = MaybeUninit::uninit();
static mut BUTTON: MaybeUninit<Pin<Input<PullUp>, PD3>> = MaybeUninit::uninit();
static mut SHARED_EXINT: MaybeUninit<EXINT> = MaybeUninit::uninit();

#[derive(Clone, Copy)]
#[repr(u8)]
enum LoggingMode {
    Slow,
    Fast,
    Instant,
}

fn delay_ms(ms: u8) {
    Delay::new().delay_ms(ms)
}

/// Pin change interrupt for pin INT0
/// This interrupt is called on the falling edge of a GM pulse.
#[avr_device::interrupt(attiny2313)]
fn INT0() {
    // SAFETY: We are inside a blocking interrupt.
    let cs = unsafe { CriticalSection::new() };

    let shared = unsafe { SHARED_DATA.borrow(cs).get().as_mut().unwrap() };
    shared.count = shared.count.saturating_add(1);

    // Tell main program loop that a GM pulse has occurred.
    shared.event_flag = true;

    // Send a pulse to the PULSE connector.
    // A delay of 100us limits the max CPS to about 8000.
    // You can comment out this code and increase the max CPS possible (up to 65535!).
    let pulse = unsafe {
        // SAFETY: PULSE is initialized in the main function and is exclusively used here.
        PULSE.assume_init_mut()
    };
    pulse.set_high();
    Delay::new().delay_us(PULSE_WIDTH);
    pulse.set_low();
}

/// Pin change interrupt for pin INT1 (pushbutton)
/// If the user pushes the button, this interrupt is executed.
/// We need to be careful about switch bounce, which will make the interrupt
/// execute multiple times if we're not careful.
#[avr_device::interrupt(attiny2313)]
fn INT1() {
    // SAFETY: We are inside a blocking interrupt.
    let cs = unsafe { CriticalSection::new() };

    delay_ms(25u8);

    // Is button still pressed?
    let button = unsafe { BUTTON.assume_init_ref() };
    if button.is_low() {
        let shared = unsafe { SHARED_DATA.borrow(cs).get().as_mut().unwrap() };
        shared.no_beep = !shared.no_beep;
    }

    // Clear interrupt flag to avoid executing ISR again due to switch bounce
    let exint = unsafe { SHARED_EXINT.assume_init_mut() };
    exint.eifr.write(|w| w.intf().bits(0b10));
}

/// TIMER1 compare interrupt.
/// This interrupt is called every time TCNT1 reaches OCR1A and is reset back to 0 (CTC mode).
/// TIMER1 is setup so this happens once a second.
#[avr_device::interrupt(attiny2313)]
fn TIMER1_COMPA() {
    static mut BUFFER: ring_buffer::RingBuffer<LONG_PERIOD> = ring_buffer::RingBuffer::new();

    // SAFETY: We are inside a blocking interrupt.
    let cs = unsafe { CriticalSection::new() };

    let shared = unsafe { SHARED_DATA.borrow(cs).get().as_mut().unwrap() };
    shared.tick = true;

    let count = mem::replace(&mut shared.count, 0);

    shared.cps = count;

    let count = count.try_into().unwrap_or_else(|_| {
        shared.overflow = true;
        u8::MAX
    });

    let oldest_count = BUFFER.put(count);

    shared.slow_cpm -= oldest_count as u16;
    shared.slow_cpm += count as u16;

    let mut fast_cpm = 0u16;
    for val in BUFFER.iter(SHORT_PERIOD) {
        fast_cpm += val as u16;
    }

    const FAST_CPM_SCALE: u16 = (LONG_PERIOD as u8 / SHORT_PERIOD) as u16;
    shared.fast_cpm = fast_cpm * FAST_CPM_SCALE;
}

/// Flash LED and beep the piezo.
fn check_event<P: PinOps>(led: &mut led::Led<Pin<Output, P>>, beeper: &mut TC0) {
    let (event_flag, no_beep) = interrupt::free(|cs| {
        let shared = unsafe { SHARED_DATA.borrow(cs).get().as_mut().unwrap() };
        let event_flag = mem::replace(&mut shared.event_flag, false);
        (event_flag, shared.no_beep)
    });

    if event_flag {
        led.turn_on();

        if !no_beep {
            // enable OCR0A output on pin PB2
            beeper.tccr0a.modify(|_, w| w.com0a().match_toggle());
            // Set prescaler to clk/8 (1Mhz) or 1us/count.
            beeper.tccr0b.modify(|_, w| w.cs0().prescale_8());
            // 160 = toggle OCR0A every 160ms, period = 320us, freq= 3.125kHz
            beeper.ocr0a.write(|w| unsafe { w.bits(160) });
        }

        // 10ms delay gives a nice short flash and 'click' on the piezo.
        delay_ms(10u8);

        led.turn_off();

        // Disable TIMER0 since we're no longer using it.
        beeper.tccr0b.reset();
        // Disconnect OCR0A from TIMER0, this avoids occasional HVPS whine after beep.
        beeper.tccr0a.modify(|_, w| w.com0a().disconnected());
    }
}

/// Log data over the serial port.
fn send_report<W>(w: &mut W)
where
    W: NanoWrite,
{
    let report = interrupt::free(|cs| {
        let shared = unsafe { SHARED_DATA.borrow(cs).get().as_mut().unwrap() };
        let tick = mem::replace(&mut shared.tick, false);

        if tick {
            let (cpm, mode) = if shared.overflow {
                shared.overflow = false;
                (shared.cps as u32 * 60, LoggingMode::Instant)
            } else if shared.fast_cpm > THRESHOLD {
                (shared.fast_cpm as u32, LoggingMode::Fast)
            } else {
                (shared.slow_cpm as u32, LoggingMode::Slow)
            };

            Some((shared.cps, cpm, mode))
        } else {
            None
        }
    });

    if let Some((cps, cpm, mode)) = report {
        // uwrite!(w, "CPS, {}, CPM, {}, {}\r\n", cps, cpm, mode).void_unwrap();
        cps.fmt(w);
        cpm.fmt(w);
        //mode.fmt(w);
        //"\r\n".fmt(w);
    }
}

#[hal::entry]
fn main() -> ! {
    let dp = hal::Peripherals::take().unwrap();
    let pins = hal::pins!(dp);

    let mut serial = usart::Usart0::new(
        dp.USART,
        pins.pd0.into_pull_up_input(),
        pins.pd1.into_output(),
        Baudrate::new(BAUDRATE),
    );

    // uwrite!(serial, "mightyohm.com Geiger Counter\r\n").unwrap();

    // Set pins connected to LED and piezo as outputs.
    let mut led = led::Led::new(pins.pb4.into_output());
    let mut _piezo = pins.pb2.into_output();

    // Configure PULSE output.
    let pulse = pins.pd6.into_output();

    // Enable internal pull up resistor on pin connected to button.
    let button = pins.pd3.into_pull_up_input();

    // Set up external interrupts.
    // INT0 is triggered by a GM impulse.
    // INT1 is triggered by pushing the button.

    // Config interrupts on falling edge of INT0 and INT1.
    dp.CPU
        .mcucr
        .modify(|_, w| w.isc0().falling().isc1().val_0x01());

    // Enable external interrupts on pins INT0 and INT1.
    dp.EXINT.gimsk.modify(|_, w| w.int().bits(0b11));

    // Configure the Timers.
    // Set up TIMER0 for tone generation.
    // Toggle OC0A (pin PB2) on compare match and set timer to CTC mode.
    dp.TC0
        .tccr0a
        .write(|w| w.com0a().match_toggle().wgm0().ctc());
    // Stop TIMER0 (no sound).
    dp.TC0.tccr0b.reset();

    // Set up TIMER1 for 1 second interrupts.
    // CTC mode, prescaler = 256 (32us ticks).
    dp.TC1
        .tccr1b
        .write(|w| w.wgm1().bits(0b01).cs1().prescale_256());
    // 32us * 31250 = 1 sec
    dp.TC1.ocr1a.write(|w| unsafe { w.bits(31250) });
    // TIMER1 overflow interrupt enable.
    dp.TC1.timsk.write(|w| w.ocie1a().set_bit());

    let exint = dp.EXINT;
    let mut beeper = dp.TC0;

    unsafe {
        // SAFETY: Shared peripherals are initialized exclusively in this function
        PULSE.write(pulse);
        BUTTON.write(button);
        SHARED_EXINT.write(exint);
    }

    // Enable interrupts.
    unsafe {
        // SAFETY: Not inside a critical section and any non-atomic operations have been completed
        // at this point.
        avr_device::interrupt::enable();
    }

    loop {
        // Set sleep mode to IDLE and enable sleep.
        dp.CPU.mcucr.modify(|_, w| w.sm().idle().se().set_bit());
        // Go to sleep until next interrupt.
        avr_device::asm::sleep();
        // Disable sleep so we don't accidentally go to  sleep.
        dp.CPU.mcucr.modify(|_, w| w.se().clear_bit());

        check_event(&mut led, &mut beeper);
        send_report(&mut serial);
        check_event(&mut led, &mut beeper);
    }
}

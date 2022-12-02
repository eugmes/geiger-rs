#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]
#![feature(asm_experimental_arch)]

use avr_device::interrupt::{self, CriticalSection, Mutex};
use core::{
    arch::asm,
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
};
use geiger::{fixed::Fixed2, led::Led, ring_buffer::RingBuffer, usart::Usart0};
use nano_fmt::NanoWrite;
use panic_halt as _;
use progmem::{write, P};

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

/// Board clock rate.
type DefaultClock = hal::clock::MHz8;

type Baudrate = geiger::usart::Baudrate<DefaultClock>;
type Delay = hal::delay::Delay<DefaultClock>;

/// UART baud rate.
const BAUDRATE: u32 = 9600;

/// Width of the PULSE output (in microseconds).
const PULSE_WIDTH: u8 = 100;

const SHORT_PERIOD: u8 = 5;
const LONG_PERIOD: usize = 60;

/// CPM threshold for fast averaging mode.
const THRESHOLD: u16 = 1000;

// CPM to uSv/hr conversion factor (x10,000 to avoid float).
const SCALE_FACTOR: u32 = 57u32;

/// Flags for events that can wakeup the main loop.
struct EventFlags(u8);

impl EventFlags {
    /// Create a new instance of flags with all flags reset.
    pub const fn new() -> Self {
        Self(0)
    }

    /// Flag for ISR to tell main loop if a GM event has occurred.
    const GM_EVENT: u8 = 0x01;
    /// Flag that tells main loop when 1 second has passed.
    const TICK_EVENT: u8 = 0x02;

    /// Indicate that a GM event has occured.
    pub fn set_gm_event(&mut self) {
        self.0 |= Self::GM_EVENT;
    }

    /// Indicate that a tick event has occured.
    pub fn set_tick_event(&mut self) {
        self.0 |= Self::TICK_EVENT;
    }

    /// Returns `true` if any of the events has occured.
    pub fn has_any_event(&self) -> bool {
        self.0 != 0
    }

    /// Returns and resets GM event status.
    pub fn take_gm_event(&mut self) -> bool {
        let e = self.0 & Self::GM_EVENT != 0;
        self.0 &= !Self::GM_EVENT;
        e
    }

    /// Returns and resets tick event status.
    pub fn take_tick_event(&mut self) -> bool {
        let e = self.0 & Self::TICK_EVENT != 0;
        self.0 &= !Self::TICK_EVENT;
        e
    }
}

/// Data that is shared by multiple tasks.
struct SharedData {
    /// Number of GM events that has occurred.
    count: u16,
    /// GM counts per second, updated once a second.
    cps: u16,
    /// Flag used to mute beeper.
    no_beep: bool,
    /// Flags for tick and GM events.
    event_flags: EventFlags,
}

impl SharedData {
    pub const fn new() -> Self {
        Self {
            count: 0,
            cps: 0,
            no_beep: false,
            event_flags: EventFlags::new(),
        }
    }
}

static SHARED_DATA: Mutex<UnsafeCell<SharedData>> = Mutex::new(UnsafeCell::new(SharedData::new()));

struct Smoother {
    buffer: RingBuffer<LONG_PERIOD>,
    /// GM counts per minute in slow mode.
    slow_cpm: u16,
}

impl Smoother {
    pub const fn new() -> Self {
        Self {
            buffer: RingBuffer::new(),
            slow_cpm: 0,
        }
    }
}

static mut SMOOTHER: Smoother = Smoother::new();

// TODO: Find a way to get rid of configs
static mut PULSE: MaybeUninit<Pin<Output, PD6>> = MaybeUninit::uninit();
static mut BUTTON: MaybeUninit<Pin<Input<PullUp>, PD3>> = MaybeUninit::uninit();
static mut SHARED_EXINT: MaybeUninit<EXINT> = MaybeUninit::uninit();

/// Pin change interrupt for pin INT0
/// This interrupt is called on the falling edge of a GM pulse.
#[avr_device::interrupt(attiny2313)]
fn INT0() {
    // SAFETY: We are inside a blocking interrupt.
    let cs = unsafe { CriticalSection::new() };

    let shared = unsafe { SHARED_DATA.borrow(cs).get().as_mut().unwrap() };
    shared.count = shared.count.saturating_add(1);

    // Tell main program loop that a GM pulse has occurred.
    shared.event_flags.set_gm_event();

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

    Delay::new().delay_ms(25u8);

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
    // SAFETY: We are inside a blocking interrupt.
    let cs = unsafe { CriticalSection::new() };

    let shared = unsafe { SHARED_DATA.borrow(cs).get().as_mut().unwrap() };
    shared.event_flags.set_tick_event();
    shared.cps = mem::replace(&mut shared.count, 0);
}

/// Flash LED and beep the piezo.
fn check_event<P: PinOps>(led: &mut Led<Pin<Output, P>>, beeper: &mut TC0) {
    let (event_flag, no_beep) = interrupt::free(|cs| {
        let shared = unsafe { SHARED_DATA.borrow(cs).get().as_mut().unwrap() };
        let event_flag = shared.event_flags.take_gm_event();
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
        Delay::new().delay_ms(10u8);

        led.turn_off();

        // Disable TIMER0 since we're no longer using it.
        beeper.tccr0b.reset();
        // Disconnect OCR0A from TIMER0, this avoids occasional HVPS whine after beep.
        beeper.tccr0a.modify(|_, w| w.com0a().disconnected());
    }
}

/// Log data over the serial port.
fn send_report<W>(w: &mut W, smoother: &mut Smoother)
where
    W: NanoWrite,
{
    let report = interrupt::free(|cs| {
        let shared = unsafe { SHARED_DATA.borrow(cs).get().as_mut().unwrap() };

        if shared.event_flags.take_tick_event() {
            Some(shared.cps)
        } else {
            None
        }
    });

    if let Some(cps) = report {
        write!(w, "CPS, {}, CPM, ", cps);

        let count = cps.try_into().unwrap_or(u8::MAX);
        let oldest_count = smoother.buffer.put(count);

        smoother.slow_cpm -= u16::from(oldest_count);
        smoother.slow_cpm += u16::from(count);

        let (cpm, mode_str) = {
            if cps > u16::from(u8::MAX) {
                (u32::from(cps) * 60, P!("INST"))
            } else if smoother.slow_cpm <= THRESHOLD {
                // Report cpm based on last 60 samples.
                (u32::from(smoother.slow_cpm), P!("SLOW"))
            } else {
                // Report cpm based on last 5 samples.
                let mut fast_cpm = 0u16;
                for val in smoother.buffer.iter(SHORT_PERIOD) {
                    fast_cpm += u16::from(val);
                }
                const FAST_CPM_SCALE: u16 = (LONG_PERIOD as u8 / SHORT_PERIOD) as u16;
                fast_cpm *= FAST_CPM_SCALE;
                (u32::from(fast_cpm), P!("FAST"))
            }
        };

        write!(w, "{}, uSv/hr, ", cpm);

        let usv_scaled = cpm * SCALE_FACTOR / 100;
        let usv = Fixed2::from_bits(usv_scaled);

        write!(w, "{}, {}\r\n", usv, mode_str);
    }
}

/// Wait for an event to occur.
/// Interrupts are enabled when this function returns.
fn wait_for_event() {
    let cs = unsafe {
        avr_device::interrupt::disable();
        CriticalSection::new()
    };

    let shared = unsafe { SHARED_DATA.borrow(cs).get().as_mut().unwrap() };

    if !shared.event_flags.has_any_event() {
        // Go to sleep until next interrupt.
        // This has to be inline assembly so that the compiler does not insert
        // additional instructions in between.
        unsafe {
            asm! {
                "sei",
                "sleep",
            }
        }
    }

    unsafe {
        avr_device::interrupt::enable();
    }
}

// Normally one would use `#[hal::entry]` here, but it generates an additional
// trampoline function. Doing everything explicitly saves a few bytes of RAM and
// flash.
#[export_name = "main"]
pub extern "C" fn main() -> ! {
    let dp = unsafe {
        // SAFETY: This is the only place where we get the peripherals.
        hal::Peripherals::steal()
    };
    let pins = hal::pins!(dp);

    let mut serial = Usart0::new(
        dp.USART,
        pins.pd0.into_pull_up_input(),
        pins.pd1.into_output(),
        Baudrate::new(BAUDRATE),
    );

    write!(
        &mut serial,
        "mightyohm.com Geiger Counter 1.00\r\nhttp://mightyohm.com/geiger\r\n"
    );

    // Set pins connected to LED and piezo as outputs.
    let mut led = Led::new(pins.pb4.into_output());
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

    // Set sleep mode to IDLE and enable sleep.
    dp.CPU.mcucr.modify(|_, w| w.sm().idle().se().set_bit());

    loop {
        wait_for_event();

        check_event(&mut led, &mut beeper);
        send_report(&mut serial, unsafe { &mut SMOOTHER });
    }
}

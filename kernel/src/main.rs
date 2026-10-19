//! lemOS-Kernel
//!
//! Limine lädt diesen Kernel, schaltet in den 64-Bit-Modus und springt dann
//! nach [`kmain`]. Ab hier gibt es kein Betriebssystem mehr unter uns: keine
//! Standardbibliothek, kein `main`, kein `println!` – alles bauen wir selbst.

#![no_std]
#![no_main]

mod console;
mod serial;

use core::fmt::{self, Write};
use core::panic::PanicInfo;

use limine::request::FramebufferRequest;
use limine::{BaseRevision, RequestsEndMarker, RequestsStartMarker};
use spin::Mutex;

use console::Console;
use serial::Serial;

// --- Anfragen an den Bootloader ---------------------------------------------
// Limine sucht diese Strukturen im Kernel-Image und füllt die Antworten aus,
// bevor `kmain` aufgerufen wird.

#[used]
#[unsafe(link_section = ".limine_requests_start")]
static REQUESTS_START: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static FRAMEBUFFER: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests_end")]
static REQUESTS_END: RequestsEndMarker = RequestsEndMarker::new();

// --- Ausgabe ------------------------------------------------------------------

static CONSOLE: Mutex<Option<Console>> = Mutex::new(None);

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let _ = Serial.write_fmt(args);
    if let Some(console) = CONSOLE.lock().as_mut() {
        let _ = console.write_fmt(args);
    }
}

/// Schreibt auf den Bildschirm und die serielle Schnittstelle.
#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => ($crate::_print(format_args!($($arg)*)));
}

/// Wie [`kprint!`], aber mit Zeilenumbruch.
#[macro_export]
macro_rules! kprintln {
    () => ($crate::kprint!("\n"));
    ($($arg:tt)*) => ($crate::kprint!("{}\n", format_args!($($arg)*)));
}

// --- Einstiegspunkt -----------------------------------------------------------

#[unsafe(no_mangle)]
extern "C" fn kmain() -> ! {
    Serial::init();

    if !BASE_REVISION.is_supported() {
        kprintln!("Fehler: Diese Limine-Version unterstuetzt unser Boot-Protokoll nicht.");
        halt();
    }

    if let Some(fb) = FRAMEBUFFER
        .response()
        .and_then(|r| r.framebuffers().first())
    {
        *CONSOLE.lock() = Console::new(fb);
    }

    kprintln!("Willkommen bei lemOS {}!", env!("CARGO_PKG_VERSION"));
    kprintln!("Vom Urknall zur Zitrone: der Kernel laeuft.");
    // Die CI sucht nach dieser Zeile, um zu pruefen, dass das System bootet.
    kprintln!("lemOS: boot ok");

    halt();
}

/// Hält die CPU an, bis ein Interrupt kommt – für immer.
fn halt() -> ! {
    loop {
        unsafe { core::arch::asm!("cli; hlt") };
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!("\nKERNEL PANIC: {info}");
    halt();
}

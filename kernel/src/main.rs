//! lemOS-Kernel
//!
//! Limine lädt diesen Kernel, schaltet in den 64-Bit-Modus und springt dann
//! nach [`kmain`]. Ab hier gibt es kein Betriebssystem mehr unter uns: keine
//! Standardbibliothek, kein `main`, kein `println!` – alles bauen wir selbst.

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod allocator;
mod console;
mod gdt;
mod interrupts;
mod logo;
mod memory;
mod serial;
mod shell;

use core::fmt::{self, Write};
use core::panic::PanicInfo;

use limine::request::{FramebufferRequest, HhdmRequest, MemmapRequest};
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
#[unsafe(link_section = ".limine_requests")]
static HHDM: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static MEMMAP: MemmapRequest = MemmapRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests_end")]
static REQUESTS_END: RequestsEndMarker = RequestsEndMarker::new();

// --- Ausgabe ------------------------------------------------------------------

static CONSOLE: Mutex<Option<Console>> = Mutex::new(None);

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    // Ohne Interrupts, damit uns kein Interrupt-Handler unterbricht, der
    // selbst etwas ausgeben will und dann ewig auf den Lock warten würde.
    x86_64::instructions::interrupts::without_interrupts(|| {
        let _ = Serial.write_fmt(args);
        if let Some(console) = CONSOLE.lock().as_mut() {
            let _ = console.write_fmt(args);
        }
    });
}

/// Setzt die Schriftfarbe auf dem Bildschirm (0xRRGGBB).
pub fn set_color(color: u32) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(console) = CONSOLE.lock().as_mut() {
            console.set_color(color);
        }
    });
}

/// Löscht den Bildschirm.
pub fn clear_screen() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(console) = CONSOLE.lock().as_mut() {
            console.clear();
        }
    });
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

    logo::show();
    kprintln!("Willkommen bei lemOS {}!", env!("CARGO_PKG_VERSION"));
    kprintln!("Vom Urknall zur Zitrone: der Kernel laeuft.");

    let (Some(hhdm), Some(memmap)) = (HHDM.response(), MEMMAP.response()) else {
        kprintln!("Fehler: Limine hat keine Speicherinformationen geliefert.");
        halt();
    };
    memory::init(hhdm.offset, memmap.entries());
    allocator::init();
    kprintln!("Heap mit {} KiB bereit.", allocator::HEAP_SIZE / 1024);

    gdt::init();
    interrupts::init();
    kprintln!("GDT und IDT geladen.");

    // Testet die Ausnahmebehandlung: der Handler meldet sich und kehrt zurück.
    x86_64::instructions::interrupts::int3();

    x86_64::instructions::interrupts::enable();
    // Eine Zehntelsekunde warten, um zu sehen, dass der Timer tickt.
    while interrupts::ticks() < u64::from(interrupts::TIMER_HZ / 10) {
        x86_64::instructions::hlt();
    }
    kprintln!("Timer laeuft mit {} Hz.", interrupts::TIMER_HZ);

    // Die CI sucht nach dieser Zeile, um zu pruefen, dass das System bootet.
    kprintln!("lemOS: boot ok");

    shell::run()
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

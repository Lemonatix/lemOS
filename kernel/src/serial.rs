//! Serielle Schnittstelle (COM1, 16550-UART).
//!
//! Die Ausgabe landet in QEMU direkt im Terminal (`-serial stdio`) und
//! funktioniert auch dann, wenn es keinen Bildschirm gibt. Deshalb ist sie
//! der verlässlichste Weg, um Meldungen und Fehler aus dem Kernel zu sehen.

use core::arch::asm;
use core::fmt;

const COM1: u16 = 0x3F8;

unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags))
    };
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags))
    };
    value
}

pub struct Serial;

impl Serial {
    /// Stellt COM1 auf 38400 Baud, 8 Datenbits, keine Parität, 1 Stoppbit.
    pub fn init() {
        unsafe {
            outb(COM1 + 1, 0x00); // Interrupts aus
            outb(COM1 + 3, 0x80); // DLAB an, um den Teiler zu setzen
            outb(COM1, 0x03); // Teiler 3 -> 38400 Baud
            outb(COM1 + 1, 0x00);
            outb(COM1 + 3, 0x03); // 8N1, DLAB aus
            outb(COM1 + 2, 0xC7); // FIFO an
            outb(COM1 + 4, 0x0B); // DTR, RTS, OUT2
        }
    }

    fn write_byte(byte: u8) {
        unsafe {
            // Warten, bis der Sendepuffer frei ist.
            while inb(COM1 + 5) & 0x20 == 0 {}
            outb(COM1, byte);
        }
    }
}

impl fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' {
                Self::write_byte(b'\r');
            }
            Self::write_byte(byte);
        }
        Ok(())
    }
}

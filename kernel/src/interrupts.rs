//! Interrupts: CPU-Ausnahmen, Timer und Tastatur.
//!
//! Die Interrupt Descriptor Table (IDT) sagt der CPU, welche Funktion sie bei
//! welchem Interrupt aufrufen soll. Die Nummern 0–31 sind für Ausnahmen der
//! CPU reserviert (z. B. Page Fault). Ab 32 liegen die Hardware-Interrupts,
//! die der PIC (Programmable Interrupt Controller) an die CPU weiterleitet.

use core::sync::atomic::{AtomicU64, Ordering};

use pc_keyboard::{DecodedKey, HandleControl, PS2Keyboard, ScancodeSet1, layouts};
use pic8259::ChainedPics;
use spin::{LazyLock, Mutex};
use x86_64::instructions::port::Port;
use x86_64::registers::control::Cr2;
use x86_64::registers::model_specific::Msr;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::{gdt, kprintln, memory};

/// Die beiden PICs werden hinter die 32 CPU-Ausnahmen verschoben.
const PIC_1_OFFSET: u8 = 32;
const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Clone, Copy)]
#[repr(u8)]
enum Irq {
    Timer = PIC_1_OFFSET,
    Keyboard,
}

/// So oft pro Sekunde meldet sich der Timer.
pub const TIMER_HZ: u32 = 100;

/// Grundfrequenz des PIT (Programmable Interval Timer) in Hertz.
const PIT_FREQUENCY: u32 = 1_193_182;

/// Vektor für "falsche" Interrupts des Local APIC, die nichts bedeuten.
const SPURIOUS_VECTOR: u8 = 0xFF;

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Anzahl der Timer-Ticks seit dem Start.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

static IDT: LazyLock<InterruptDescriptorTable> = LazyLock::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.divide_error.set_handler_fn(divide_error);
    idt.breakpoint.set_handler_fn(breakpoint);
    idt.invalid_opcode.set_handler_fn(invalid_opcode);
    idt.general_protection_fault
        .set_handler_fn(general_protection_fault);
    idt.page_fault.set_handler_fn(page_fault);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault)
            .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
    }
    idt[Irq::Timer as u8].set_handler_fn(timer);
    idt[Irq::Keyboard as u8].set_handler_fn(keyboard);
    idt[SPURIOUS_VECTOR].set_handler_fn(spurious);
    idt
});

/// Richtet IDT, PIC und Timer ein. Danach müssen Interrupts noch mit
/// `x86_64::instructions::interrupts::enable()` eingeschaltet werden.
pub fn init() {
    IDT.load();
    enable_virtual_wire();
    unsafe {
        let mut pics = PICS.lock();
        pics.initialize();
        // Nur Timer (IRQ 0), Tastatur (IRQ 1) und die Verbindung zum
        // zweiten PIC (IRQ 2) durchlassen, alles andere bleibt stumm.
        pics.write_masks(0b1111_1000, 0b1111_1111);
    }
    init_pit();
}

/// Lässt die Interrupts des PIC durch den Local APIC der CPU.
///
/// Moderne CPUs haben einen Local APIC, der zwischen PIC und CPU sitzt.
/// Limine lässt ihn ausgeschaltet zurück, und dann sperrt er den Eingang
/// (LINT0), über den der PIC seine Interrupts schickt. Wir schalten ihn an
/// und reichen LINT0 einfach durch ("Virtual Wire Mode"). Später ersetzen wir
/// den PIC ganz durch den APIC.
fn enable_virtual_wire() {
    const IA32_APIC_BASE: u32 = 0x1B;
    const SPURIOUS: usize = 0xF0;
    const LVT_LINT0: usize = 0x350;
    const LVT_LINT1: usize = 0x360;
    const APIC_SOFTWARE_ENABLE: u32 = 1 << 8;
    const DELIVERY_EXTINT: u32 = 0b111 << 8;
    const DELIVERY_NMI: u32 = 0b100 << 8;

    let physical = unsafe { Msr::new(IA32_APIC_BASE).read() } & 0x000F_FFFF_FFFF_F000;
    let base = memory::map_mmio(physical).cast::<u32>();
    let write = |register: usize, value: u32| unsafe {
        base.byte_add(register).write_volatile(value);
    };
    write(SPURIOUS, APIC_SOFTWARE_ENABLE | u32::from(SPURIOUS_VECTOR));
    write(LVT_LINT0, DELIVERY_EXTINT);
    write(LVT_LINT1, DELIVERY_NMI);
}

/// Stellt den PIT so ein, dass er `TIMER_HZ`-mal pro Sekunde auslöst.
fn init_pit() {
    let divisor = (PIT_FREQUENCY / TIMER_HZ) as u16;
    unsafe {
        // Kanal 0, erst Low- dann High-Byte, Modus 3 (Rechteckwelle).
        Port::<u8>::new(0x43).write(0x36);
        let mut data = Port::<u8>::new(0x40);
        data.write(divisor as u8);
        data.write((divisor >> 8) as u8);
    }
}

fn end_of_interrupt(irq: Irq) {
    unsafe { PICS.lock().notify_end_of_interrupt(irq as u8) };
}

// --- CPU-Ausnahmen --------------------------------------------------------------

extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
    // Ein Breakpoint ist harmlos: melden und einfach weitermachen.
    kprintln!(
        "Ausnahme: Breakpoint bei {:#x}",
        frame.instruction_pointer.as_u64()
    );
}

extern "x86-interrupt" fn divide_error(frame: InterruptStackFrame) {
    panic!("Ausnahme: Division durch Null\n{frame:#?}");
}

extern "x86-interrupt" fn invalid_opcode(frame: InterruptStackFrame) {
    panic!("Ausnahme: Ungueltiger Befehl\n{frame:#?}");
}

extern "x86-interrupt" fn general_protection_fault(frame: InterruptStackFrame, code: u64) {
    panic!("Ausnahme: General Protection Fault (Code {code:#x})\n{frame:#?}");
}

extern "x86-interrupt" fn page_fault(frame: InterruptStackFrame, code: PageFaultErrorCode) {
    // CR2 enthält die Adresse, auf die zugegriffen werden sollte.
    let address = Cr2::read_raw();
    panic!("Ausnahme: Page Fault bei Adresse {address:#x} ({code:?})\n{frame:#?}");
}

extern "x86-interrupt" fn double_fault(frame: InterruptStackFrame, _code: u64) -> ! {
    panic!("Ausnahme: Double Fault\n{frame:#?}");
}

// --- Hardware-Interrupts --------------------------------------------------------

extern "x86-interrupt" fn timer(_frame: InterruptStackFrame) {
    TICKS.fetch_add(1, Ordering::Relaxed);
    end_of_interrupt(Irq::Timer);
}

extern "x86-interrupt" fn spurious(_frame: InterruptStackFrame) {
    // Hier darf kein End-of-Interrupt gesendet werden.
}

static KEYBOARD: Mutex<PS2Keyboard<layouts::De105Key, ScancodeSet1>> =
    Mutex::new(PS2Keyboard::new(
        ScancodeSet1::new(),
        layouts::De105Key,
        HandleControl::Ignore,
    ));

/// Tastendrücke, die noch niemand abgeholt hat. Ein Ringpuffer mit fester
/// Größe, denn im Interrupt-Handler dürfen wir keinen Heap benutzen.
struct KeyBuffer {
    keys: [char; 64],
    start: usize,
    len: usize,
}

impl KeyBuffer {
    fn push(&mut self, c: char) {
        // Ist der Puffer voll, geht der Tastendruck verloren.
        if self.len < self.keys.len() {
            self.keys[(self.start + self.len) % self.keys.len()] = c;
            self.len += 1;
        }
    }

    fn pop(&mut self) -> Option<char> {
        if self.len == 0 {
            return None;
        }
        let c = self.keys[self.start];
        self.start = (self.start + 1) % self.keys.len();
        self.len -= 1;
        Some(c)
    }
}

static KEYS: Mutex<KeyBuffer> = Mutex::new(KeyBuffer {
    keys: ['\0'; 64],
    start: 0,
    len: 0,
});

/// Wartet auf den nächsten Tastendruck und gibt ihn zurück.
pub fn read_key() -> char {
    use x86_64::instructions::interrupts;
    loop {
        // Interrupts kurz aus, damit zwischen Nachsehen und Schlafen keine
        // Taste durchrutscht. `enable_and_hlt` schaltet sie in einem Schritt
        // wieder ein und schläft bis zum nächsten Interrupt.
        interrupts::disable();
        if let Some(c) = KEYS.lock().pop() {
            interrupts::enable();
            return c;
        }
        interrupts::enable_and_hlt();
    }
}

extern "x86-interrupt" fn keyboard(_frame: InterruptStackFrame) {
    // Der Scancode muss gelesen werden, sonst schickt der Controller nichts mehr.
    let scancode: u8 = unsafe { Port::new(0x60).read() };
    let mut keyboard = KEYBOARD.lock();
    if let Ok(Some(event)) = keyboard.add_byte(scancode)
        && let Some(DecodedKey::Unicode(c)) = keyboard.process_keyevent(event)
    {
        KEYS.lock().push(c);
    }
    end_of_interrupt(Irq::Keyboard);
}

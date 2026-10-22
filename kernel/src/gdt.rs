//! Global Descriptor Table (GDT) und Task State Segment (TSS).
//!
//! Im 64-Bit-Modus spielen Segmente kaum noch eine Rolle, aber die CPU
//! braucht trotzdem eine GDT. Wichtig ist vor allem das TSS: Es nennt der
//! CPU einen eigenen Stack für den Double Fault. Läuft der normale Stack
//! über, kann der Fehler so trotzdem noch gemeldet werden.

use spin::LazyLock;
use x86_64::VirtAddr;
use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

/// Index des Stacks im TSS, den der Double-Fault-Handler benutzt.
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

const STACK_SIZE: usize = 5 * 4096;

#[repr(align(16))]
// Wird nur über seine Adresse benutzt, nie direkt gelesen.
#[allow(dead_code)]
struct Stack([u8; STACK_SIZE]);

static mut DOUBLE_FAULT_STACK: Stack = Stack([0; STACK_SIZE]);

static TSS: LazyLock<TaskStateSegment> = LazyLock::new(|| {
    let mut tss = TaskStateSegment::new();
    // Stacks wachsen nach unten, also tragen wir das obere Ende ein.
    let start = VirtAddr::from_ptr(&raw const DOUBLE_FAULT_STACK);
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = start + STACK_SIZE as u64;
    tss
});

struct Selectors {
    code: SegmentSelector,
    data: SegmentSelector,
    tss: SegmentSelector,
}

static GDT: LazyLock<(GlobalDescriptorTable, Selectors)> = LazyLock::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let code = gdt.append(Descriptor::kernel_code_segment());
    let data = gdt.append(Descriptor::kernel_data_segment());
    let tss = gdt.append(Descriptor::tss_segment(&TSS));
    (gdt, Selectors { code, data, tss })
});

/// Lädt unsere GDT und ersetzt damit die von Limine.
pub fn init() {
    let (gdt, selectors) = &*GDT;
    gdt.load();
    unsafe {
        CS::set_reg(selectors.code);
        DS::set_reg(selectors.data);
        ES::set_reg(selectors.data);
        SS::set_reg(selectors.data);
        load_tss(selectors.tss);
    }
}

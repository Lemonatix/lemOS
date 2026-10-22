//! Der Heap: dynamischer Speicher für `Box`, `Vec`, `String` und Co.
//!
//! Wir reservieren einen festen Bereich im (sonst leeren) unteren Adressraum,
//! blenden dort freie Speicherseiten ein und übergeben ihn einem fertigen
//! Allocator. Der merkt sich die freien Stücke in einer verketteten Liste, so
//! dass freigegebener Speicher wiederverwendet werden kann.

use linked_list_allocator::LockedHeap;
use x86_64::VirtAddr;

use crate::memory;

/// Startadresse des Heaps. Gut wiederzuerkennen, falls mal etwas abstürzt.
pub const HEAP_START: u64 = 0x_4444_4444_0000;
/// Größe des Heaps: 1 MiB reicht für die Shell locker.
pub const HEAP_SIZE: usize = 1024 * 1024;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Richtet den Heap ein. Danach funktionieren `alloc::vec::Vec` & Co.
///
/// Wichtig: Interrupt-Handler dürfen keinen Speicher anfordern, sonst warten
/// sie womöglich ewig auf den Lock, den der unterbrochene Code gerade hält.
pub fn init() {
    memory::map_memory(VirtAddr::new(HEAP_START), HEAP_SIZE as u64);
    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }
}

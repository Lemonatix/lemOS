//! Physischer Speicher und Seitentabellen.
//!
//! Limine hat die Seitentabellen schon eingerichtet: Der Kernel liegt ganz
//! oben im Adressraum, und über die Higher-Half Direct Map (HHDM) ist der
//! physische Arbeitsspeicher ab `hhdm_offset` eingeblendet. Geräte-Register
//! wie die des Local APIC sind dort aber nicht dabei, die blenden wir mit
//! [`map_mmio`] selbst ein. Für den Heap holt [`map_memory`] freie Seiten aus
//! der Speicherkarte und blendet sie an einer beliebigen Adresse ein.

use limine::memmap::{Entry, MEMMAP_USABLE};
use spin::Mutex;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

struct Memory {
    hhdm_offset: u64,
    page_table: OffsetPageTable<'static>,
    frames: BootFrameAllocator,
}

static MEMORY: Mutex<Option<Memory>> = Mutex::new(None);

/// Muss einmal beim Start aufgerufen werden, bevor [`map_mmio`] benutzt wird.
pub fn init(hhdm_offset: u64, memory_map: &'static [&'static Entry]) {
    let (level_4, _) = x86_64::registers::control::Cr3::read();
    let level_4 = (hhdm_offset + level_4.start_address().as_u64()) as *mut PageTable;
    let page_table = unsafe { OffsetPageTable::new(&mut *level_4, VirtAddr::new(hhdm_offset)) };
    *MEMORY.lock() = Some(Memory {
        hhdm_offset,
        page_table,
        frames: BootFrameAllocator {
            memory_map,
            next: 0,
        },
    });
}

/// Blendet eine 4-KiB-Seite mit Geräte-Registern an ihrer Adresse in der
/// HHDM ein (ohne Cache) und gibt einen Zeiger darauf zurück.
pub fn map_mmio(physical: u64) -> *mut u8 {
    let mut guard = MEMORY.lock();
    let memory = guard.as_mut().expect("memory::init wurde nicht aufgerufen");
    let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(physical));
    let virt = VirtAddr::new(memory.hhdm_offset + frame.start_address().as_u64());
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;
    unsafe {
        memory
            .page_table
            .map_to(
                Page::containing_address(virt),
                frame,
                flags,
                &mut memory.frames,
            )
            .expect("MMIO-Seite konnte nicht eingeblendet werden")
            .flush();
    }
    (virt + (physical - frame.start_address().as_u64())).as_mut_ptr()
}

/// Blendet ab `start` neuen, beschreibbaren Arbeitsspeicher der Größe `size`
/// ein. Jede Seite bekommt eine freie physische Seite aus der Speicherkarte.
pub fn map_memory(start: VirtAddr, size: u64) {
    let mut guard = MEMORY.lock();
    let memory = guard.as_mut().expect("memory::init wurde nicht aufgerufen");
    let pages = Page::<Size4KiB>::range_inclusive(
        Page::containing_address(start),
        Page::containing_address(start + (size - 1)),
    );
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
    for page in pages {
        let frame = memory
            .frames
            .allocate_frame()
            .expect("Kein freier Arbeitsspeicher mehr");
        unsafe {
            memory
                .page_table
                .map_to(page, frame, flags, &mut memory.frames)
                .expect("Seite konnte nicht eingeblendet werden")
                .flush();
        }
    }
}

/// Verteilt freie Speicherseiten aus der Speicherkarte von Limine, eine nach
/// der anderen. Zurückgeben kann man sie (noch) nicht.
struct BootFrameAllocator {
    memory_map: &'static [&'static Entry],
    next: usize,
}

impl BootFrameAllocator {
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> + '_ {
        self.memory_map
            .iter()
            .filter(|entry| entry.type_ == MEMMAP_USABLE)
            .flat_map(|entry| (entry.base..entry.base + entry.length).step_by(4096))
            .map(|address| PhysFrame::containing_address(PhysAddr::new(address)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}

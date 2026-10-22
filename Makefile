# lemOS – Build und Start
#
#   make          baut den Kernel und das bootfähige Image (build/lemos.iso)
#   make run      startet lemOS in QEMU (Fenster + serielle Ausgabe im Terminal)
#   make run-nox  startet lemOS ohne Fenster, nur mit serieller Ausgabe
#   make test     bootet headless und prüft Start, Shell und Neustart
#   make clean    räumt build/ und kernel/target/ auf

PROFILE ?= release
LIMINE_BRANCH := v11.x-binary

BUILD  := build
KERNEL := kernel/target/x86_64-unknown-none/$(PROFILE)/lemos-kernel
LIMINE := $(BUILD)/limine
ISO    := $(BUILD)/lemos.iso

QEMU       := qemu-system-x86_64
QEMU_FLAGS := -M q35 -m 256M -cdrom $(ISO) -no-reboot

CARGO_FLAGS := $(if $(filter release,$(PROFILE)),--release,)

INSTALL_HINT := Installieren unter Ubuntu/Debian/WSL: sudo apt install qemu-system-x86 xorriso git make gcc, unter macOS: brew install qemu xorriso
# Bricht mit einer klaren Meldung ab, wenn ein benötigtes Programm fehlt.
need = @command -v $(1) >/dev/null 2>&1 || { echo "Fehler: '$(1)' wurde nicht gefunden. $(2)"; exit 1; }

.PHONY: all kernel iso run run-nox test clean FORCE

all: iso

kernel:
	$(call need,cargo,Rust installieren: https://rustup.rs)
	cd kernel && cargo build $(CARGO_FLAGS)

# Limine wird beim ersten Build als fertige Binärversion heruntergeladen.
$(LIMINE)/limine:
	$(call need,git,$(INSTALL_HINT))
	$(call need,cc,$(INSTALL_HINT))
	rm -rf $(LIMINE)
	git clone https://github.com/limine-bootloader/limine.git --branch=$(LIMINE_BRANCH) --depth=1 $(LIMINE)
	$(MAKE) -C $(LIMINE)

iso: $(ISO)

$(ISO): kernel $(LIMINE)/limine boot/limine.conf
	$(call need,xorriso,$(INSTALL_HINT))
	rm -rf $(BUILD)/iso_root
	mkdir -p $(BUILD)/iso_root/boot/limine $(BUILD)/iso_root/EFI/BOOT
	cp $(KERNEL) $(BUILD)/iso_root/boot/lemos-kernel
	cp boot/limine.conf $(LIMINE)/limine-bios.sys $(LIMINE)/limine-bios-cd.bin \
	   $(LIMINE)/limine-uefi-cd.bin $(BUILD)/iso_root/boot/limine/
	cp $(LIMINE)/BOOTX64.EFI $(BUILD)/iso_root/EFI/BOOT/
	xorriso -as mkisofs -R -r -J -b boot/limine/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table -hfsplus \
		-apm-block-size 2048 --efi-boot boot/limine/limine-uefi-cd.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		$(BUILD)/iso_root -o $@ 2>/dev/null
	$(LIMINE)/limine bios-install $@ 2>/dev/null

run: iso
	$(call need,$(QEMU),$(INSTALL_HINT))
	$(QEMU) $(QEMU_FLAGS) -serial stdio

run-nox: iso
	$(call need,$(QEMU),$(INSTALL_HINT))
	$(QEMU) $(QEMU_FLAGS) -display none -serial stdio

# Bootet lemOS ohne Bildschirm und prüft Start, Shell und Neustart.
test: iso
	$(call need,$(QEMU),$(INSTALL_HINT))
	@scripts/boot-test.sh $(QEMU) $(QEMU_FLAGS)

clean:
	rm -rf $(BUILD)
	cd kernel && cargo clean

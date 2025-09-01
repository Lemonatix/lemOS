# scripts/test.mk
# Build + run kernel/test.asm
#   MODE=elf  (default)  -> userspace ELF using Linux syscalls
#   MODE=boot           -> 512-byte boot sector (real mode) in QEMU

# --- repo layout (auto from this file's location) ---
ROOT   := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/..)
KERNEL := $(ROOT)/kernel
BUILD  := $(ROOT)/build

# --- tools ---
ASM    := nasm
LD     := ld
QEMU32 := qemu-system-i386

# --- config ---
MODE    ?= elf                    # elf | boot
SRC_ASM := $(KERNEL)/test.asm

# --- outputs ---
ELF_OBJ := $(BUILD)/test.o
ELF_BIN := $(BUILD)/test
BOOTBIN := $(BUILD)/boot.bin

.PHONY: all run run-elf run-boot clean dirs help
all: run

help:
	@echo 'Usage: make -f scripts/test.mk run [MODE=elf|boot]'
	@echo '       make -f scripts/test.mk clean'

dirs:
	@mkdir -p $(BUILD)

# ===== ELF (userspace) =====
$(ELF_OBJ): $(SRC_ASM) | dirs
	$(ASM) -f elf64 -g -F dwarf -o $@ $<

$(ELF_BIN): $(ELF_OBJ)
	$(LD) -o $@ $^ -nostdlib -e _start

run-elf: $(ELF_BIN)
	$<

# ===== Boot sector (bare metal) =====
$(BOOTBIN): $(SRC_ASM) | dirs
	$(ASM) -f bin -o $@ $<

run-boot: $(BOOTBIN)
	$(QEMU32) -drive format=raw,file=$< -serial stdio

# ===== dispatcher =====
run: run-$(MODE)

# ===== cleanup =====
clean:
	rm -rf $(BUILD)

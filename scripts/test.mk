# scripts/Makefile

# ---- Paths (auto-detect repo root from scripts/) ----
ROOT   := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/..)
KERNEL := $(ROOT)/kernel
BUILD  := $(ROOT)/build

# ---- Tools ----
ASM    := nasm
LD     := gcc               # for ELF mode; can be 'ld' if you prefer
QEMU   := qemu-system-x86_64

# ---- Config ----
TEST_ASM := $(KERNEL)/test.asm
MODE    ?= elf              # elf | boot

# ---- Outputs ----
ELF_OBJ := $(BUILD)/test.o
ELF_BIN := $(BUILD)/test
BOOTBIN := $(BUILD)/boot.bin

.PHONY: all run run-elf run-boot clean dirs

all: run

run: run-$(MODE)

dirs:
	@mkdir -p $(BUILD)

# ---------- ELF (userspace) ----------
$(ELF_OBJ): $(TEST_ASM) | dirs
	$(ASM) -f elf64 -g -F dwarf -o $@ $<

$(ELF_BIN): $(ELF_OBJ)
	$(LD) -o $@ $^

run-elf: $(ELF_BIN)
	$<

# ---------- Boot sector (bare metal) ----------
$(BOOTBIN): $(TEST_ASM) | dirs
	$(ASM) -f bin -o $@ $<

run-boot: $(BOOTBIN)
	$(QEMU) -drive format=raw,file=$< -serial stdio

# ---------- Housekeeping ----------
clean:
	rm -rf $(BUILD)
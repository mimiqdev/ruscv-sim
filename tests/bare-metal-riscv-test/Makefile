# Makefile for RISC-V Bare Metal Tests
# Build RISC-V 64-bit ELF executables for testing the ruscv-sim simulator

# RISC-V toolchain prefix (change if different)
RISCV_PREFIX ?= riscv64-unknown-elf-

# Assembly and linking flags
AS      = $(RISCV_PREFIX)as
LD      = $(RISCV_PREFIX)ld
OBJDUMP = $(RISCV_PREFIX)objdump

# Output directories
OUTDIR  = .

# Subdirectories for each RISC-V extension
EXT_DIRS = rv64i rv64m rv64a rv64f rv64d rv64c

# Assembly flags
ASFLAGS = -march=rv64ima -mabi=lp64

# Linker script for bare metal
LDSCRIPT = -Tlinker.ld

# Find all .S files in extension subdirectories
SOURCES = $(foreach dir,$(EXT_DIRS),$(wildcard $(dir)/*.S))

# Generated ELF files (placed in respective subdirectories)
ELFS    = $(SOURCES:.S=.elf)

.PHONY: all clean disasm help $(EXT_DIRS)

all: $(ELFS)

# Pattern rule for building .elf from .S files in subdirectories
%.elf: %.S linker.ld
	@echo "  AS      $<"
	$(AS) $(ASFLAGS) $< -o $*.o
	@echo "  LD      $@"
	$(LD) $(LDSCRIPT) $*.o -o $@
	@rm -f $*.o

disasm: $(ELFS)
	@for elf in $(ELFS); do \
		echo "=== $$elf ==="; \
		$(OBJDUMP) -d $$elf; \
	done

clean:
	rm -f $(ELFS) $(foreach dir,$(EXT_DIRS),$(dir)/*.o)

help:
	@echo "RISC-V Bare Metal Test Programs Build System"
	@echo ""
	@echo "Directory Structure:"
	@echo "  rv64i/  - RV64I base integer instruction tests"
	@echo "  rv64m/  - RV64M integer multiplication/division tests"
	@echo "  rv64a/  - RV64A atomic instruction tests"
	@echo "  rv64f/  - RV64F single-precision floating-point tests"
	@echo "  rv64d/  - RV64D double-precision floating-point tests"
	@echo "  rv64c/  - RV64C compressed instruction tests"
	@echo ""
	@echo "Targets:"
	@echo "  all     - Build all test programs (default)"
	@echo "  disasm  - Disassemble ELF files"
	@echo "  clean   - Remove generated files"
	@echo "  help    - Show this help message"

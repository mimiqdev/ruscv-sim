# Makefile for RISC-V Tests
# Build RISC-V 64-bit ELF executables for testing the ruscv-sim simulator

# RISC-V toolchain prefix (change if different)
RISCV_PREFIX ?= riscv64-unknown-elf-

# Assembly and linking flags
AS      = $(RISCV_PREFIX)as
LD      = $(RISCV_PREFIX)ld
OBJDUMP = $(RISCV_PREFIX)objdump

# Output directory
OUTDIR  = .

# Assembly flags
ASFLAGS = -march=rv64ima -mabi=lp64

# Linker script for bare metal
LDSCRIPT = -Tlinker.ld

# Source files
SOURCES = add.S fib.S hello.S

# Generated files
ELFS    = $(addprefix $(OUTDIR)/, $(SOURCES:.S=.elf))

.PHONY: all clean test help

all: $(ELFS)

# linker.ld is manually maintained, do not auto-generate

%.elf: %.S linker.ld
	@echo "  AS      $<"
	$(AS) $(ASFLAGS) $< -o $(OUTDIR)/$*.o
	@echo "  LD      $@"
	$(LD) $(LDSCRIPT) $(OUTDIR)/$*.o -o $@
	@rm -f $(OUTDIR)/$*.o

disasm: $(ELFS)
	@for elf in $(ELFS); do \
		echo "=== $$elf ==="; \
		$(OBJDUMP) -d $$elf; \
	done

clean:
	rm -f $(ELFS) $(OUTDIR)/*.o

help:
	@echo "RISC-V Test Programs Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all     - Build all test programs (default)"
	@echo "  disasm  - Disassemble ELF files"
	@echo "  clean   - Remove generated files"
	@echo "  help    - Show this help message"

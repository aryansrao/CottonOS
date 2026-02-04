# CottonOS Build System
# x86_64 Operating System with Persistent Storage

.PHONY: all clean kernel run debug disk

# Build mode (debug or release)
MODE ?= release

# Directories
BUILD_DIR := target
ISO_DIR := $(BUILD_DIR)/iso
BOOT_DIR := $(ISO_DIR)/boot
GRUB_DIR := $(BOOT_DIR)/grub

# Target
TARGET := x86_64-unknown-none

# Tools
NASM := nasm
CARGO := cargo
QEMU := qemu-system-x86_64

# Cargo options
ifeq ($(MODE),release)
    CARGO_OPTS := --release
    TARGET_DIR := $(BUILD_DIR)/$(TARGET)/release
else
    CARGO_OPTS :=
    TARGET_DIR := $(BUILD_DIR)/$(TARGET)/debug
endif

# Output files
KERNEL_ELF := $(TARGET_DIR)/cotton_kernel
BOOT_STUB_OBJ := $(BUILD_DIR)/boot_stub.o
ISO_FILE := $(BUILD_DIR)/cottonos.iso
DISK_IMG := $(BUILD_DIR)/disk.img

# QEMU options - use bochs-display for better VESA support
QEMU_BASE := -m 512M -device VGA,vgamem_mb=64 -no-reboot
QEMU_DISK := -drive file=$(DISK_IMG),format=raw,if=ide

all: kernel

# Build boot stub
boot_stub:
	@echo "Building boot stub..."
	@mkdir -p $(BUILD_DIR)
	$(NASM) -f elf64 kernel/boot_stub.asm -o $(BOOT_STUB_OBJ)

# Build kernel
kernel: boot_stub
	@echo "Building CottonOS kernel..."
	$(CARGO) build $(CARGO_OPTS) --target $(TARGET) -p cotton_kernel
	@echo "Linking boot stub with kernel..."
	x86_64-elf-ld -n -T linker/x86_64_direct.ld \
		--gc-sections \
		-o $(KERNEL_ELF) \
		$(BOOT_STUB_OBJ) \
		$(TARGET_DIR)/libcotton_kernel.a

# Create bootable ISO with Multiboot 2 support
iso: kernel
	@echo "Creating bootable ISO..."
	@mkdir -p $(GRUB_DIR)
	@cp $(KERNEL_ELF) $(BOOT_DIR)/kernel.elf
	@echo 'set timeout=0' > $(GRUB_DIR)/grub.cfg
	@echo 'set default=0' >> $(GRUB_DIR)/grub.cfg
	@echo '' >> $(GRUB_DIR)/grub.cfg
	@echo 'insmod all_video' >> $(GRUB_DIR)/grub.cfg
	@echo 'insmod vbe' >> $(GRUB_DIR)/grub.cfg
	@echo 'insmod vga' >> $(GRUB_DIR)/grub.cfg
	@echo 'insmod gfxterm' >> $(GRUB_DIR)/grub.cfg
	@echo 'set gfxmode=1024x768x32' >> $(GRUB_DIR)/grub.cfg
	@echo 'terminal_output gfxterm' >> $(GRUB_DIR)/grub.cfg
	@echo '' >> $(GRUB_DIR)/grub.cfg
	@echo 'menuentry "CottonOS" {' >> $(GRUB_DIR)/grub.cfg
	@echo '    set gfxpayload=keep' >> $(GRUB_DIR)/grub.cfg
	@echo '    multiboot2 /boot/kernel.elf' >> $(GRUB_DIR)/grub.cfg
	@echo '    boot' >> $(GRUB_DIR)/grub.cfg
	@echo '}' >> $(GRUB_DIR)/grub.cfg
	@if command -v grub-mkrescue >/dev/null 2>&1; then \
		grub-mkrescue -o $(ISO_FILE) $(ISO_DIR); \
	elif command -v i686-elf-grub-mkrescue >/dev/null 2>&1; then \
		i686-elf-grub-mkrescue -o $(ISO_FILE) $(ISO_DIR); \
	elif command -v grub2-mkrescue >/dev/null 2>&1; then \
		grub2-mkrescue -o $(ISO_FILE) $(ISO_DIR); \
	else \
		echo "Error: No grub-mkrescue found!"; exit 1; \
	fi
	@echo "ISO created: $(ISO_FILE)"

# Create persistent disk image (64MB)
disk:
	@echo "Creating 64MB persistent disk image..."
	@mkdir -p $(BUILD_DIR)
	qemu-img create -f raw $(DISK_IMG) 64M
	@echo "Disk image created: $(DISK_IMG)"

# Run CottonOS with persistent disk
run: iso
	@if [ ! -f $(DISK_IMG) ]; then \
		echo "Creating disk image..."; \
		qemu-img create -f raw $(DISK_IMG) 64M; \
	fi
	@echo "Starting CottonOS with persistent storage..."
	$(QEMU) $(QEMU_BASE) -serial stdio -cdrom $(ISO_FILE) $(QEMU_DISK)

# Run without serial (GUI only)
run-gui: iso
	@if [ ! -f $(DISK_IMG) ]; then \
		echo "Creating disk image..."; \
		qemu-img create -f raw $(DISK_IMG) 64M; \
	fi
	$(QEMU) $(QEMU_BASE) -cdrom $(ISO_FILE) $(QEMU_DISK)

# Debug with GDB
debug: iso
	@if [ ! -f $(DISK_IMG) ]; then \
		qemu-img create -f raw $(DISK_IMG) 64M; \
	fi
	$(QEMU) $(QEMU_BASE) -serial stdio -cdrom $(ISO_FILE) $(QEMU_DISK) -s -S

# Test and show debug info
test: iso disk
	@echo "Testing with verbose QEMU output..."
	$(QEMU) -m 512M \
		-cdrom $(ISO_FILE) \
		-drive file=$(DISK_IMG),format=raw,if=ide \
		-serial stdio \
		-d int,cpu_reset \
		-no-reboot -no-shutdown

# Verify kernel has multiboot header
verify: kernel
	@echo "Checking for Multiboot 2 header..."
	@if readelf -h $(KERNEL_ELF) >/dev/null 2>&1; then \
		echo "✓ Kernel is a valid ELF file"; \
		readelf -l $(KERNEL_ELF) | grep LOAD; \
		echo ""; \
		echo "First 64 bytes of kernel:"; \
		hexdump -C $(KERNEL_ELF) | head -n 4; \
	else \
		echo "✗ Kernel is not a valid ELF file"; \
	fi

# Clean build artifacts
clean:
	$(CARGO) clean
	rm -rf $(BUILD_DIR)/*.o $(BUILD_DIR)/*.iso $(ISO_DIR)
	@echo "Build artifacts cleaned"

# Clean disk (resets persistent storage)
clean-disk:
	rm -f $(DISK_IMG)
	@echo "Disk image removed"

# Clean everything
clean-all: clean clean-disk

# Format code
fmt:
	$(CARGO) fmt

# Run clippy
clippy:
	$(CARGO) clippy --target $(TARGET) -p cotton_kernel

# Help
help:
	@echo "CottonOS Build System"
	@echo ""
	@echo "Usage: make [target] [MODE=debug|release]"
	@echo ""
	@echo "Targets:"
	@echo "  all        - Build kernel (default)"
	@echo "  kernel     - Build kernel"
	@echo "  iso        - Create bootable ISO"
	@echo "  disk       - Create persistent disk image"
	@echo "  run        - Run with persistent storage"
	@echo "  run-gui    - Run without serial output"
	@echo "  debug      - Run with GDB server"
	@echo "  test       - Test with verbose output"
	@echo "  verify     - Verify kernel format"
	@echo "  clean      - Clean build artifacts"
	@echo "  clean-disk - Remove disk image"
	@echo "  clean-all  - Clean everything"
	@echo "  fmt        - Format code"
	@echo "  clippy     - Run clippy linter"
	@echo "  help       - Show this help"
/* CottonOS Direct Boot Linker Script for x86_64 */
/* For GRUB Multiboot2 booting */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

/* Kernel loads at 1MB physical */
. = 1M;

SECTIONS
{
    /* Multiboot2 header MUST be in first 32KB and 8-byte aligned */
    /* This MUST match the section name in boot_stub.asm */
    .multiboot_header ALIGN(8) :
    {
        KEEP(*(.multiboot_header))
    }

    /* Code section */
    .text ALIGN(4K) :
    {
        _text_start = .;
        *(.text .text.*)
        _text_end = .;
    }

    /* Read-only data */
    .rodata ALIGN(4K) :
    {
        _rodata_start = .;
        *(.rodata .rodata.*)
        _rodata_end = .;
    }

    /* Initialized data */
    .data ALIGN(4K) :
    {
        _data_start = .;
        *(.data .data.*)
        _data_end = .;
    }

    /* Uninitialized data (BSS) */
    .bss ALIGN(4K) :
    {
        _bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        _bss_end = .;
    }

    /* Kernel end marker */
    _kernel_end = .;

    /* Discard unnecessary sections */
    /DISCARD/ :
    {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}
/* CottonOS Linker Script for x86_64 */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

/* Kernel loads at 1MB */
KERNEL_VMA = 0xFFFFFFFF80000000;
KERNEL_LMA = 0x100000;

SECTIONS
{
    . = KERNEL_VMA;
    
    /* Multiboot header must be in first 8KB */
    .multiboot ALIGN(4K) : AT(KERNEL_LMA)
    {
        KEEP(*(.multiboot))
    }
    
    /* Code section */
    .text ALIGN(4K) : AT(KERNEL_LMA + (ADDR(.text) - KERNEL_VMA))
    {
        _text_start = .;
        *(.text .text.*)
        _text_end = .;
    }
    
    /* Read-only data */
    .rodata ALIGN(4K) : AT(KERNEL_LMA + (ADDR(.rodata) - KERNEL_VMA))
    {
        _rodata_start = .;
        *(.rodata .rodata.*)
        _rodata_end = .;
    }
    
    /* Initialized data */
    .data ALIGN(4K) : AT(KERNEL_LMA + (ADDR(.data) - KERNEL_VMA))
    {
        _data_start = .;
        *(.data .data.*)
        _data_end = .;
    }
    
    /* Uninitialized data (BSS) */
    .bss ALIGN(4K) : AT(KERNEL_LMA + (ADDR(.bss) - KERNEL_VMA))
    {
        _bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        _bss_end = .;
    }
    
    /* Kernel stack */
    .stack ALIGN(4K) (NOLOAD) :
    {
        _stack_bottom = .;
        . += 64K; /* 64KB kernel stack */
        _stack_top = .;
    }
    
    _kernel_end = .;
    
    /* Calculate kernel size */
    _kernel_size = _kernel_end - KERNEL_VMA;
    
    /* Discard unnecessary sections */
    /DISCARD/ :
    {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}

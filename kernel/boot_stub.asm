; CottonOS Boot Stub - 32-bit to 64-bit transition
; Multiboot 2 specification
; Assembled with: nasm -f elf64 boot_stub.asm -o boot_stub.o

section .multiboot_header
align 8
header_start:
    dd 0xe85250d6                ; Multiboot 2 magic number
    dd 0                         ; Architecture: 0 = i386 32-bit protected mode
    dd header_end - header_start ; Header length
    ; Checksum
    dd 0x100000000 - (0xe85250d6 + 0 + (header_end - header_start))
    
    ; Framebuffer tag
align 8
framebuffer_tag_start:
    dw 5    ; type = framebuffer
    dw 0    ; flags = NOT optional (required!)
    dd framebuffer_tag_end - framebuffer_tag_start  ; size
    dd 1024 ; width
    dd 768  ; height
    dd 32   ; depth
framebuffer_tag_end:

    ; Module alignment tag
align 8
    dw 6    ; type = module alignment
    dw 0    ; flags
    dd 8    ; size

    ; End tag
align 8
    dw 0    ; type = end
    dw 0    ; flags
    dd 8    ; size
header_end:

section .data
; Save multiboot info here where 64-bit code can access it
global multiboot_magic_saved
global multiboot_info_saved
multiboot_magic_saved: dq 0
multiboot_info_saved:  dq 0

section .bss
align 16
stack_bottom:
    resb 65536  ; 64KB stack
stack_top:

; Page tables for identity mapping (must be 4KB aligned)
align 4096
pml4_table:
    resb 4096
pdpt_table:
    resb 4096
pd_table:
    resb 4096

section .rodata
gdt64:
    dq 0                        ; null descriptor
.code: equ $ - gdt64
    dq (1<<43) | (1<<44) | (1<<47) | (1<<53)  ; code segment
.data: equ $ - gdt64
    dq (1<<44) | (1<<47) | (1<<41)             ; data segment
.pointer:
    dw $ - gdt64 - 1
    dq gdt64

section .text
bits 32
global _start
extern _start64

_start:
    ; Set up stack
    mov esp, stack_top
    
    ; Save multiboot info immediately (EAX = magic, EBX = info pointer)
    mov dword [multiboot_magic_saved], eax
    mov dword [multiboot_info_saved], ebx
    
    ; Output 'B' to serial port (0x3F8) for debug
    mov dx, 0x3F8
    mov al, 'B'
    out dx, al
    
    ; Set up identity paging (map first 4GB using 2MB pages)
    
    ; PML4[0] -> PDPT
    mov eax, pdpt_table
    or eax, 0x03            ; present + writable
    mov [pml4_table], eax
    
    ; PDPT[0] -> PD  (for first 1GB)
    mov eax, pd_table
    or eax, 0x03            ; present + writable
    mov [pdpt_table], eax
    
    ; Fill PD with 2MB pages (identity map first 1GB)
    mov ecx, 512
    mov edi, pd_table
    mov eax, 0x83           ; present + writable + huge (2MB)
.fill_pd:
    mov [edi], eax
    add eax, 0x200000       ; next 2MB
    add edi, 8
    loop .fill_pd
    
    ; Output 'o' to serial
    mov dx, 0x3F8
    mov al, 'o'
    out dx, al
    
    ; Load PML4 into CR3
    mov eax, pml4_table
    mov cr3, eax
    
    ; Enable PAE in CR4
    mov eax, cr4
    or eax, 1 << 5          ; PAE bit
    mov cr4, eax
    
    ; Enable long mode in EFER MSR
    mov ecx, 0xC0000080     ; EFER MSR
    rdmsr
    or eax, 1 << 8          ; LME bit
    wrmsr
    
    ; Enable paging (and protected mode, but that's already on from GRUB)
    mov eax, cr0
    or eax, 1 << 31         ; PG bit
    mov cr0, eax
    
    ; Output 'o' to serial
    mov dx, 0x3F8
    mov al, 'o'
    out dx, al
    
    ; Load 64-bit GDT
    lgdt [gdt64.pointer]
    
    ; Far jump to 64-bit code
    jmp gdt64.code:long_mode_entry

bits 64
long_mode_entry:
    ; Set up segment registers
    mov ax, gdt64.data
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    
    ; Set up 64-bit stack
    mov rsp, stack_top
    
    ; Output 't' to serial
    mov dx, 0x3F8
    mov al, 't'
    out dx, al
    
    ; Output '6' to confirm we're in 64-bit mode
    mov al, '6'
    out dx, al
    mov al, '4'
    out dx, al
    mov al, ' '
    out dx, al
    
    ; Output 'C' before call
    mov dx, 0x3F8
    mov al, 'C'
    out dx, al
    
    ; Align stack to 16 bytes (required by System V ABI)
    and rsp, -16
    
    ; Pass multiboot info pointer as first argument
    mov edi, dword [multiboot_info_saved]
    
    ; Call Rust entry point
    call _start64
    
    ; Output 'R' if we somehow return
    mov dx, 0x3F8
    mov al, 'R'
    out dx, al
    
    ; Should never return, halt if it does
.halt:
    cli
    hlt
    jmp .halt
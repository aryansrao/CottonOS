; CottonOS x86_64 Multiboot2 Header
; Compatible with GRUB2 and other Multiboot2 bootloaders

section .multiboot_header
header_start:
    ; Magic number
    dd 0xe85250d6                   ; Multiboot2 magic
    dd 0                            ; Architecture: i386/x86
    dd header_end - header_start    ; Header length
    dd -(0xe85250d6 + 0 + (header_end - header_start))  ; Checksum

    ; Information request tag
    align 8
    dw 1                            ; Type: information request
    dw 0                            ; Flags
    dd 24                           ; Size
    dd 6                            ; Request: memory map
    dd 8                            ; Request: framebuffer info
    dd 9                            ; Request: ELF symbols
    
    ; Framebuffer tag
    align 8
    dw 5                            ; Type: framebuffer
    dw 0                            ; Flags
    dd 20                           ; Size
    dd 1024                         ; Width
    dd 768                          ; Height
    dd 32                           ; Depth

    ; Module alignment tag
    align 8
    dw 6                            ; Type: module alignment
    dw 0                            ; Flags
    dd 8                            ; Size

    ; End tag
    align 8
    dw 0                            ; Type: end
    dw 0                            ; Flags
    dd 8                            ; Size
header_end:

section .bss
align 4096
stack_bottom:
    resb 16384                      ; 16 KB stack
stack_top:

section .text
global _start
extern kernel_main

_start:
    ; Set up stack
    mov esp, stack_top
    
    ; Push multiboot info pointer and magic
    push ebx                        ; Multiboot info structure
    push eax                        ; Multiboot magic number
    
    ; Call kernel main
    call kernel_main
    
    ; Halt if kernel returns
    cli
.halt:
    hlt
    jmp .halt

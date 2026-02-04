; CottonOS x86 Bootloader - Stage 2
; Sets up protected mode, long mode (if x86_64), and loads the kernel

[BITS 16]
[ORG 0x10000]

section .text
global stage2_start

stage2_start:
    ; Save boot drive
    mov [boot_drive], dl
    
    ; Set up segments
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0xFFFF
    sti
    
    ; Print message
    mov si, msg_stage2
    call print_string_16
    
    ; Detect CPU type
    call detect_cpu
    
    ; Check if 64-bit capable
    cmp byte [cpu_64bit], 1
    je .setup_long_mode
    
    ; Fall back to 32-bit protected mode
    jmp .setup_protected_mode

.setup_long_mode:
    mov si, msg_64bit
    call print_string_16
    
    ; Load kernel to memory
    call load_kernel
    
    ; Set up paging for long mode
    call setup_paging_64
    
    ; Enter long mode
    call enter_long_mode
    
    ; Should not return
    jmp halt

.setup_protected_mode:
    mov si, msg_32bit
    call print_string_16
    
    ; Load kernel to memory
    call load_kernel
    
    ; Enter protected mode
    call enter_protected_mode
    
    ; Should not return
    jmp halt

; ============================================
; 16-bit Functions
; ============================================

; Print string in 16-bit mode
print_string_16:
    pusha
    mov ah, 0x0E
.loop:
    lodsb
    test al, al
    jz .done
    int 0x10
    jmp .loop
.done:
    popa
    ret

; Detect CPU capabilities
detect_cpu:
    pusha
    
    ; Check for CPUID support
    pushfd
    pop eax
    mov ecx, eax
    xor eax, 0x200000       ; Flip ID bit
    push eax
    popfd
    pushfd
    pop eax
    xor eax, ecx
    jz .no_cpuid
    
    ; CPUID is supported, check for extended functions
    mov eax, 0x80000000
    cpuid
    cmp eax, 0x80000001
    jb .no_long_mode
    
    ; Check for long mode support
    mov eax, 0x80000001
    cpuid
    test edx, (1 << 29)     ; LM bit
    jz .no_long_mode
    
    ; Long mode supported
    mov byte [cpu_64bit], 1
    popa
    ret

.no_cpuid:
.no_long_mode:
    mov byte [cpu_64bit], 0
    popa
    ret

; Load kernel from disk
load_kernel:
    pusha
    
    mov si, msg_loading_kernel
    call print_string_16
    
    ; Set up for disk read
    mov ax, 0x2000          ; Load kernel at 0x20000
    mov es, ax
    xor bx, bx
    
    ; Read kernel (64 sectors = 32KB)
    mov ah, 0x02
    mov al, 64
    mov ch, 0
    mov cl, 34              ; Start after stage 2
    mov dh, 0
    mov dl, [boot_drive]
    int 0x13
    jc .error
    
    popa
    ret

.error:
    mov si, msg_kernel_error
    call print_string_16
    jmp halt

; ============================================
; Paging Setup for 64-bit
; ============================================

setup_paging_64:
    pusha
    
    ; Clear page tables area (0x1000 - 0x5000)
    mov edi, 0x1000
    mov ecx, 0x1000
    xor eax, eax
    rep stosd
    
    ; PML4 (Page Map Level 4) at 0x1000
    mov edi, 0x1000
    mov eax, 0x2003         ; Point to PDPT, present + writable
    mov [edi], eax
    
    ; PDPT (Page Directory Pointer Table) at 0x2000
    mov edi, 0x2000
    mov eax, 0x3003         ; Point to PD, present + writable
    mov [edi], eax
    
    ; PD (Page Directory) at 0x3000
    mov edi, 0x3000
    mov eax, 0x4003         ; Point to PT, present + writable
    mov [edi], eax
    
    ; PT (Page Table) at 0x4000 - identity map first 2MB
    mov edi, 0x4000
    mov eax, 0x0003         ; Present + writable
    mov ecx, 512            ; 512 entries = 2MB
.pt_loop:
    mov [edi], eax
    add eax, 0x1000         ; Next page
    add edi, 8
    loop .pt_loop
    
    popa
    ret

; ============================================
; Mode Transitions
; ============================================

enter_protected_mode:
    cli
    
    ; Load GDT
    lgdt [gdt32_descriptor]
    
    ; Set PE bit in CR0
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    
    ; Far jump to 32-bit code
    jmp 0x08:protected_mode_start

enter_long_mode:
    cli
    
    ; Load GDT
    lgdt [gdt64_descriptor]
    
    ; Set PAE and PGE bits in CR4
    mov eax, cr4
    or eax, (1 << 5) | (1 << 7)  ; PAE | PGE
    mov cr4, eax
    
    ; Load PML4 address into CR3
    mov eax, 0x1000
    mov cr3, eax
    
    ; Enable long mode in EFER MSR
    mov ecx, 0xC0000080     ; EFER MSR
    rdmsr
    or eax, (1 << 8)        ; LME bit
    wrmsr
    
    ; Enable paging
    mov eax, cr0
    or eax, (1 << 31) | 1   ; PG | PE
    mov cr0, eax
    
    ; Far jump to 64-bit code
    jmp 0x08:long_mode_start

halt:
    cli
    hlt
    jmp halt

; ============================================
; 32-bit Protected Mode Code
; ============================================

[BITS 32]

protected_mode_start:
    ; Set up segment registers
    mov ax, 0x10            ; Data segment
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, 0x90000        ; Set up stack
    
    ; Call kernel entry point (32-bit)
    mov eax, 0x20000        ; Kernel loaded here
    call eax
    
    ; Should not return
    jmp $

; ============================================
; 64-bit Long Mode Code
; ============================================

[BITS 64]

long_mode_start:
    ; Set up segment registers
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    
    ; Set up stack
    mov rsp, 0x90000
    
    ; Call kernel entry point (64-bit)
    mov rax, 0x20000
    call rax
    
    ; Should not return
    jmp $

; ============================================
; Data
; ============================================

[BITS 16]

boot_drive:         db 0
cpu_64bit:          db 0

msg_stage2:         db "CottonOS Stage 2", 13, 10, 0
msg_64bit:          db "64-bit CPU detected", 13, 10, 0
msg_32bit:          db "32-bit CPU detected", 13, 10, 0
msg_loading_kernel: db "Loading kernel...", 13, 10, 0
msg_kernel_error:   db "Kernel load error!", 13, 10, 0

; ============================================
; GDT for 32-bit Protected Mode
; ============================================

align 16
gdt32_start:
    ; Null descriptor
    dq 0
    
    ; Code segment (0x08)
    dw 0xFFFF               ; Limit low
    dw 0x0000               ; Base low
    db 0x00                 ; Base middle
    db 10011010b            ; Access: present, ring 0, code, readable
    db 11001111b            ; Flags: 4K granularity, 32-bit
    db 0x00                 ; Base high
    
    ; Data segment (0x10)
    dw 0xFFFF               ; Limit low
    dw 0x0000               ; Base low
    db 0x00                 ; Base middle
    db 10010010b            ; Access: present, ring 0, data, writable
    db 11001111b            ; Flags: 4K granularity, 32-bit
    db 0x00                 ; Base high
gdt32_end:

gdt32_descriptor:
    dw gdt32_end - gdt32_start - 1
    dd gdt32_start

; ============================================
; GDT for 64-bit Long Mode
; ============================================

align 16
gdt64_start:
    ; Null descriptor
    dq 0
    
    ; Code segment (0x08)
    dw 0x0000               ; Limit low (ignored in long mode)
    dw 0x0000               ; Base low
    db 0x00                 ; Base middle
    db 10011010b            ; Access: present, ring 0, code
    db 00100000b            ; Flags: long mode
    db 0x00                 ; Base high
    
    ; Data segment (0x10)
    dw 0x0000               ; Limit low
    dw 0x0000               ; Base low
    db 0x00                 ; Base middle
    db 10010010b            ; Access: present, ring 0, data
    db 00000000b            ; Flags
    db 0x00                 ; Base high
gdt64_end:

gdt64_descriptor:
    dw gdt64_end - gdt64_start - 1
    dd gdt64_start

; Padding
times 8192 - ($ - $$) db 0

; CottonOS x86 Bootloader - Stage 1
; This is the first stage bootloader loaded by BIOS at 0x7C00
; It sets up the environment and loads the second stage

[BITS 16]
[ORG 0x7C00]

; Constants
STAGE2_SEGMENT  equ 0x1000
STAGE2_OFFSET   equ 0x0000
STAGE2_SECTORS  equ 32        ; Load 16KB for stage 2
KERNEL_SEGMENT  equ 0x2000

section .text
global _start

_start:
    ; Disable interrupts during setup
    cli
    
    ; Set up segment registers
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00          ; Stack grows down from bootloader
    
    ; Save boot drive number
    mov [boot_drive], dl
    
    ; Enable interrupts
    sti
    
    ; Print welcome message
    mov si, msg_welcome
    call print_string
    
    ; Enable A20 line
    call enable_a20
    
    ; Load stage 2 bootloader
    mov si, msg_loading
    call print_string
    call load_stage2
    
    ; Check for errors
    jc disk_error
    
    ; Jump to stage 2
    mov si, msg_jumping
    call print_string
    
    ; Pass boot drive to stage 2
    mov dl, [boot_drive]
    jmp STAGE2_SEGMENT:STAGE2_OFFSET

; ============================================
; Functions
; ============================================

; Print null-terminated string
; Input: SI = pointer to string
print_string:
    pusha
    mov ah, 0x0E            ; BIOS teletype function
.loop:
    lodsb                   ; Load byte from SI
    test al, al             ; Check for null terminator
    jz .done
    int 0x10                ; Print character
    jmp .loop
.done:
    popa
    ret

; Enable A20 line for accessing memory above 1MB
enable_a20:
    pusha
    
    ; Try BIOS method first
    mov ax, 0x2401
    int 0x15
    jnc .done
    
    ; Try keyboard controller method
    call .wait_input
    mov al, 0xAD            ; Disable keyboard
    out 0x64, al
    
    call .wait_input
    mov al, 0xD0            ; Read output port
    out 0x64, al
    
    call .wait_output
    in al, 0x60
    push ax
    
    call .wait_input
    mov al, 0xD1            ; Write output port
    out 0x64, al
    
    call .wait_input
    pop ax
    or al, 2                ; Enable A20
    out 0x60, al
    
    call .wait_input
    mov al, 0xAE            ; Enable keyboard
    out 0x64, al
    
    call .wait_input
    
.done:
    popa
    ret

.wait_input:
    in al, 0x64
    test al, 2
    jnz .wait_input
    ret

.wait_output:
    in al, 0x64
    test al, 1
    jz .wait_output
    ret

; Load stage 2 from disk
load_stage2:
    pusha
    
    ; Reset disk system
    xor ax, ax
    mov dl, [boot_drive]
    int 0x13
    jc .error
    
    ; Set up for disk read
    mov ax, STAGE2_SEGMENT
    mov es, ax
    mov bx, STAGE2_OFFSET
    
    ; Read sectors using extended BIOS functions if available
    mov ah, 0x02            ; Read sectors function
    mov al, STAGE2_SECTORS  ; Number of sectors
    mov ch, 0               ; Cylinder 0
    mov cl, 2               ; Start from sector 2
    mov dh, 0               ; Head 0
    mov dl, [boot_drive]
    int 0x13
    jc .error
    
    popa
    clc                     ; Clear carry (success)
    ret

.error:
    popa
    stc                     ; Set carry (error)
    ret

; Handle disk error
disk_error:
    mov si, msg_disk_error
    call print_string
    jmp halt

; Halt the system
halt:
    cli
    hlt
    jmp halt

; ============================================
; Data
; ============================================

boot_drive:     db 0
msg_welcome:    db "CottonOS Boot v1.0", 13, 10, 0
msg_loading:    db "Loading stage 2...", 13, 10, 0
msg_jumping:    db "Starting...", 13, 10, 0
msg_disk_error: db "Disk error!", 13, 10, 0

; ============================================
; Boot signature
; ============================================
times 510 - ($ - $$) db 0
dw 0xAA55

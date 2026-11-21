; 32-bit protected-mode side of RTRACK.BIN. RTRACK.COM (stub.asm) loads the image into
; extended memory and jumps to offset 0 with flat 4 GB segments whose base is the load
; address (code 08h, data 10h), interrupts disabled and
;   esi = load address (physical), edi = physical address of the boot information,
;   ebp = offset of the 16-bit return code in RTRACK.COM (code segment 18h).
; Here the interrupt table is set up, the interrupt controllers are moved to vectors
; 20h..2Fh (all lines masked; Rust unmasks the Sound Blaster's), and rust_main runs. Its
; result goes back to RTRACK.COM in eax; a CPU exception returns 100h + vector.
bits 32

extern rust_main, rust_irq
extern __bss_start, __bss_end, __image_end, __file_end, stack_top

section .text.header progbits alloc exec
global _image_start
_image_start:
    jmp _start
    align 8, db 0
    db 'RTRK'                   ; +8  magic
    dd __image_end              ; +12 bytes of memory needed (code, data, bss, stack)
    dd __file_end               ; +16 bytes in the file

section .text
global _start, inb, outb, irq_enable, irq_disable

_start:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, stack_top
    cld
    mov ebx, edi                ; zero the bss (it may hold anything)
    mov edi, __bss_start
    mov ecx, __bss_end
    sub ecx, edi
    xor eax, eax
    rep stosb
    mov [base_phys], esi
    mov [boot_phys], ebx
    mov [pm_ret], ebp
    mov word [pm_ret+4], 0x18
    call build_idt
    lea eax, [esi + idt]        ; the IDT register takes a linear address
    mov [idtr+2], eax
    lidt [idtr]
    call remap_pic
    fninit
    push ebx                    ; rust_main(base, boot information)
    push esi
    call rust_main
    add esp, 8
exit_to_dos:                    ; eax = exit code
    cli
    jmp far [pm_ret]

; ---- interrupt table: exceptions 0..31, hardware interrupts 20h..2Fh, the rest ignored
build_idt:
    mov edi, idt
    xor ecx, ecx
.next:
    mov eax, ignore_int
    cmp ecx, 32
    jae .not_exc
    mov eax, ecx
    shl eax, 4
    add eax, exc_stubs
    jmp .set
.not_exc:
    cmp ecx, 0x30
    jae .set
    mov eax, ecx
    sub eax, 0x20
    shl eax, 4
    add eax, irq_stubs
.set:
    mov [edi], ax
    mov word [edi+2], 0x08
    mov byte [edi+4], 0
    mov byte [edi+5], 0x8E      ; present, ring 0, 32-bit interrupt gate
    shr eax, 16
    mov [edi+6], ax
    add edi, 8
    inc ecx
    cmp ecx, 256
    jb .next
    ret

align 16
exc_stubs:
%assign v 0
%rep 32
    push dword v
    jmp exc_common
    align 16, db 0xCC
%assign v v+1
%endrep

align 16
irq_stubs:
%assign v 0
%rep 16
    push dword v
    jmp irq_common
    align 16, db 0xCC
%assign v v+1
%endrep

exc_common:                     ; a CPU exception: back to DOS with 100h + vector; the
    pop eax                     ; faulting EIP and the error code go into the boot info
    xor edx, edx                ; (offsets 40 and 44) for RTRACK.COM to print
    cmp eax, 8                  ; vectors with an error code: 8, 10..14, 17
    je .err
    cmp eax, 17
    je .err
    cmp eax, 10
    jb .noerr
    cmp eax, 14
    ja .noerr
.err:
    pop edx
.noerr:
    mov ecx, [esp]              ; EIP
    mov ebx, [boot_phys]
    sub ebx, [base_phys]
    mov [ebx+40], ecx
    mov [ebx+44], edx
    add eax, 0x100
    mov esp, stack_top
    jmp exit_to_dos

irq_common:                     ; [esp] = IRQ number
    pushad
    cld
    fsave [fpu_state]           ; the main program may be in the middle of a calculation
    push dword [esp+32]
    call rust_irq               ; acknowledges the interrupt at the PIC
    add esp, 4
    frstor [fpu_state]
    popad
    add esp, 4
    iretd

ignore_int:
    iretd

; ---- interrupt controllers: master to 20h, slave to 28h, everything masked
remap_pic:
    mov al, 0x11
    out 0x20, al
    call io_wait
    out 0xA0, al
    call io_wait
    mov al, 0x20
    out 0x21, al
    call io_wait
    mov al, 0x28
    out 0xA1, al
    call io_wait
    mov al, 4
    out 0x21, al
    call io_wait
    mov al, 2
    out 0xA1, al
    call io_wait
    mov al, 1
    out 0x21, al
    call io_wait
    out 0xA1, al
    call io_wait
    mov al, 0xFF
    out 0x21, al
    out 0xA1, al
    ret

io_wait:
    out 0x80, al
    ret

; ---- port access and interrupt flag for Rust (cdecl)
inb:                            ; (port) -> byte
    mov edx, [esp+4]
    xor eax, eax
    in al, dx
    ret

outb:                           ; (port, byte)
    mov edx, [esp+4]
    mov eax, [esp+8]
    out dx, al
    ret

irq_enable:
    sti
    ret

irq_disable:
    cli
    ret

section .data
align 4
pm_ret:     dd 0                ; far pointer to the 16-bit return code in RTRACK.COM
            dw 0
idtr:       dw 256*8-1
            dd 0
base_phys:  dd 0
boot_phys:  dd 0

section .bss
alignb 16
idt:        resb 256*8
fpu_state:  resb 108

; RTRACK.COM: DOS loader of the Rustracker.
;   RTRACK [song.mod]          (without a name: NEON.MOD next to RTRACK.COM)
; Loads RTRACK.BIN (32-bit image, see entry.asm) and the song into extended memory (XMS
; when HIMEM.SYS is loaded, otherwise the memory reported by INT 15h/88h above 1.1 MB),
; reserves a DMA buffer in conventional memory, switches to VGA mode 13h and starts the
; image in 32-bit protected mode. When it returns, real mode, the interrupt controllers
; and text mode are restored, and the messages the image left in the log buffer are shown.
; Needs a 386 or newer in real mode (no EMM386, QEMM or Windows DOS box).
cpu 386
bits 16
org 100h

IOBUF_SIZE equ 32768
LOG_SIZE   equ 2048

start:
    mov sp, stack_top
    mov bx, prog_end            ; give the rest of conventional memory back to DOS
    add bx, 15
    shr bx, 4
    mov ah, 4Ah
    int 21h
    mov dx, msg_mem
    jc fail
    smsw ax                     ; protected mode bit set: a memory manager runs us in V86
    test al, 1
    mov dx, msg_v86
    jnz fail
    xor eax, eax
    mov ax, cs
    shl eax, 4
    mov [cs_phys], eax
    mov [real_cs], cs
    mov byte [logbuf], 0
    call get_prefix
    call parse_cmdline

    mov dx, binpath             ; RTRACK.BIN: header, then rewind
    mov ax, 3D00h
    int 21h
    mov dx, msg_bin
    jc fail
    mov [h_bin], ax
    mov bx, ax
    mov ah, 3Fh
    mov cx, 20
    mov dx, header
    int 21h
    mov dx, msg_bin
    jc fail
    cmp ax, 20
    jne fail
    cmp dword [header+8], 'RTRK'
    jne fail
    mov eax, [header+12]
    add eax, 4095
    and eax, 0FFFFF000h
    mov [img_mem], eax
    mov ax, 4200h
    xor cx, cx
    xor dx, dx
    int 21h

    mov dx, modpath             ; the song: size, then rewind
    mov ax, 3D00h
    int 21h
    jc .nomod
    mov [h_mod], ax
    mov bx, ax
    mov ax, 4202h
    xor cx, cx
    xor dx, dx
    int 21h
    mov [mod_len], ax
    mov [mod_len+2], dx
    mov ax, 4200h
    xor cx, cx
    xor dx, dx
    int 21h
    mov eax, [mod_len]
    test eax, eax
    jz .nomod
    cmp eax, 4*1024*1024
    jbe .modok
.nomod:
    mov dx, msg_mod
    jmp fail
.modok:
    mov eax, [img_mem]          ; image, song and 4 KB to spare, in KB
    add eax, [mod_len]
    add eax, 4096+1023
    shr eax, 10
    mov [need_kb], eax
    call alloc_ext

    mov bx, [h_bin]
    xor edi, edi
    mov ecx, [header+16]
    call load_file
    mov bx, [h_mod]
    mov edi, [img_mem]
    mov ecx, [mod_len]
    call load_file
    mov bx, [h_bin]
    mov ah, 3Eh
    int 21h
    mov bx, [h_mod]
    mov ah, 3Eh
    int 21h

    mov ah, 48h                 ; 32 KB conventional memory for the DMA buffer
    mov bx, 2048
    int 21h
    mov dx, msg_mem
    jc fail_free
    mov [dma_seg], ax

    mov eax, [base]             ; boot information for the image
    mov [bi_base], eax
    mov eax, [need_kb]
    shl eax, 10
    mov [bi_mem], eax
    mov eax, [base]
    add eax, [img_mem]
    mov [bi_mod], eax
    mov eax, [mod_len]
    mov [bi_modlen], eax
    movzx eax, word [dma_seg]
    shl eax, 4
    mov [bi_dma], eax
    mov dword [bi_dmalen], 32768
    mov si, str_blaster
    mov cx, 8
    call find_var
    mov [bi_env], eax
    mov si, str_rtrack
    mov cx, 7
    call find_var
    mov [bi_opt], eax
    mov eax, [cs_phys]
    add eax, logbuf
    mov [bi_log], eax
    mov dword [bi_loglen], LOG_SIZE

    mov eax, [base]             ; descriptors: flat 32-bit code/data at the image,
    mov bx, gdt+08h             ; 16-bit code/data at this program (for the way back)
    call set_base
    mov bx, gdt+10h
    call set_base
    mov eax, [cs_phys]
    mov bx, gdt+18h
    call set_base
    mov bx, gdt+20h
    call set_base
    mov eax, [cs_phys]
    add eax, gdt
    mov [gdtr+2], eax

    mov ax, 13h                 ; VGA 320 x 200, 256 colours
    int 10h
    in al, 21h
    mov [pic1], al
    in al, 0A1h
    mov [pic2], al
    cmp byte [use_xms], 1       ; without XMS: A20 on again (INT 15h/87h may switch it off)
    je .a20
    in al, 92h
    or al, 2
    and al, 0FEh
    out 92h, al
.a20:
    mov [save_sp], sp
    cli
    lgdt [gdtr]
    mov esi, [base]
    mov edi, [cs_phys]
    add edi, bootinfo
    mov ebp, pm_return
    mov eax, cr0
    or al, 1
    mov cr0, eax
    jmp dword 08h:0

; ---- back from the image (16-bit protected mode, code segment 18h), eax = exit code
pm_return:
    mov bx, 20h                 ; 64 KB segments again before leaving protected mode
    mov ds, bx
    mov es, bx
    mov fs, bx
    mov gs, bx
    mov ss, bx
    mov sp, [save_sp]
    mov [exit_code], eax
    mov eax, cr0
    and al, 0FEh
    mov cr0, eax
    push word [real_cs]
    push word rm_back
    retf
rm_back:
    mov ax, cs
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov sp, [save_sp]
    lidt [rm_idtr]
    mov al, 11h                 ; interrupt controllers back to vectors 08h and 70h
    out 20h, al
    out 0A0h, al
    mov al, 08h
    out 21h, al
    mov al, 70h
    out 0A1h, al
    mov al, 4
    out 21h, al
    mov al, 2
    out 0A1h, al
    mov al, 1
    out 21h, al
    out 0A1h, al
    mov al, [pic1]
    out 21h, al
    mov al, [pic2]
    out 0A1h, al
    sti
    mov ax, 3
    int 10h
    mov si, logbuf
.log:
    lodsb
    test al, al
    jz .logged
    mov dl, al
    mov ah, 2
    int 21h
    jmp .log
.logged:
    mov eax, [exit_code]
    cmp eax, 100h
    jb .done
    mov dx, msg_exc
    mov ah, 9
    int 21h
    mov ax, [exit_code]
    sub ax, 100h
    call print_hex8
    mov dx, msg_crlf
    mov ah, 9
    int 21h
    mov byte [exit_code], 9
.done:
    call free_all
    mov al, [exit_code]
    mov ah, 4Ch
    int 21h

; ---- errors before the start: message, release memory, exit code 1
fail_free:
    push dx
    call free_all
    pop dx
fail:
    mov ah, 9
    int 21h
    mov ax, 4C01h
    int 21h

free_all:
    cmp word [xms_h], 0
    je .noxms
    mov ah, 0Dh
    mov dx, [xms_h]
    call far [xms]
    mov ah, 0Ah
    mov dx, [xms_h]
    call far [xms]
    mov word [xms_h], 0
.noxms:
    cmp word [dma_seg], 0
    je .nodma
    mov es, [dma_seg]
    mov ah, 49h
    int 21h
    push cs
    pop es
    mov word [dma_seg], 0
.nodma:
    ret

; ---- extended memory: [need_kb] KB, physical start in [base]
alloc_ext:
    mov ax, 4300h
    int 2Fh
    cmp al, 80h
    jne .bios
    mov ax, 4310h
    int 2Fh
    mov [xms], bx
    mov [xms+2], es
    push cs
    pop es
    mov ah, 03h                 ; A20 on
    call far [xms]
    mov ah, 09h
    mov dx, [need_kb]
    call far [xms]
    cmp ax, 1
    jne .xmsfail
    mov [xms_h], dx
    mov ah, 0Ch                 ; lock: dx:bx = physical address
    call far [xms]
    cmp ax, 1
    jne .xmsfail
    mov [base], bx
    mov [base+2], dx
    mov byte [use_xms], 1
    ret
.xmsfail:
    mov dx, msg_ext
    jmp fail_free
.bios:
    mov ah, 88h                 ; KB above 1 MB; the first 64 KB (HMA) stay untouched
    int 15h
    mov dx, msg_ext
    jc fail
    movzx eax, ax
    sub eax, 64
    jb fail
    cmp eax, [need_kb]
    jb fail
    mov dword [base], 110000h
    mov eax, [base]             ; int 15h/87h reaches only the first 16 MB
    mov edx, [need_kb]
    shl edx, 10
    add eax, edx
    cmp eax, 1000000h
    ja fail
    in al, 92h                  ; A20 on (fast A20 gate)
    or al, 2
    and al, 0FEh
    out 92h, al
    ret

; ---- file bx, ecx bytes, to offset edi of the extended memory block
load_file:
    mov [lf_h], bx
    mov [lf_left], ecx
    mov [lf_dst], edi
.next:
    mov ecx, [lf_left]
    test ecx, ecx
    jz .done
    cmp ecx, IOBUF_SIZE
    jbe .n
    mov ecx, IOBUF_SIZE
.n:
    mov [lf_n], ecx
    mov bx, [lf_h]
    mov dx, iobuf
    mov ah, 3Fh
    int 21h
    mov dx, msg_read
    jc fail_free
    cmp ax, cx
    jne fail_free
    call copy_up
    mov eax, [lf_n]
    add [lf_dst], eax
    sub [lf_left], eax
    jmp .next
.done:
    ret

copy_up:                        ; [lf_n] bytes of iobuf to [lf_dst] (rounded up to even)
    mov eax, [lf_n]
    inc eax
    and al, 0FEh
    cmp byte [use_xms], 1
    jne .bios
    mov [xm_len], eax
    mov word [xm_srch], 0
    mov word [xm_srco], iobuf
    mov [xm_srco+2], cs
    mov ax, [xms_h]
    mov [xm_dsth], ax
    mov eax, [lf_dst]
    mov [xm_dsto], eax
    mov si, xmove
    mov ah, 0Bh
    call far [xms]
    cmp ax, 1
    mov dx, msg_read
    jne fail_free
    ret
.bios:
    shr eax, 1
    push ax
    mov eax, [cs_phys]
    add eax, iobuf
    mov bx, gdt15+10h
    call set_base
    mov eax, [base]
    add eax, [lf_dst]
    mov bx, gdt15+18h
    call set_base
    pop cx
    mov si, gdt15
    mov ah, 87h
    int 15h
    mov dx, msg_read
    jc fail_free
    ret

set_base:                       ; descriptor at bx gets base eax
    mov [bx+2], ax
    shr eax, 16
    mov [bx+4], al
    mov [bx+7], ah
    ret

; ---- directory of RTRACK.COM (from the environment) -> binpath, default modpath
get_prefix:
    mov es, [2Ch]
    xor di, di
    xor cx, cx                  ; prefix length
    cmp byte [es:0], 0
    je .names
.scan:
    cmp word [es:di], 0
    je .found
    inc di
    jmp .scan
.found:
    cmp word [es:di+2], 1       ; strings after the environment (DOS 3+)
    jb .names
    lea si, [di+4]
    xor bx, bx
.copy:
    mov al, [es:si]
    inc si
    mov [binpath+bx], al
    inc bx
    test al, al
    jz .names
    cmp bx, 100
    jae .names
    cmp al, '\'
    je .mark
    cmp al, ':'
    jne .copy
.mark:
    mov cx, bx
    jmp .copy
.names:
    push cs
    pop es
    mov [prefix_len], cx
    mov si, binpath             ; same prefix for the default song
    mov di, modpath
    rep movsb
    mov di, modpath
    add di, [prefix_len]
    mov si, name_mod
    call strcpy
    mov di, binpath
    add di, [prefix_len]
    mov si, name_bin
    jmp strcpy

strcpy:                         ; ds:si -> es:di including the 0
    lodsb
    stosb
    test al, al
    jnz strcpy
    ret

; ---- first word of the command line replaces the default song
parse_cmdline:
    mov si, 81h
    movzx cx, byte [80h]
    add cx, si
.skip:
    cmp si, cx
    jae .none
    lodsb
    cmp al, ' '
    je .skip
    cmp al, 9
    je .skip
    cmp al, 13
    je .none
    dec si
    mov di, modpath
.copy:
    cmp si, cx
    jae .end
    lodsb
    cmp al, ' '
    jbe .end
    stosb
    cmp di, modpath+120
    jb .copy
.end:
    mov byte [di], 0
.none:
    ret

; ---- environment variable (si = 'NAME=', cx = its length) -> physical address of its value
;      in eax (0 if missing)
find_var:
    mov es, [2Ch]
    xor di, di
    mov bx, si
    mov dx, cx
.var:
    cmp byte [es:di], 0
    je .none
    push di
    mov si, bx
    mov cx, dx
    repe cmpsb
    je .found
    pop di
.skip:
    mov al, [es:di]
    inc di
    test al, al
    jnz .skip
    jmp .var
.found:
    pop ax
    xor eax, eax
    mov ax, es
    shl eax, 4
    movzx edi, di
    add eax, edi
    push cs
    pop es
    ret
.none:
    push cs
    pop es
    xor eax, eax
    ret

print_hex8:                     ; al
    push ax
    shr al, 4
    call .digit
    pop ax
.digit:
    and al, 15
    add al, '0'
    cmp al, '9'
    jbe .out
    add al, 7
.out:
    mov dl, al
    mov ah, 2
    int 21h
    ret

; ---- data
msg_mem     db 'RTRACK: nicht genug konventioneller Speicher.', 13, 10, '$'
msg_v86     db 'RTRACK: Die CPU laeuft im V86-Modus (EMM386, QEMM oder Windows).', 13, 10
            db 'Bitte DOS ohne Speichermanager starten (HIMEM.SYS ist in Ordnung).', 13, 10, '$'
msg_bin     db 'RTRACK: RTRACK.BIN fehlt oder ist beschaedigt.', 13, 10, '$'
msg_mod     db 'RTRACK: Die MOD-Datei fehlt oder ist zu gross (max. 4 MB).', 13, 10, '$'
msg_ext     db 'RTRACK: nicht genug Erweiterungsspeicher (etwa 10 MB noetig).', 13, 10, '$'
msg_read    db 'RTRACK: Fehler beim Laden.', 13, 10, '$'
msg_exc     db 'RTRACK: CPU-Ausnahme $'
msg_crlf    db 13, 10, '$'
name_bin    db 'RTRACK.BIN', 0
name_mod    db 'NEON.MOD', 0
str_blaster db 'BLASTER='
str_rtrack  db 'RTRACK='

align 8
gdt:        dq 0
            dw 0FFFFh, 0, 9A00h, 00CFh  ; 08h: 32-bit code, 4 GB
            dw 0FFFFh, 0, 9200h, 00CFh  ; 10h: 32-bit data, 4 GB
            dw 0FFFFh, 0, 9A00h, 0000h  ; 18h: 16-bit code, 64 KB
            dw 0FFFFh, 0, 9200h, 0000h  ; 20h: 16-bit data, 64 KB
gdt_end:
gdtr:       dw gdt_end - gdt - 1
            dd 0
rm_idtr:    dw 3FFh
            dd 0
gdt15:      times 16 db 0               ; descriptor table for INT 15h/87h
            dw 0FFFFh, 0, 9300h, 0      ; source
            dw 0FFFFh, 0, 9300h, 0      ; destination
            times 16 db 0

align 4
bootinfo:                               ; = BootInfo in dos.rs
bi_base     dd 0
bi_mem      dd 0
bi_mod      dd 0
bi_modlen   dd 0
bi_dma      dd 0
bi_dmalen   dd 0
bi_env      dd 0
bi_log      dd 0
bi_loglen   dd 0
bi_opt      dd 0

xmove:                                  ; XMS move structure
xm_len      dd 0
xm_srch     dw 0
xm_srco     dd 0
xm_dsth     dw 0
xm_dsto     dd 0

xms         dd 0
xms_h       dw 0
use_xms     db 0
dma_seg     dw 0

section .bss
alignb 4
cs_phys     resd 1
base        resd 1
img_mem     resd 1
mod_len     resd 1
need_kb     resd 1
exit_code   resd 1
lf_left     resd 1
lf_dst      resd 1
lf_n        resd 1
lf_h        resw 1
h_bin       resw 1
h_mod       resw 1
real_cs     resw 1
save_sp     resw 1
prefix_len  resw 1
pic1        resb 1
pic2        resb 1
header      resb 20
binpath     resb 128
modpath     resb 128
logbuf      resb LOG_SIZE
iobuf       resb IOBUF_SIZE + 2
            resb 2048
stack_top:
prog_end:

; Linux (i386) test harness for the DOS build: runs rust_selftest without hardware.
;   selftest SONG.MOD OUT.RAW SECONDS VIEW   (VIEW 0..6, or 7 for all in turn)
; OUT.RAW receives the last frame as 64000 palette indices and the palette (768 bytes).
; Port access is a no-op here; the interrupt is simulated inside rust_selftest.
bits 32
extern rust_selftest
global _start, inb, outb, irq_enable, irq_disable

section .text
_start:
    mov ebp, esp
    cmp dword [ebp], 5
    jne fail
    mov eax, 5                  ; open(song, O_RDONLY)
    mov ebx, [ebp+8]
    xor ecx, ecx
    int 0x80
    test eax, eax
    js fail
    mov ebx, eax
    mov eax, 3                  ; read(fd, song, max)
    mov ecx, song
    mov edx, 4*1024*1024
    int 0x80
    test eax, eax
    jle fail
    mov [song_len], eax
    mov esi, [ebp+20]           ; view
    call parse
    mov [view], ecx
    mov esi, [ebp+16]           ; seconds
    call parse
    push out
    push dword [view]
    push ecx
    push dword [song_len]
    push song
    call rust_selftest
    add esp, 20
    jmp written

parse:                          ; esi = decimal string -> ecx
    xor ecx, ecx
.num:
    movzx eax, byte [esi]
    sub eax, '0'
    jb .numdone
    cmp eax, 9
    ja .numdone
    imul ecx, 10
    add ecx, eax
    inc esi
    jmp .num
.numdone:
    ret

written:
    mov [result], eax
    mov eax, 8                  ; creat(out, 0644)
    mov ebx, [ebp+12]
    mov ecx, 420
    int 0x80
    test eax, eax
    js fail
    mov ebx, eax
    mov eax, 4                  ; write(fd, out, 64768)
    mov ecx, out
    mov edx, 64768
    int 0x80
    mov eax, 1                  ; exit(blocks mixed & 255)
    mov ebx, [result]
    and ebx, 255
    int 0x80
fail:
    mov eax, 1
    mov ebx, 255
    int 0x80

inb:
    xor eax, eax
    ret
outb:
irq_enable:
irq_disable:
    ret

section .bss
alignb 16
song:       resb 4*1024*1024
out:        resb 64768
song_len:   resd 1
result:     resd 1
view:       resd 1

; Runtime functions for the Rust code of the DOS build (no C library): memory functions that
; the compiler calls, and volatile 32-bit access for data shared with the interrupt handler.
; cdecl; eax, ecx, edx may be changed, ebx, esi, edi, ebp are preserved.
bits 32
section .text

global memcpy, memmove, memset, memcmp, bcmp, peek32, poke32

memcpy:                         ; (dst, src, n) -> dst
    push esi
    push edi
    mov edi, [esp+12]
    mov esi, [esp+16]
    mov ecx, [esp+20]
    mov eax, edi
    cld
    mov edx, ecx
    shr ecx, 2
    rep movsd
    mov ecx, edx
    and ecx, 3
    rep movsb
    pop edi
    pop esi
    ret

memmove:                        ; (dst, src, n) -> dst, regions may overlap
    push esi
    push edi
    mov edi, [esp+12]
    mov esi, [esp+16]
    mov ecx, [esp+20]
    mov eax, edi
    cmp edi, esi
    jbe .forward
    lea edx, [esi+ecx]
    cmp edi, edx
    jae .forward
    lea esi, [esi+ecx-1]        ; destination above an overlapping source: copy backwards
    lea edi, [edi+ecx-1]
    std
    rep movsb
    cld
    pop edi
    pop esi
    ret
.forward:
    cld
    rep movsb
    pop edi
    pop esi
    ret

memset:                         ; (dst, byte, n) -> dst
    push edi
    mov edi, [esp+8]
    movzx eax, byte [esp+12]
    mov ecx, [esp+16]
    mov edx, edi
    cld
    rep stosb
    mov eax, edx
    pop edi
    ret

memcmp:                         ; (a, b, n) -> <0, 0, >0
bcmp:
    push esi
    push edi
    mov esi, [esp+12]
    mov edi, [esp+16]
    mov ecx, [esp+20]
    xor eax, eax
    cld
    jecxz .done
    repe cmpsb
    je .done
    movzx eax, byte [esi-1]
    movzx edx, byte [edi-1]
    sub eax, edx
.done:
    pop edi
    pop esi
    ret

peek32:                         ; (addr) -> *addr, always read from memory
    mov eax, [esp+4]
    mov eax, [eax]
    ret

poke32:                         ; (addr, value), always written to memory
    mov eax, [esp+4]
    mov edx, [esp+8]
    mov [eax], edx
    ret

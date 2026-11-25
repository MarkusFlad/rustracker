; Runtime functions for the Rust code of the DOS build (no C library): memory functions that
; the compiler calls, volatile 32-bit access for data shared with the interrupt handler, and
; the Pentium's time stamp counter for the benchmark.
; cdecl; eax, ecx, edx may be changed, ebx, esi, edi, ebp are preserved.
bits 32
section .text

global memcpy, memmove, memset, memcmp, bcmp, peek32, poke32, rt_has_tsc, rt_tsc

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

rt_has_tsc:                     ; () -> 1 if the CPU has a time stamp counter (RDTSC), else 0
    push ebx
    pushfd                      ; CPUID exists if bit 21 of EFLAGS (ID) can be changed
    pop eax
    mov ecx, eax
    xor eax, 200000h
    push eax
    popfd
    pushfd
    pop eax
    push ecx
    popfd
    xor eax, ecx
    test eax, 200000h
    jz .no                      ; 386 and early 486: no CPUID, no TSC
    xor eax, eax
    cpuid
    test eax, eax
    jz .no
    mov eax, 1
    cpuid
    xor eax, eax
    test edx, 10h               ; feature bit 4: TSC
    jz .done
    inc eax
    jmp .done
.no:
    xor eax, eax
.done:
    pop ebx
    ret

rt_tsc:                         ; () -> CPU clock cycles since reset, as a double (in st0)
    rdtsc
    push edx
    push eax
    fild qword [esp]
    add esp, 8
    ret

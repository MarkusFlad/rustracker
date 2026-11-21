/* RTRACK.BIN: a flat image linked at offset 0 (the segments' base is the load address).
   The header (entry.asm) must come first; bss and stack follow the file contents. */
ENTRY(_image_start)
SECTIONS
{
    . = 0;
    .text : { *(.text.header) *(.text .text.*) }
    .rodata : { *(.rodata .rodata.*) }
    .data : { *(.data .data.*) }
    __file_end = .;
    .bss (NOLOAD) : ALIGN(16) { __bss_start = .; *(.bss .bss.* COMMON) __bss_end = .; }
    .stack (NOLOAD) : ALIGN(16) { . += 0x40000; stack_top = .; }
    __image_end = ALIGN(4096);
    /DISCARD/ : { *(.eh_frame*) *(.comment) *(.note*) }
}

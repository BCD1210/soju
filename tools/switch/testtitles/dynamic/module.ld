OUTPUT_ARCH(aarch64)
ENTRY(_start)
PHDRS {
 text PT_LOAD FLAGS(5);
 ro PT_LOAD FLAGS(4);
 data PT_LOAD FLAGS(6);
 dynamic PT_DYNAMIC;
 tls PT_TLS;
}
SECTIONS {
 . = 0;
 .text : { KEEP(*(.entry)) *(.text .text.*) *(.plt .plt.*) } :text
 . = ALIGN(4096);
 .rodata : { *(.rodata .rodata.*) } :ro
 .hash : { *(.hash) } :ro
 .gnu.hash : { *(.gnu.hash) } :ro
 .dynsym : { *(.dynsym) } :ro
 .dynstr : { *(.dynstr) } :ro
 .rela.dyn : { *(.rela.dyn) } :ro
 .rela.plt : { *(.rela.plt) } :ro
 .eh_frame_hdr : { *(.eh_frame_hdr) } :ro
 .eh_frame : { __probe_eh_frame = .; KEEP(*(.eh_frame)) QUAD(0); } :ro
 .gcc_except_table : { *(.gcc_except_table*) } :ro
 . = ALIGN(4096);
 .dynamic : { *(.dynamic) } :data :dynamic
 .data : { *(.data .data.*) *(.data.rel.ro*) *(.got .got.*) } :data
 .init_array : { KEEP(*(.init_array .init_array.*)) } :data
 .fini_array : { KEEP(*(.fini_array .fini_array.*)) } :data
 .tdata : { __probe_tls_start = .; *(.tdata .tdata.*) } :data :tls
 .tbss : { *(.tbss .tbss.*) } :data :tls
 __probe_tls_filesz = SIZEOF(.tdata);
 __probe_tls_size = ADDR(.tbss) + SIZEOF(.tbss) - ADDR(.tdata);
 .bss : { __bss_start = .; *(.bss .bss.* COMMON) __bss_end = .; } :data
 /DISCARD/ : { *(.comment .note*) }
}

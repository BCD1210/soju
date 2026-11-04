.section .entry,"ax"
.global _start
.hidden _start
.hidden __bss_start, __bss_end, _DYNAMIC
_start:
 b entry
 .word mod0 - _start
 .quad 0
mod0:
 .ascii "MOD0"
 .word _DYNAMIC - mod0
 .word __bss_start - mod0
 .word __bss_end - mod0
 .word 0
 .word 0
 .word 0
entry:
 adr x0, _start
 b module_entry

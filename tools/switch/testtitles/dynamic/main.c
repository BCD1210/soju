#include "abi.h"
__attribute__((constructor)) static void init(void) { lifecycle = lifecycle * 10 + 3; }
__attribute__((destructor)) static void fini(void) { lifecycle = lifecycle * 10 + 3; }
void module_entry(void) {
 extern void check_objects(void); check_objects();
 probe_check("imported data GLOB_DAT", sdk_data == 37 && sub_data == 19);
 probe_check("cross-module JUMP_SLOT and RELATIVE", sub_add(7) == 74 && sdk_add(2) == 50);
 sdk_data = 41;
 probe_check("shared imported data mutation", sub_add(7) == 78);
 probe_check("dependency init arrays", lifecycle == 123);
 probe_check("module TLS initialization and independence", sdk_tls() == 6 && sub_tls() == 101 && sdk_tls() == 7 && sub_tls() == 102);
}

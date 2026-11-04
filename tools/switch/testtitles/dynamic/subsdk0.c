#include "abi.h"
int sub_data = 19;
__thread int sub_local = 100;
int sub_add(int x) { return sdk_add(x) + sub_data; }
int sub_tls(void) { return ++sub_local; }
__attribute__((constructor)) static void init(void) { lifecycle = lifecycle * 10 + 2; }
__attribute__((destructor)) static void fini(void) { lifecycle = lifecycle * 10 + 2; }
void module_entry(void) {}

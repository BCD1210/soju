#include "abi.h"
int sdk_data = 37;
int lifecycle;
static int relative_value = 11;
static int *volatile relative_pointer = &relative_value;
__thread int sdk_local = 5;
int sdk_add(int x) { return x + sdk_data + *relative_pointer; }
int sdk_tls(void) { return ++sdk_local; }
__attribute__((constructor)) static void init(void) { lifecycle = 1; }
__attribute__((destructor)) static void fini(void) { lifecycle = lifecycle * 10 + 1; }
void module_entry(void) {}

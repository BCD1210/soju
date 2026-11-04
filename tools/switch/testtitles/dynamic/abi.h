#pragma once
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
void probe_check(const char *, int);
extern int sdk_data, sub_data, lifecycle;
int sdk_add(int);
int sub_add(int);
int sdk_tls(void);
int sub_tls(void);
#ifdef __cplusplus
}
#endif

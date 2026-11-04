#pragma once
#include <switch.h>
#include <stdio.h>
#include <string.h>
static unsigned failures, checks;
static bool check(const char *name, Result rc, bool valid) {
    char line[256];
    bool ok = R_SUCCEEDED(rc) && valid;
    checks++; failures += !ok;
    int n = snprintf(line, sizeof(line), "%s|%s|rc=%08x\n", ok ? "PASS" : "FAIL", name, rc);
    svcOutputDebugString(line, n);
    return ok;
}
#define CALL(expr) check(#expr, (expr), true)
static int finish(void) {
    char line[96];
    int n = snprintf(line, sizeof(line), "TOTAL|checks=%u|failures=%u\n", checks, failures);
    svcOutputDebugString(line, n);
    return failures ? 1 : 0;
}
// Keep probes independent: a failed service must not prevent other titles booting.
void __appInit(void) {
    hosversionSet(MAKEHOSVERSION(17,0,0));
    if (R_FAILED(smInitialize())) svcExitProcess();
}
void __appExit(void) { smExit(); }

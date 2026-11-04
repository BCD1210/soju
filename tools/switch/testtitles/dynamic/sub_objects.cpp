#include "objects.h"
Base::~Base() = default;
int Derived::value() const { return 73; }
extern "C" { int sub_unwound = 0; }
extern "C" Base *sub_object() { static Derived object; return &object; }
extern "C" void sub_throw() {
 struct Guard { ~Guard() { ++sub_unwound; } } guard;
 throw ProbeError{91};
}

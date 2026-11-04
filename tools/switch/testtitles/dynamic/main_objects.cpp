#include "abi.h"
#include "objects.h"
#include <typeinfo>
extern "C" void check_objects() {
 Base *object = sub_object();
 probe_check("cross-module RTTI and virtual dispatch", dynamic_cast<Derived *>(object) != nullptr && typeid(*object) == typeid(Derived) && object->value() == 73);
 int caught = 0;
 try { sub_throw(); } catch (const ProbeError &error) { caught = error.value; }
 probe_check("cross-module exception and stack cleanup", caught == 91 && sub_unwound == 1);
}

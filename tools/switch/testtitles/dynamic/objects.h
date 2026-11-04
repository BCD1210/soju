#pragma once
struct Base { virtual ~Base(); virtual int value() const = 0; };
struct Derived final : Base { int value() const override; };
struct ProbeError { int value; };
extern "C" Base *sub_object();
extern "C" void sub_throw();
extern "C" int sub_unwound;

//! Minimal bindings to the installed Apple Hypervisor SDK (hv_vm/hv_vcpu headers).
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Exception {
    pub syndrome: u64,
    pub virtual_address: u64,
    pub physical_address: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Exit {
    pub reason: u32,
    pub exception: Exception,
}
pub const PC: u32 = 31;
pub const CPSR: u32 = 34;
pub const SCTLR_EL1: u32 = 0xc080;
pub const SPSR_EL1: u32 = 0xc200;
pub const ELR_EL1: u32 = 0xc201;
pub const SP_EL0: u32 = 0xc208;
pub const ESR_EL1: u32 = 0xc290;
pub const VBAR_EL1: u32 = 0xc600;
pub const CNTKCTL_EL1: u32 = 0xc708;
pub const SP_EL1: u32 = 0xe208;
pub const TTBR0_EL1: u32 = 0xc100;
pub const TCR_EL1: u32 = 0xc102;
pub const MAIR_EL1: u32 = 0xc510;
pub const TPIDRRO_EL0: u32 = 0xde83;
pub const CPACR_EL1: u32 = 0xc082;

#[link(name = "Hypervisor", kind = "framework")]
unsafe extern "C" {
    pub fn hv_vm_create(config: *const c_void) -> i32;
    pub fn hv_vm_destroy() -> i32;
    pub fn hv_vm_map(address: *mut c_void, ipa: u64, size: usize, flags: u64) -> i32;
    pub fn hv_vm_unmap(ipa: u64, size: usize) -> i32;
    pub fn hv_vcpu_create(vcpu: *mut u64, exit: *mut *const Exit, config: *const c_void) -> i32;
    pub fn hv_vcpu_destroy(vcpu: u64) -> i32;
    pub fn hv_vcpu_run(vcpu: u64) -> i32;
    pub fn hv_vcpus_exit(vcpus: *const u64, count: u32) -> i32;
    pub fn hv_vcpu_get_reg(vcpu: u64, reg: u32, value: *mut u64) -> i32;
    pub fn hv_vcpu_set_reg(vcpu: u64, reg: u32, value: u64) -> i32;
    pub fn hv_vcpu_get_sys_reg(vcpu: u64, reg: u32, value: *mut u64) -> i32;
    pub fn hv_vcpu_set_sys_reg(vcpu: u64, reg: u32, value: u64) -> i32;
}

#include <Hypervisor/Hypervisor.h>
#include <stdint.h>
#include <mach/mach_time.h>
#include <string.h>
/* Pointer-only ABI: the SDK's vector-by-value ABI stays entirely in C. */
static const hv_sys_reg_t system_registers[7] = {
    HV_SYS_REG_SP_EL0, HV_SYS_REG_SP_EL1, HV_SYS_REG_TPIDRRO_EL0,
    HV_SYS_REG_TPIDR_EL0, HV_SYS_REG_ELR_EL1, HV_SYS_REG_SPSR_EL1,
    HV_SYS_REG_ESR_EL1,
};
int soju_context_save(uint64_t cpu, uint64_t *registers, uint64_t *system, uint8_t *vectors) {
    hv_return_t rc;
    for (unsigned i = 0; i < 35; ++i)
        if ((rc = hv_vcpu_get_reg(cpu, (hv_reg_t)i, &registers[i]))) return rc;
    for (unsigned i = 0; i < 7; ++i)
        if ((rc = hv_vcpu_get_sys_reg(cpu, system_registers[i], &system[i]))) return rc;
    for (unsigned i = 0; i < 32; ++i) {
        hv_simd_fp_uchar16_t value;
        if ((rc = hv_vcpu_get_simd_fp_reg(cpu, (hv_simd_fp_reg_t)i, &value))) return rc;
        memcpy(vectors + i * 16, &value, 16);
    }
    return 0;
}
int soju_context_load(uint64_t cpu, const uint64_t *registers, const uint64_t *system, const uint8_t *vectors) {
    hv_return_t rc;
    for (unsigned i = 0; i < 35; ++i)
        if ((rc = hv_vcpu_set_reg(cpu, (hv_reg_t)i, registers[i]))) return rc;
    for (unsigned i = 0; i < 7; ++i)
        if ((rc = hv_vcpu_set_sys_reg(cpu, system_registers[i], system[i]))) return rc;
    for (unsigned i = 0; i < 32; ++i) {
        hv_simd_fp_uchar16_t value;
        memcpy(&value, vectors + i * 16, 16);
        if ((rc = hv_vcpu_set_simd_fp_reg(cpu, (hv_simd_fp_reg_t)i, value))) return rc;
    }
    return 0;
}

/* The guest timer uses the VM's native counter (zero offset). The exposed
 * 19.2 MHz userspace clock remains independently scaled by the HLE runtime. */
int soju_preemption_timer(uint64_t cpu, uint64_t nanoseconds) {
    mach_timebase_info_data_t timebase;
    if (mach_timebase_info(&timebase) != KERN_SUCCESS) return -1;
    uint64_t delta = (uint64_t)((__uint128_t)nanoseconds * timebase.denom / timebase.numer);
    hv_return_t rc = hv_vcpu_set_sys_reg(cpu, HV_SYS_REG_CNTV_CVAL_EL0, mach_absolute_time() + (delta ? delta : 1));
    if (rc) return rc;
    if ((rc = hv_vcpu_set_sys_reg(cpu, HV_SYS_REG_CNTV_CTL_EL0, 1))) return rc;
    return hv_vcpu_set_vtimer_mask(cpu, false);
}

#include <pthread/qos.h>
int soju_worker_qos(void) {
    return pthread_set_qos_class_self_np(QOS_CLASS_USER_INITIATED, 0);
}

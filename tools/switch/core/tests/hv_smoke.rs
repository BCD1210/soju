#![cfg(all(feature = "hv", target_os = "macos", target_arch = "aarch64"))]
use soju_switch_core::hv::{Cpu, Error, TIMER_HZ};

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn el0_svc_and_virtual_timer() {
    {
        let mut cpu = Cpu::new(&[0xd28000e0, 0xd40004c1, 0xd2800120, 0xd40000e1]).unwrap(); // mov x0,#7; svc #0x26
        let exit = cpu.run_until_svc().unwrap();
        assert_eq!(exit.immediate, 0x26);
        assert_eq!(exit.registers[0], 7);
        assert_eq!(exit.guest_pc, 0x14008);
        assert_eq!(exit.timer_reads, 0);
        println!("EL0 SVC 0x26 reached host with x0=7");
        let resumed = cpu.run_until_svc().unwrap();
        assert_eq!(resumed.immediate, 7);
        assert_eq!(resumed.registers[0], 9);
        assert_eq!(resumed.guest_pc, 0x14010);
    }
    {
        let mut cpu = Cpu::new(&[
            0xd53be001, // mrs x1,CNTFRQ_EL0
            0xd53be042, // mrs x2,CNTVCT_EL0
            0xd53be043, // mrs x3,CNTVCT_EL0
            0xd28000e0, 0xd40004c1,
        ])
        .unwrap();
        let exit = cpu.run_until_svc().unwrap();
        assert_eq!(exit.immediate, 0x26);
        assert_eq!(exit.registers[0], 7);
        assert_eq!(exit.registers[1], TIMER_HZ);
        assert!(exit.registers[3] >= exit.registers[2]);
        assert_eq!(exit.timer_reads, 3); // proves reads trapped rather than leaking host Hz
        println!(
            "CNTFRQ={} Hz; two monotonic CNTVCT reads trapped",
            exit.registers[1]
        );
    }
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn looping_guest_is_canceled() {
    let mut cpu = Cpu::new(&[0x14000000]).unwrap();
    assert!(matches!(cpu.run_until_svc(), Err(Error::Timeout)));
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn unsupported_exception_stops() {
    let mut cpu = Cpu::new(&[0]).unwrap();
    assert!(matches!(
        cpu.run_until_svc(),
        Err(Error::UnexpectedGuestException(_))
    ));
}

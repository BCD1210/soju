#![cfg(all(feature = "hv", target_os = "macos", target_arch = "aarch64"))]
use soju_switch_core::hv::{Cpu, Error, TIMER_HZ};
use soju_switch_core::nro::{BASE, CONFIG, Nro, TLS};
use std::io::Cursor;

fn image(program: &[u32]) -> Nro {
    let mut bytes = vec![0; 0x3000];
    bytes[0x10..0x14].copy_from_slice(b"NRO0");
    for (offset, value) in [
        (0, 0x14000020u32),
        (0x18, 0x3000),
        (0x24, 0x1000),
        (0x28, 0x1000),
        (0x2c, 0x1000),
        (0x30, 0x2000),
        (0x34, 0x1000),
        (0x38, 0x1000),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (index, instruction) in program.iter().enumerate() {
        bytes[0x80 + index * 4..0x84 + index * 4].copy_from_slice(&instruction.to_le_bytes());
    }
    Nro::read(&mut Cursor::new(bytes)).unwrap()
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn nro_boot_abi_and_page_permissions() {
    // Exercise ABI memory and TLS from real EL0 instructions, then write/read BSS.
    let boot = image(&[
        0xf9400002, // ldr x2,[x0] (MainThreadHandle key/flags)
        0xf9400403, // ldr x3,[x0,#8] (handle)
        0xd53bd064, // mrs x4,TPIDRRO_EL0
        0xd2c00025, // mov x5,#0x100000000
        0x91400ca5, // add x5,x5,#3,lsl #12 (BSS)
        0xf94000a6, // ldr x6,[x5] (zero)
        0xf90000a3, // str x3,[x5]
        0xf94000a7, // ldr x7,[x5]
        0xd40000e1,
    ])
    .protected_homebrew()
    .unwrap();
    {
        let mut cpu = Cpu::from_homebrew(&boot).unwrap();
        let exit = cpu.run_until_svc().unwrap();
        assert_eq!(exit.immediate, 7);
        assert_eq!(
            &exit.registers[..8],
            &[CONFIG, u64::MAX, 1, 0x101, TLS, BASE + 0x3000, 0, 0x101]
        );
    }
    // Adjacent 4 KiB pages have distinct permissions even within a host 16 KiB page.
    for offset in [0u32, 1] {
        let boot = image(&[
            0xd2c00025,
            0x914000a5 | (offset << 10),
            0xf90000a0,
            0xd40000e1,
        ])
        .protected_homebrew()
        .unwrap();
        let mut cpu = Cpu::from_homebrew(&boot).unwrap();
        match cpu.run_until_svc() {
            Err(Error::UnexpectedGuestException(esr)) => {
                assert_eq!(esr >> 26, 0x24); // lower EL data abort
                assert_eq!(esr & 0x3f, 0x0f); // L3 permission fault
                assert_ne!(esr & (1 << 6), 0); // write
            }
            other => panic!("write to text/rodata succeeded: {other:?}"),
        }
    }
    for address in [
        BASE + 0x1000,
        BASE + 0x2000,
        BASE + 0x3000,
        TLS,
        0x10000,
        0x11000,
    ] {
        let mut boot = image(&[0xd40000e1]).protected_homebrew().unwrap();
        boot.pc = address;
        let mut cpu = Cpu::from_homebrew(&boot).unwrap();
        match cpu.run_until_svc() {
            Err(Error::UnexpectedGuestException(esr)) => assert_eq!(esr >> 26, 0x20),
            other => panic!("non-executable/private page executed: {other:?}"),
        }
    }
}

#[test]
#[ignore = "requires local homebrew and entitlement; use scripts/test-switch-hv.sh"]
fn optional_homebrew_reaches_first_svc() {
    let Some(path) = std::env::var_os("SOJU_SWITCH_TEST_NRO") else {
        return;
    };
    let nro = Nro::read(&mut std::fs::File::open(path).unwrap()).unwrap();
    let mut cpu = Cpu::from_homebrew(&nro.protected_homebrew().unwrap()).unwrap();
    let exit = cpu.run_until_svc().unwrap();
    println!(
        "Homebrew first SVC: {:#x} at {:#x}",
        exit.immediate, exit.guest_pc
    );
}

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

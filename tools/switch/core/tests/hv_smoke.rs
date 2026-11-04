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

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn shared_process_threads_tls_simd_and_translation_generations() {
    use soju_switch_core::hv::{Process, ThreadState};
    use std::sync::{Arc, mpsc};
    let boot = image(&[
        0x9e670020, // fmov d0,x1
        0xd53bd064, // mrs x4,TPIDRRO_EL0
        0xf9000081, // str x1,[x4]
        0xf9400002, 0x91000442, 0xf9000002, 0xd4000401, 0x9e660003, // fmov x3,d0
        0xf9400002, 0xd4000401,
    ])
    .protected_homebrew()
    .unwrap();
    let process = Process::new(&boot).unwrap();
    assert!(matches!(Process::new(&boot), Err(Error::VmBusy)));
    {
        let mut g = process.lock().unwrap();
        g.map_zeroed(TLS + 0x4000, 0x2000, 3).unwrap();
    }
    let mut workers = Vec::new();
    for index in 0..2 {
        let p = Arc::clone(&process);
        let mut state = ThreadState::from_homebrew(&boot);
        state.registers[0] = BASE + 0x2000;
        state.registers[1] = 0x1111 + index;
        state.tpidrro_el0 = TLS + 0x4000 + index * 0x1000;
        let (send, receive) = mpsc::channel();
        let (reply, result) = mpsc::channel();
        let join = std::thread::spawn(move || {
            let mut cpu = Cpu::attach(p, state).unwrap();
            while receive.recv().unwrap_or(false) {
                reply.send(cpu.run_until_svc().unwrap()).unwrap();
            }
        });
        workers.push((send, result, join));
    }
    for (i, (send, result, _)) in workers.iter().enumerate() {
        send.send(true).unwrap();
        let exit = result.recv().unwrap();
        assert_eq!(exit.registers[2], i as u64 + 1);
        assert_eq!(exit.registers[4], TLS + 0x4000 + i as u64 * 0x1000);
    }
    {
        let mut g = process.lock().unwrap();
        let mut tls = [0; 8];
        for i in 0..2 {
            g.read(TLS + 0x4000 + i * 0x1000, &mut tls).unwrap();
            assert_eq!(u64::from_le_bytes(tls), 0x1111 + i);
        }
        g.unmap(BASE + 0x2000, 0x1000).unwrap();
        g.map_zeroed(BASE + 0x2000, 0x1000, 3).unwrap();
        g.write(BASE + 0x2000, &99u64.to_le_bytes()).unwrap();
    }
    drop(process);
    for (i, (send, result, join)) in workers.into_iter().enumerate() {
        send.send(true).unwrap();
        let exit = result.recv().unwrap();
        assert_eq!(exit.registers[2], 99);
        assert_eq!(exit.registers[3], 0x1111 + i as u64);
        send.send(false).unwrap();
        join.join().unwrap();
    }
    let mut cpu = Cpu::new(&[0xd40000e1]).unwrap();
    assert_eq!(cpu.run_until_svc().unwrap().immediate, 7);
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn live_permissions_unmap_and_instruction_cache_are_observed() {
    for unmap in [false, true] {
        let mut boot = image(&[0xf9000001, 0xd4000401, 0xf9000001, 0xd4000401])
            .protected_homebrew()
            .unwrap();
        boot.x0 = BASE + 0x2000;
        let mut cpu = Cpu::from_homebrew(&boot).unwrap();
        cpu.run_until_svc().unwrap();
        let process = cpu.process();
        {
            let mut g = process.lock().unwrap();
            if unmap {
                g.unmap(BASE + 0x2000, 0x1000).unwrap();
            } else {
                g.protect(BASE + 0x2000, 0x1000, 1).unwrap();
            }
        }
        match cpu.run_until_svc() {
            Err(Error::UnexpectedGuestException(esr)) => {
                assert_eq!(esr >> 26, 0x24);
                assert_eq!(esr & 0x3f, if unmap { 7 } else { 15 });
            }
            other => panic!("stale translation survived: {other:?}"),
        }
    }
    {
        let boot = image(&[0xd4000401, 0xd4000421])
            .protected_homebrew()
            .unwrap();
        let mut cpu = Cpu::from_homebrew(&boot).unwrap();
        cpu.run_until_svc().unwrap();
        let p = cpu.process();
        {
            let mut g = p.lock().unwrap();
            g.protect(BASE, 0x1000, 1).unwrap();
        }
        assert!(
            matches!(cpu.run_until_svc(),Err(Error::UnexpectedGuestException(esr)) if esr>>26==0x20)
        );
    }
    {
        let boot = image(&[0xd4000401, 0xd4000421])
            .protected_homebrew()
            .unwrap();
        let mut cpu = Cpu::from_homebrew(&boot).unwrap();
        cpu.run_until_svc().unwrap();
        let p = cpu.process();
        {
            let mut g = p.lock().unwrap();
            g.unmap(BASE, 0x1000).unwrap();
            g.map_zeroed(BASE, 0x1000, 3).unwrap();
            g.write(BASE + 0x84, &0xd4000461u32.to_le_bytes()).unwrap();
            g.protect(BASE, 0x1000, 5).unwrap();
        }
        assert_eq!(cpu.run_until_svc().unwrap().immediate, 0x23);
    }
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn svc_results_and_resumable_quantum_boundaries() {
    use soju_switch_core::hv::{RunBudget, RunOutcome, YieldReason};
    use std::time::Duration;
    {
        let mut cpu = Cpu::new(&[0xd4000401, 0x8b010002, 0xd4000421]).unwrap();
        let p = cpu.process();
        let mut g = p.lock().unwrap();
        assert!(matches!(
            cpu.set_svc_results(&mut g, &[0]),
            Err(Error::InvalidState)
        ));
        assert!(matches!(
            cpu.run_quantum(&mut g, RunBudget::default()).unwrap(),
            RunOutcome::Svc(_)
        ));
        assert!(matches!(
            cpu.set_svc_results(&mut g, &[0; 32]),
            Err(Error::InvalidState)
        ));
        cpu.set_svc_results(&mut g, &[20, 22]).unwrap();
        let RunOutcome::Svc(exit) = cpu.run_quantum(&mut g, RunBudget::default()).unwrap() else {
            panic!("expected SVC")
        };
        assert_eq!(exit.registers[2], 42);
    }
    {
        let mut program = vec![0xd53be042; 64];
        program.push(0xd4000401);
        let mut cpu = Cpu::new(&program).unwrap();
        assert!(matches!(cpu.run_until_svc(), Err(Error::ExitBudget)));
        assert_eq!(cpu.run_until_svc().unwrap().immediate, 0x20);
    }
    {
        let mut cpu = Cpu::new(&[0x14000000]).unwrap();
        let p = cpu.process();
        let mut g = p.lock().unwrap();
        for _ in 0..3 {
            assert!(matches!(
                cpu.run_quantum(
                    &mut g,
                    RunBudget {
                        max_duration: Duration::from_millis(2),
                        max_exits: 64
                    }
                )
                .unwrap(),
                RunOutcome::Yield(YieldReason::Deadline)
            ));
        }
    }
    {
        let mut cpu = Cpu::new(&[0xd53be021, 0xd53be042, 0xd4000401]).unwrap();
        let exit = cpu.run_until_svc().unwrap();
        assert_eq!(exit.timer_reads, 2);
        assert!(exit.registers[2] >= exit.registers[1]);
    }
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn runtime_creates_blocks_joins_and_exits_guest_threads() {
    use soju_switch_core::runtime::{NoIpc, Runtime, Stop};
    use std::time::Duration;
    let boot = image(&[
        0xd2800000, 0x10000261, 0xd2800542, 0xd2c00043, 0x91400863, 0xd2800584, 0xd2800005,
        0xd4000101, 0xaa0103f3, 0xaa0103e0, 0xd4000121, 0xd2c00034, 0x91400a94, 0xb9000293,
        0xaa1403e1, 0xd2800022, 0x92800003, 0xd4000301, 0xf9000e80, 0xd40000e1, 0xaa0003f3,
        0xd2884800, 0xf2a001e0, 0xd4000161, 0xd2c00034, 0x91400a94, 0xf9000693, 0xd53bd064,
        0xf9000a84, 0xd4000141,
    ])
    .protected_homebrew()
    .unwrap();
    let mut runtime = Runtime::new(&boot).unwrap();
    assert_eq!(
        runtime
            .run(&mut NoIpc, Duration::from_secs(2), 100)
            .unwrap(),
        Stop::ProcessExited(0)
    );
    let p = runtime.process();
    let g = p.lock().unwrap();
    let mut values = [0; 32];
    g.read(BASE + 0x2000, &mut values).unwrap();
    assert_eq!(u64::from_le_bytes(values[8..16].try_into().unwrap()), 42);
    assert_eq!(
        u64::from_le_bytes(values[16..24].try_into().unwrap()),
        0x310000000
    );
    assert_eq!(u64::from_le_bytes(values[24..32].try_into().unwrap()), 0);
    assert!(runtime.schedule.contains(&0x101));
    assert!(runtime.schedule.contains(&0x500));
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn runtime_event_wait_timeout_and_signal() {
    use soju_switch_core::kernel::{
        Handle, IpcRequest, Kernel, MAIN_THREAD, TIMED_OUT, ThreadStatus, Wait,
    };
    use soju_switch_core::memory::GuestMemory;
    use soju_switch_core::runtime::{IpcHandler, IpcReply, NoIpc, Runtime, Stop};
    use std::time::Duration;
    struct Signal(Handle);
    impl IpcHandler for Signal {
        fn dispatch(
            &mut self,
            _: &IpcRequest,
            _: &mut Kernel,
            _: &mut dyn GuestMemory,
        ) -> Result<IpcReply, String> {
            Ok(IpcReply::Unhandled)
        }
        fn poll(&mut self, k: &mut Kernel, _: &mut dyn GuestMemory, _: u64) -> Result<(), String> {
            if matches!(
                k.thread(MAIN_THREAD).unwrap().status,
                ThreadStatus::Waiting(Wait::Objects(_))
            ) {
                k.signal_event(self.0).unwrap();
            }
            Ok(())
        }
    }
    for signal in [false, true] {
        let boot = image(&[
            0xd2c00033, 0x91400a73, 0xaa1303e1, 0xd2800022, 0xd2884803, 0xf2a001e3, 0xd4000301,
            0xf9000660, 0xf9000a61, 0xd40000e1,
        ])
        .protected_homebrew()
        .unwrap();
        let mut runtime = Runtime::new(&boot).unwrap();
        let event = runtime.kernel_mut().register_event(false).unwrap();
        let p = runtime.process();
        {
            p.lock()
                .unwrap()
                .write(BASE + 0x2000, &event.to_le_bytes())
                .unwrap();
        }
        let stop = if signal {
            runtime.run(&mut Signal(event), Duration::from_secs(2), 30)
        } else {
            runtime.run(&mut NoIpc, Duration::from_secs(2), 30)
        }
        .unwrap();
        assert_eq!(stop, Stop::ProcessExited(0));
        let mut result = [0; 8];
        p.lock().unwrap().read(BASE + 0x2008, &mut result).unwrap();
        assert_eq!(
            u64::from_le_bytes(result),
            if signal { 0 } else { TIMED_OUT }
        );
    }
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn runtime_ipc_handoff_is_explicit_and_resumable() {
    use soju_switch_core::kernel::{IpcRequest, Kernel};
    use soju_switch_core::memory::GuestMemory;
    use soju_switch_core::runtime::{IpcHandler, IpcReply, NoIpc, Runtime, Stop};
    use std::time::Duration;
    struct Reply;
    impl IpcHandler for Reply {
        fn dispatch(
            &mut self,
            r: &IpcRequest,
            _: &mut Kernel,
            m: &mut dyn GuestMemory,
        ) -> Result<IpcReply, String> {
            assert_eq!(r.service, 0);
            m.write(r.tls, &77u32.to_le_bytes()).unwrap();
            Ok(IpcReply::Complete(vec![0, 99]))
        }
    }
    let boot = image(&[
        0xd2c00033, 0x91400a73, 0xaa1303e1, 0xd40003e1, 0xaa0103e0, 0xd4000421, 0xf9000660,
        0xf9000a61, 0xd40000e1,
    ])
    .protected_homebrew()
    .unwrap();
    let mut runtime = Runtime::new(&boot).unwrap();
    let p = runtime.process();
    p.lock().unwrap().write(BASE + 0x2000, b"sm:\0").unwrap();
    assert!(matches!(
        runtime.run(&mut NoIpc, Duration::from_secs(2), 30).unwrap(),
        Stop::IpcRequired(_)
    ));
    assert_eq!(
        runtime.run(&mut Reply, Duration::from_secs(2), 30).unwrap(),
        Stop::ProcessExited(0)
    );
    let mut value = [0; 8];
    p.lock().unwrap().read(BASE + 0x2010, &mut value).unwrap();
    assert_eq!(u64::from_le_bytes(value), 99);
}

#[test]
#[ignore = "requires local homebrew and entitlement; use scripts/test-switch-hv.sh"]
fn optional_homebrew_runtime_reaches_service_manager() {
    use soju_switch_core::runtime::{NoIpc, Runtime, Stop};
    use std::time::Duration;
    let Some(path) = std::env::var_os("SOJU_SWITCH_TEST_NRO") else {
        return;
    };
    let nro = Nro::read(&mut std::fs::File::open(path).unwrap()).unwrap();
    let mut runtime = Runtime::new(&nro.protected_homebrew().unwrap()).unwrap();
    let stop = runtime
        .run(&mut NoIpc, Duration::from_secs(10), 2000)
        .unwrap();
    println!("Homebrew runtime: {stop:?}");
    assert!(matches!(stop,Stop::IpcRequired(request) if request.service==0));
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn runtime_mutex_condition_and_address_waits_between_vcpus() {
    use soju_switch_core::runtime::{NoIpc, Runtime, Stop};
    use std::time::Duration;
    // Main holds a mutex; child blocks, then waits on a condition and an address.
    // Shared phase flags are stored immediately before each blocking SVC, so the
    // main thread cannot signal early regardless of host scheduling speed.
    let boot = image(&[
        0xd2c00034, 0x91400a94, 0xd2802035, 0x52802026, 0x72a80006, 0xb9002286, 0x100005c1,
        0xd2800002, 0xd2c00043, 0x91400863, 0xd2800584, 0xd2800005, 0xd4000101, 0xb9000281,
        0xaa0103e0, 0xd4000121, 0xb9402a86, 0x710004df, 0x54000080, 0xd2800000, 0xd4000161,
        0x17fffffb, 0x91008280, 0xd4000361, 0xb9402a86, 0x710008df, 0x54000080, 0xd2800000,
        0xd4000161, 0x17fffffb, 0xb9002295, 0x91009280, 0xd2800021, 0xd40003a1, 0x91008280,
        0xd4000361, 0xb9402a86, 0x71000cdf, 0x54000080, 0xd2800000, 0xd4000161, 0x17fffffb,
        0x9100b280, 0xd2800021, 0xd2800002, 0xd2800023, 0xd40006a1, 0xaa1403e1, 0xd2800022,
        0x92800003, 0xd4000301, 0xd40000e1, 0xd2c00034, 0x91400a94, 0x128fffe1, 0xd40004a1,
        0xaa0103f3, 0xd2802020, 0x91008281, 0xaa1303e2, 0x52800026, 0xb9002a86, 0xd4000341,
        0xf9001e80, 0x91008280, 0x91009281, 0xaa1303e2, 0x92800003, 0x52800046, 0xb9002a86,
        0xd4000381, 0xf9002280, 0x52800066, 0xb9002a86, 0x9100b280, 0xd2800041, 0xd2800002,
        0x92800003, 0xd4000681, 0xf9001a80, 0xd4000141,
    ])
    .protected_homebrew()
    .unwrap();
    let mut runtime = Runtime::new(&boot).unwrap();
    assert_eq!(
        runtime
            .run(&mut NoIpc, Duration::from_secs(2), 200)
            .unwrap(),
        Stop::ProcessExited(0)
    );
    let p = runtime.process();
    let g = p.lock().unwrap();
    let mut words = [0; 40];
    g.read(BASE + 0x2020, &mut words).unwrap();
    assert_eq!(u32::from_le_bytes(words[8..12].try_into().unwrap()), 3);
    assert_eq!(u32::from_le_bytes(words[12..16].try_into().unwrap()), 1);
    for offset in [16, 24, 32] {
        assert_eq!(
            u64::from_le_bytes(words[offset..offset + 8].try_into().unwrap()),
            0
        );
    }
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn runtime_system_tick_matches_physical_and_virtual_counter_epoch() {
    use soju_switch_core::runtime::{NoIpc, Runtime, Stop};
    use std::time::Duration;
    let boot = image(&[
        0xd53be033, 0xd40003c1, 0xd53be054, 0xd2c00022, 0x91400842, 0xf9000053, 0xf9000440,
        0xf9000854, 0xd40000e1,
    ])
    .protected_homebrew()
    .unwrap();
    let mut runtime = Runtime::new(&boot).unwrap();
    assert_eq!(
        runtime.run(&mut NoIpc, Duration::from_secs(2), 20).unwrap(),
        Stop::ProcessExited(0)
    );
    let mut bytes = [0; 24];
    runtime
        .process()
        .lock()
        .unwrap()
        .read(BASE + 0x2000, &mut bytes)
        .unwrap();
    let ticks: Vec<_> = bytes
        .chunks_exact(8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        .collect();
    assert!(ticks[0] <= ticks[1] && ticks[1] <= ticks[2], "{ticks:?}");
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn host_instruction_patches_invalidate_without_mapping_changes() {
    for alias in [false, true] {
        let boot = image(&[0xd4000401, 0xd4000421])
            .protected_homebrew()
            .unwrap();
        let mut cpu = Cpu::from_homebrew(&boot).unwrap();
        let p = cpu.process();
        if alias {
            p.lock()
                .unwrap()
                .alias(BASE + 0x10000, BASE, 0x1000, 3)
                .unwrap();
        }
        cpu.run_until_svc().unwrap();
        {
            let mut g = p.lock().unwrap();
            if alias {
                g.write(BASE + 0x10084, &0xd4000461u32.to_le_bytes())
                    .unwrap();
            } else {
                g.memory()
                    .write_kernel(BASE + 0x84, &0xd4000461u32.to_le_bytes())
                    .unwrap();
            }
        }
        assert_eq!(cpu.run_until_svc().unwrap().immediate, 0x23);
    }
}

#[test]
#[ignore = "requires local homebrew and entitlement; use scripts/test-switch-hv.sh"]
fn optional_homebrew_ipc_presents_frame() {
    use soju_switch_core::ipc::router::Hub;
    use soju_switch_core::runtime::{Runtime, Stop};
    use std::time::{Duration, Instant};
    let Some(path) = std::env::var_os("SOJU_SWITCH_TEST_NRO") else {
        return;
    };
    let path = std::path::Path::new(&path);
    let nro = Nro::read(&mut std::fs::File::open(path).unwrap()).unwrap();
    let mut runtime = Runtime::new(&nro.protected_homebrew().unwrap()).unwrap();
    let home = std::env::temp_dir().join(format!("soju-ipc-frame-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let home = home.canonicalize().unwrap();
    let mut hub = Hub::new(path, &home).unwrap();
    runtime.kernel_mut().program_id = hub.program_id();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let stop = runtime
            .run(&mut hub, Duration::from_millis(50), 100_000)
            .unwrap();
        if let Some(frame) = hub.take_frame() {
            assert!(frame.width > 0 && frame.height > 0);
            assert_eq!(
                frame.pixels.len(),
                frame.width as usize * frame.height as usize * 4
            );
            assert!(
                frame
                    .pixels
                    .chunks_exact(4)
                    .any(|pixel| pixel != &frame.pixels[..4])
            );
            println!(
                "Rust IPC presented {}x{} in {:.3}s",
                frame.width,
                frame.height,
                30.0 - deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs_f64()
            );
            hub.finish().unwrap();
            break;
        }
        assert_eq!(stop, Stop::BudgetExpired);
        assert!(
            Instant::now() < deadline,
            "no guest frame within 30 seconds"
        );
    }
    drop(hub);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn virtual_cache_profile_and_checked_maintenance() {
    use soju_switch_core::hv::CACHE_TYPE;
    let mut boot = image(&[
        0xd53b0022, 0xd53b00e3, 0xd50b7a20, 0xd50b7b20, 0xd50b7e20, 0xd50b7520, 0xd50b7421,
        0xd4000401,
    ])
    .protected_homebrew()
    .unwrap();
    boot.x0 = BASE + 0x2001;
    boot.x1 = BASE + 0x2043;
    let mut cpu = Cpu::from_homebrew(&boot).unwrap();
    let p = cpu.process();
    p.lock()
        .unwrap()
        .write(BASE + 0x2000, &[0xa5; 192])
        .unwrap();
    let exit = cpu.run_until_svc().unwrap();
    assert_eq!(exit.immediate, 0x20);
    assert_eq!(exit.registers[2], CACHE_TYPE);
    assert_ne!(exit.registers[3] & 0x10, 0);
    assert_eq!(exit.timer_reads, 0);
    let mut bytes = [0; 192];
    p.lock().unwrap().read(BASE + 0x2000, &mut bytes).unwrap();
    assert_eq!(&bytes[..64], &[0xa5; 64]);
    assert_eq!(&bytes[64..128], &[0; 64]);
    assert_eq!(&bytes[128..], &[0xa5; 64]);
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn guest_cache_operands_cannot_access_private_unmapped_or_readonly_storage() {
    use soju_switch_core::memory;
    for (instruction, address, readonly) in [
        (0xd50b7a20, 0x10000, false),
        (0xd50b7520, 0x11000, false),
        (0xd50b7e20, 0x500000000, false),
        (0xd50b7420, BASE, false),
        (0xd50b7420, BASE + 0x2000, true),
    ] {
        let mut boot = image(&[instruction, 0xd4000401])
            .protected_homebrew()
            .unwrap();
        boot.x0 = address;
        let mut cpu = Cpu::from_homebrew(&boot).unwrap();
        if readonly {
            cpu.process()
                .lock()
                .unwrap()
                .protect(address, 0x1000, 1)
                .unwrap();
        }
        assert!(matches!(
            cpu.run_until_svc(),
            Err(Error::Memory(
                memory::Error::Unmapped | memory::Error::Access
            ))
        ));
    }
    let mut cpu = Cpu::new(&[0xd5380002, 0xd4000401]).unwrap();
    assert!(matches!(
        cpu.run_until_svc(),
        Err(Error::UnexpectedGuestException(_))
    ));
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn guest_cache_maintenance_publishes_guest_written_instructions() {
    use soju_switch_core::hv::{Process, ThreadState};
    let mut program = vec![0xd503201f; 34];
    program[..10].copy_from_slice(&[
        0xd63f0000, 0xd4000401, 0xb9000041, 0xd50b7b22, 0xd5033b9f, 0xd50b7520, 0xd5033b9f,
        0xd5033fdf, 0xd63f0000, 0xd4000421,
    ]);
    program[32] = 0xd28000e5;
    program[33] = 0xd65f03c0;
    let boot = image(&program).protected_homebrew().unwrap();
    let p = Process::new(&boot).unwrap();
    p.lock()
        .unwrap()
        .alias(BASE + 0x10000, BASE, 0x1000, 3)
        .unwrap();
    let mut state = ThreadState::from_homebrew(&boot);
    state.registers[0] = BASE + 0x100;
    state.registers[1] = 0xd2800125;
    state.registers[2] = BASE + 0x10100;
    let mut cpu = Cpu::attach(p, state).unwrap();
    assert_eq!(cpu.run_until_svc().unwrap().registers[5], 7);
    let exit = cpu.run_until_svc().unwrap();
    assert_eq!(exit.immediate, 0x21);
    assert_eq!(exit.registers[5], 9);
}

#[test]
#[ignore = "requires ad-hoc hypervisor entitlement; use scripts/test-switch-hv.sh"]
fn guest_cache_maintenance_reaches_other_vcpu() {
    use soju_switch_core::hv::{Process, ThreadState};
    use std::sync::{Arc, mpsc};
    let mut program = vec![0xd503201f; 34];
    program[..10].copy_from_slice(&[
        0xd63f0000, 0xd4000401, 0xb9000041, 0xd50b7b22, 0xd5033b9f, 0xd50b7520, 0xd5033b9f,
        0xd5033fdf, 0xd63f0000, 0xd4000421,
    ]);
    program[16..20].copy_from_slice(&[0xd63f0000, 0xd4000401, 0xd63f0000, 0xd4000421]);
    program[32] = 0xd28000e5;
    program[33] = 0xd65f03c0;
    let boot = image(&program).protected_homebrew().unwrap();
    let p = Process::new(&boot).unwrap();
    p.lock()
        .unwrap()
        .alias(BASE + 0x10000, BASE, 0x1000, 3)
        .unwrap();
    let mut workers = Vec::new();
    for offset in [0x80, 0xc0] {
        let mut state = ThreadState::from_homebrew(&boot);
        state.pc = BASE + offset;
        state.registers[0] = BASE + 0x100;
        state.registers[1] = 0xd2800125;
        state.registers[2] = BASE + 0x10100;
        let process = Arc::clone(&p);
        let (send, receive) = mpsc::channel();
        let (reply, result) = mpsc::channel();
        let join = std::thread::spawn(move || {
            let mut cpu = Cpu::attach(process, state).unwrap();
            while receive.recv().unwrap_or(false) {
                reply.send(cpu.run_until_svc().unwrap()).unwrap();
            }
        });
        workers.push((send, result, join));
    }
    for (send, result, _) in &workers {
        send.send(true).unwrap();
        assert_eq!(result.recv().unwrap().registers[5], 7);
    }
    // Writer executes DC/IC maintenance first. The reader's cached old function
    // must be invalidated by its own generation pass before it re-enters EL0.
    for (send, result, join) in workers {
        send.send(true).unwrap();
        assert_eq!(result.recv().unwrap().registers[5], 9);
        send.send(false).unwrap();
        join.join().unwrap();
    }
}

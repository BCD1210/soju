use soju_switch_core::kernel::*;
use soju_switch_core::memory::{MemoryImage, PAGE};
use soju_switch_core::nro::{BASE, Homebrew, LOADER_RETURN, Region, STACK, TLS};
fn setup() -> (Kernel, MemoryImage) {
    let boot = Homebrew {
        regions: vec![
            Region {
                address: BASE,
                bytes: vec![0; PAGE],
                permissions: 5,
            },
            Region {
                address: BASE + PAGE as u64,
                bytes: vec![0; PAGE * 2],
                permissions: 3,
            },
            Region {
                address: STACK,
                bytes: vec![0; PAGE * 4],
                permissions: 3,
            },
            Region {
                address: TLS,
                bytes: vec![0; PAGE],
                permissions: 3,
            },
        ],
        pc: BASE,
        sp: STACK + PAGE as u64 * 4,
        x0: 0,
        x1: 0,
        x30: LOADER_RETURN,
        tpidrro_el0: TLS,
    };
    (Kernel::new(&boot), MemoryImage::new(&boot).unwrap())
}
fn svc(
    k: &mut Kernel,
    m: &mut MemoryImage,
    t: ThreadId,
    s: u16,
    args: &[u64],
    now: u64,
) -> Dispatch {
    let mut r = [0; 31];
    r[..args.len()].copy_from_slice(args);
    k.dispatch(m, t, s, &r, BASE + 4, now)
}
fn call(k: &mut Kernel, m: &mut MemoryImage, s: u16, args: &[u64]) -> Vec<u64> {
    match svc(k, m, MAIN_THREAD, s, args, 0) {
        Dispatch::Return(r) => r,
        other => panic!("unexpected {other:?}"),
    }
}
fn create(k: &mut Kernel, m: &mut MemoryImage) -> u32 {
    call(k, m, 8, &[0, BASE, 42, STACK + PAGE as u64 * 2, 44, 0])[1] as u32
}
fn read32(m: &MemoryImage, address: u64) -> u32 {
    let mut b = [0; 4];
    m.read(address, &mut b).unwrap();
    u32::from_le_bytes(b)
}
fn put32(m: &mut MemoryImage, address: u64, value: u32) {
    m.write(address, &value.to_le_bytes()).unwrap();
}
#[test]
fn heap_alias_attributes_and_queries() {
    let (mut k, mut m) = setup();
    assert_eq!(call(&mut k, &mut m, 1, &[0, 3]), [INVALID_SIZE]);
    assert_eq!(call(&mut k, &mut m, 1, &[0, 0x200000]), [0, HEAP]);
    put32(&mut m, HEAP, 123);
    let alias = STACK + 0x100000;
    assert_eq!(call(&mut k, &mut m, 4, &[alias, HEAP, 4096]), [0]);
    assert!(m.read(HEAP, &mut [0; 4]).is_err());
    assert_eq!(read32(&m, alias), 123);
    put32(&mut m, alias, 456);
    assert_eq!(call(&mut k, &mut m, 1, &[0, 0]), [INVALID_MEMORY_STATE]);
    assert_eq!(call(&mut k, &mut m, 5, &[alias, HEAP, 4096]), [0]);
    assert_eq!(read32(&m, HEAP), 456);
    assert_eq!(call(&mut k, &mut m, 3, &[HEAP, 4096, 8, 8]), [0]);
    assert_eq!(call(&mut k, &mut m, 6, &[BASE + 4096, 0, HEAP]), [0, 0]);
    assert_eq!(read32(&m, BASE + 4096 + 20), 8);
    assert_eq!(call(&mut k, &mut m, 3, &[BASE + 8192, 4096, 16, 16]), [0]);
    assert_ne!(call(&mut k, &mut m, 2, &[BASE + 8192, 4096, 1]), [0]);
    assert_eq!(call(&mut k, &mut m, 2, &[BASE + 4096, 4096, 1]), [0]);
    assert!(m.write(BASE + 4096, &[1]).is_err());
    for _ in 0..270 {
        assert_eq!(call(&mut k, &mut m, 1, &[0, 0]), [0, HEAP]);
        assert_eq!(call(&mut k, &mut m, 1, &[0, 0x200000]), [0, HEAP]);
        assert_eq!(read32(&m, HEAP), 0);
        put32(&mut m, HEAP, 99);
    }
}
#[test]
fn thread_lifecycle_priority_affinity_and_round_robin() {
    let (mut k, mut m) = setup();
    let a = create(&mut k, &mut m);
    let b = create(&mut k, &mut m);
    assert_ne!(k.thread(a).unwrap().spec.tls, k.thread(b).unwrap().spec.tls);
    assert_eq!(call(&mut k, &mut m, 9, &[a as u64]), [0]);
    assert_eq!(call(&mut k, &mut m, 9, &[b as u64]), [0]);
    assert_eq!(call(&mut k, &mut m, 9, &[a as u64]), [INVALID_STATE]);
    assert_eq!(k.next_runnable(), Some(MAIN_THREAD));
    k.requeue(MAIN_THREAD);
    assert_eq!(k.next_runnable(), Some(a));
    k.requeue(a);
    assert_eq!(k.next_runnable(), Some(b));
    k.requeue(b);
    assert_eq!(call(&mut k, &mut m, 13, &[b as u64, 10]), [0]);
    assert_eq!(k.next_runnable(), Some(b));
    assert_eq!(call(&mut k, &mut m, 12, &[0, b as u64]), [0, 10]);
    assert_eq!(
        call(&mut k, &mut m, 13, &[b as u64, 64]),
        [INVALID_PRIORITY]
    );
    assert_eq!(
        call(&mut k, &mut m, 15, &[CURRENT_THREAD as u64, 0, 1]),
        [0]
    );
    assert_eq!(
        call(&mut k, &mut m, 14, &[0, 0, CURRENT_THREAD as u64]),
        [0, 0, 1]
    );
    assert_ne!(call(&mut k, &mut m, 15, &[a as u64, 0, 2]), [0]);
    assert_eq!(
        call(&mut k, &mut m, 0x25, &[0, CURRENT_THREAD as u64]),
        [0, MAIN_THREAD as u64]
    );
    assert_eq!(svc(&mut k, &mut m, a, 10, &[], 0), Dispatch::ThreadExited);
    assert_eq!(k.thread(a).unwrap().status, ThreadStatus::Exited);
}
#[test]
fn event_alias_wait_timeout_cancellation_and_thread_join() {
    let (mut k, mut m) = setup();
    let child = create(&mut k, &mut m);
    let event = k.register_event(false).unwrap();
    let alias = k.duplicate_handle(event).unwrap();
    let addr = BASE + 4096;
    put32(&mut m, addr, event);
    assert_eq!(
        svc(&mut k, &mut m, child, 0x18, &[0, addr, 1, u64::MAX], 100),
        Dispatch::Blocked
    );
    k.signal_event(alias).unwrap();
    assert_eq!(k.take_results(child), Some(vec![0, 0]));
    assert_eq!(call(&mut k, &mut m, 0x17, &[alias as u64]), [0]);
    assert_eq!(call(&mut k, &mut m, 0x18, &[0, addr, 1, 0]), [TIMED_OUT]);
    assert_eq!(
        svc(&mut k, &mut m, child, 0x18, &[0, addr, 1, 10], 100),
        Dispatch::Blocked
    );
    k.advance(&mut m, 109).unwrap();
    assert!(k.take_results(child).is_none());
    k.advance(&mut m, 110).unwrap();
    assert_eq!(k.take_results(child), Some(vec![TIMED_OUT]));
    assert_eq!(
        svc(&mut k, &mut m, child, 0x18, &[0, addr, 1, u64::MAX], 120),
        Dispatch::Blocked
    );
    assert_eq!(call(&mut k, &mut m, 0x19, &[child as u64]), [0]);
    assert_eq!(k.take_results(child), Some(vec![CANCELLED]));
    put32(&mut m, addr, child);
    assert_eq!(
        svc(
            &mut k,
            &mut m,
            MAIN_THREAD,
            0x18,
            &[0, addr, 1, u64::MAX],
            0
        ),
        Dispatch::Blocked
    );
    assert_eq!(
        svc(&mut k, &mut m, child, 10, &[], 0),
        Dispatch::ThreadExited
    );
    assert_eq!(k.take_results(MAIN_THREAD), Some(vec![0, 0]));
    assert_eq!(call(&mut k, &mut m, 0x16, &[event as u64]), [0]);
    k.signal_event(alias).unwrap();
    assert_eq!(call(&mut k, &mut m, 0x16, &[alias as u64]), [0]);
    assert_eq!(k.signal_event(alias), Err(INVALID_HANDLE));
}
#[test]
fn mutex_condition_signal_and_timeout_reacquire_before_return() {
    let (mut k, mut m) = setup();
    let child = create(&mut k, &mut m);
    let mutex = BASE + 4096;
    let key = mutex + 4;
    put32(&mut m, mutex, MAIN_THREAD | 0x40000000);
    assert_eq!(
        svc(
            &mut k,
            &mut m,
            child,
            0x1a,
            &[MAIN_THREAD as u64, mutex, child as u64],
            0
        ),
        Dispatch::Blocked
    );
    assert_eq!(call(&mut k, &mut m, 0x1b, &[mutex]), [0]);
    assert_eq!(read32(&m, mutex), child);
    assert_eq!(k.take_results(child), Some(vec![0]));
    assert_eq!(
        svc(
            &mut k,
            &mut m,
            child,
            0x1c,
            &[mutex, key, child as u64, 50],
            100
        ),
        Dispatch::Blocked
    );
    assert_eq!(read32(&m, mutex), 0);
    put32(&mut m, mutex, MAIN_THREAD);
    assert_eq!(call(&mut k, &mut m, 0x1d, &[key, 1]), [0]);
    assert!(matches!(
        k.thread(child).unwrap().status,
        ThreadStatus::Waiting(Wait::Mutex { .. })
    ));
    assert_eq!(call(&mut k, &mut m, 0x1b, &[mutex]), [0]);
    assert_eq!(read32(&m, mutex), child);
    assert_eq!(k.take_results(child), Some(vec![0]));
    assert_eq!(
        svc(
            &mut k,
            &mut m,
            child,
            0x1c,
            &[mutex, key, child as u64, 50],
            200
        ),
        Dispatch::Blocked
    );
    put32(&mut m, mutex, MAIN_THREAD);
    k.advance(&mut m, 250).unwrap();
    assert!(k.take_results(child).is_none());
    assert!(matches!(
        k.thread(child).unwrap().status,
        ThreadStatus::Waiting(Wait::Mutex {
            result: TIMED_OUT,
            ..
        })
    ));
    assert_eq!(call(&mut k, &mut m, 0x1b, &[mutex]), [0]);
    assert_eq!(read32(&m, mutex), child);
    assert_eq!(k.take_results(child), Some(vec![TIMED_OUT]));
}
#[test]
fn address_arbitration_modes_counts_and_invalid_comparison() {
    let (mut k, mut m) = setup();
    let a = create(&mut k, &mut m);
    let b = create(&mut k, &mut m);
    let address = BASE + 4096;
    put32(&mut m, address, 3);
    assert_eq!(
        svc(&mut k, &mut m, a, 0x34, &[address, 0, 2, 100], 0),
        Dispatch::Return(vec![INVALID_STATE])
    );
    assert_eq!(
        svc(&mut k, &mut m, a, 0x34, &[address, 1, 4, 100], 0),
        Dispatch::Blocked
    );
    assert_eq!(read32(&m, address), 2);
    assert_eq!(
        svc(&mut k, &mut m, b, 0x34, &[address, 2, 2, 100], 0),
        Dispatch::Blocked
    );
    assert_eq!(
        call(&mut k, &mut m, 0x35, &[address, 1, 3, 1]),
        [INVALID_STATE]
    );
    assert_eq!(call(&mut k, &mut m, 0x35, &[address, 1, 2, 1]), [0]);
    assert_eq!(read32(&m, address), 3);
    assert_eq!(k.take_results(a), Some(vec![0]));
    assert!(k.take_results(b).is_none());
    assert_eq!(
        call(&mut k, &mut m, 0x35, &[address, 2, 3, u32::MAX as u64]),
        [0]
    );
    assert_eq!(read32(&m, address), 2);
    assert_eq!(k.take_results(b), Some(vec![0]));
    assert_eq!(
        svc(&mut k, &mut m, a, 0x34, &[address, 2, 2, 10], 100),
        Dispatch::Blocked
    );
    k.advance(&mut m, 110).unwrap();
    assert_eq!(k.take_results(a), Some(vec![TIMED_OUT]));
    assert_eq!(call(&mut k, &mut m, 0x35, &[address, 2, 2, 1]), [0]);
    assert_eq!(read32(&m, address), 3);
    assert_eq!(
        call(&mut k, &mut m, 0x34, &[address + 1, 0, 4, 0]),
        [INVALID_ADDRESS]
    );
    assert_eq!(
        call(&mut k, &mut m, 0x34, &[address, 9, 4, 0]),
        [INVALID_ENUM]
    );
}
#[test]
fn shared_transfer_sessions_and_retained_object_lifetimes() {
    let (mut k, mut m) = setup();
    let shared = k.register_shared(&mut m, &vec![7; PAGE], 1).unwrap();
    let a = 0x500000000;
    let b = a + 4096;
    assert_eq!(
        call(&mut k, &mut m, 0x13, &[shared as u64, a, 4096, 1]),
        [0]
    );
    assert_eq!(
        call(&mut k, &mut m, 0x13, &[shared as u64, b, 4096, 1]),
        [0]
    );
    k.write_shared(&mut m, shared, 0, &123u32.to_le_bytes())
        .unwrap();
    assert_eq!(read32(&m, a), 123);
    assert_eq!(read32(&m, b), 123);
    assert!(m.write(a, &[1]).is_err());
    assert_eq!(call(&mut k, &mut m, 0x14, &[shared as u64, a, 4096]), [0]);
    assert_eq!(call(&mut k, &mut m, 0x14, &[shared as u64, b, 4096]), [0]);
    assert_eq!(call(&mut k, &mut m, 0x16, &[shared as u64]), [0]);
    call(&mut k, &mut m, 1, &[0, 0x200000]);
    let transfer = call(&mut k, &mut m, 0x15, &[0, HEAP, 4096, 0])[1] as u32;
    let object = k.retain_object(transfer).unwrap();
    assert_eq!(k.transfer(object), Some((HEAP, 4096)));
    assert_eq!(call(&mut k, &mut m, 0x16, &[transfer as u64]), [0]);
    assert!(m.write(HEAP, &[1]).is_err());
    k.release_object(&mut m, object).unwrap();
    m.write(HEAP, &[1]).unwrap();
    m.write(BASE + 4096, b"sm:\0").unwrap();
    let session = call(&mut k, &mut m, 0x1f, &[0, BASE + 4096])[1] as u32;
    let request = match svc(&mut k, &mut m, MAIN_THREAD, 0x21, &[session as u64], 0) {
        Dispatch::Ipc(request) => request,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        (request.session, request.service, request.tls),
        (session, 0, TLS)
    );
    k.complete_ipc(MAIN_THREAD, vec![0, 77]).unwrap();
    assert_eq!(k.take_results(MAIN_THREAD), Some(vec![0, 77]));
    let copy = k.duplicate_handle(session).unwrap();
    call(&mut k, &mut m, 0x16, &[session as u64]);
    assert!(k.take_closed_sessions().is_empty());
    call(&mut k, &mut m, 0x16, &[copy as u64]);
    assert_eq!(k.take_closed_sessions(), [request.object]);
}
#[test]
fn diagnostic_info_timer_loader_and_process_exit() {
    let (mut k, mut m) = setup();
    m.write(BASE + 4096, b"hello\0world").unwrap();
    assert_eq!(call(&mut k, &mut m, 0x27, &[BASE + 4096, 11]), [0]);
    assert_eq!(k.debug_output(), [b"hello\0world".to_vec()]);
    assert_eq!(call(&mut k, &mut m, 0x29, &[0, 4, 0, 0]), [0, HEAP]);
    assert_eq!(call(&mut k, &mut m, 0x29, &[0, 19, 0, 0]), [INVALID_ENUM]);
    assert_eq!(
        svc(&mut k, &mut m, MAIN_THREAD, 0x1e, &[], 1_000_000_000),
        Dispatch::Return(vec![19_200_000])
    );
    assert_eq!(call(&mut k, &mut m, 0x26, &[0x80000000]), [0]);
    assert_eq!(
        svc(&mut k, &mut m, MAIN_THREAD, 0x26, &[3, BASE, 4], 0),
        Dispatch::Break {
            reason: 3,
            address: BASE,
            size: 4
        }
    );
    let mut regs = [0; 31];
    regs[0] = 42;
    assert_eq!(
        k.dispatch(&mut m, MAIN_THREAD, 0x7e, &regs, LOADER_RETURN + 4, 0),
        Dispatch::ProcessExited(42)
    );
    assert_eq!(
        k.dispatch(&mut m, MAIN_THREAD, 0x7e, &regs, BASE + 4, 0),
        Dispatch::Unsupported(0x7e)
    );
    assert_eq!(
        svc(&mut k, &mut m, MAIN_THREAD, 7, &[], 0),
        Dispatch::ProcessExited(0)
    );
}

#[test]
fn cancellation_latches_transfer_aliases_reject_and_resources_bound() {
    let (mut k, mut m) = setup();
    let child = create(&mut k, &mut m);
    let event = k.register_event(false).unwrap();
    let addr = BASE + 4096;
    put32(&mut m, addr, event);
    assert_eq!(call(&mut k, &mut m, 0x19, &[child as u64]), [0]);
    assert_eq!(
        svc(&mut k, &mut m, child, 0x18, &[0, addr, 1, u64::MAX], 0),
        Dispatch::Return(vec![CANCELLED])
    );
    assert_eq!(
        svc(&mut k, &mut m, child, 0x18, &[0, addr, 1, u64::MAX], 0),
        Dispatch::Blocked
    );
    call(&mut k, &mut m, 1, &[0, 0x200000]);
    let transfer = call(&mut k, &mut m, 0x15, &[0, HEAP, 4096, 3])[1];
    assert_eq!(
        call(&mut k, &mut m, 4, &[STACK + 0x100000, HEAP, 4096]),
        [INVALID_MEMORY_STATE]
    );
    call(&mut k, &mut m, 0x16, &[transfer]);
    assert_eq!(
        call(&mut k, &mut m, 4, &[STACK + 0x100000, HEAP, 4096]),
        [0]
    );
    for _ in 0..1500 {
        assert_eq!(call(&mut k, &mut m, 0x27, &[u64::MAX, 0]), [0]);
        call(&mut k, &mut m, 0x27, &[addr, 1]);
    }
    assert_eq!(k.debug_output().len(), 1024);
    assert_eq!(k.take_debug_output().len(), 1024);
    assert!(k.debug_output().is_empty());
    let mut count = 0;
    while k.register_event(false).is_ok() {
        count += 1;
        assert!(count < 4096);
    }
    assert!(count > 4000);
    assert_eq!(k.register_session(1), Err(LIMIT_REACHED));
}

#[test]
fn exited_threads_release_tls_after_last_handle_and_boot_queries_are_typed() {
    let (mut k, mut m) = setup();
    assert_eq!(m.query(BASE).unwrap().2.unwrap().state, 4);
    assert_eq!(m.query(STACK).unwrap().2.unwrap().state, 0xb);
    assert_eq!(m.query(TLS).unwrap().2.unwrap().state, 0xc);
    for _ in 0..40 {
        let id = create(&mut k, &mut m);
        let tls = k.thread(id).unwrap().spec.tls;
        assert_eq!(tls, 0x310000000);
        assert_eq!(svc(&mut k, &mut m, id, 10, &[], 0), Dispatch::ThreadExited);
        k.close_handle(&mut m, id).unwrap();
        assert!(k.thread(id).is_none());
        assert!(m.read(tls, &mut [0; 4]).is_err());
    }
}

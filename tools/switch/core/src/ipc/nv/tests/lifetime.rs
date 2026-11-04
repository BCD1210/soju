use super::*;
use crate::kernel::{Dispatch, HEAP, MAIN_THREAD};
#[test]
fn transfer_memory_remains_retained_until_last_session_object_closes() {
    let mut f = Fixture::new();
    f.memory.unmap(HEAP, PAGE * 64).unwrap();
    let mut registers = [0; 31];
    registers[1] = 0x200000;
    assert_eq!(
        f.kernel
            .dispatch(&mut f.memory, MAIN_THREAD, 1, &registers, BASE, 0),
        Dispatch::Return(vec![0, HEAP])
    );
    registers[1] = HEAP;
    registers[2] = 4096;
    registers[3] = 0;
    let handle = match f
        .kernel
        .dispatch(&mut f.memory, MAIN_THREAD, 21, &registers, BASE, 0)
    {
        Dispatch::Return(values) => {
            assert_eq!(values[0], 0);
            u32::try_from(values[1]).unwrap()
        }
        other => panic!("unexpected {other:?}"),
    };
    let id = f.kernel.object_id(handle).unwrap();
    let mut request = Fixture::request(3, 4096u32.to_le_bytes().to_vec());
    request.copy_handles = vec![0xffff8001, handle];
    f.call(&request).unwrap();
    assert!(f.call(&request).is_err());
    f.kernel.close_handle(&mut f.memory, handle).unwrap();
    assert_eq!(f.kernel.transfer(id), Some((HEAP, 4096)));
    assert!(f.memory.check_access(HEAP, 4096, 3).is_err());
    let clone = f.kernel.duplicate_handle(f.session).unwrap();
    let object = f.kernel.object_id(f.session).unwrap();
    f.kernel.close_handle(&mut f.memory, f.session).unwrap();
    assert!(f.kernel.take_closed_sessions().is_empty());
    f.kernel.close_handle(&mut f.memory, clone).unwrap();
    assert_eq!(f.kernel.take_closed_sessions(), [object]);
    f.nv.close_object(object);
    f.nv.poll(&mut f.kernel, &mut f.memory, 0).unwrap();
    assert_eq!(f.kernel.transfer(id), None);
    assert!(f.memory.check_access(HEAP, 4096, 3).is_ok());
}
#[test]
fn closed_address_space_keeps_channel_mapping_without_fd_identity_collision() {
    let mut f = Fixture::new();
    let (channel, space, va) = f.setup();
    f.call(&Fixture::request(2, space.to_le_bytes().to_vec()))
        .unwrap();
    let new_space = f.open("/dev/nvhost-as-gpu");
    assert_ne!(space, new_space);
    let mut d = vec![0; 40];
    put32(&mut d, 0, 1);
    put32(&mut d, 8, 65536);
    f.ioctl(new_space, 0x40284109, &d, 0).unwrap();
    assert_eq!(f.nv.channels[&channel].space, Some(space));
    assert_eq!(
        f.nv.spaces[&space].resolve(va, 4).unwrap().0,
        crate::kernel::HEAP
    );
}

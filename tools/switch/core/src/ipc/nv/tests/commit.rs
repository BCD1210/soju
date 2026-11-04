use super::*;
#[test]
fn staged_copy_reads_prior_writes_and_rejects_backing_alias_overlap() {
    let mut f = Fixture::new();
    let (fd, space, va) = f.setup();
    let id = f.nv.spaces[&space].mappings[&va].view.allocation;
    let mut d = vec![0; 40];
    put32(&mut d, 4, 0xfe);
    put32(&mut d, 8, id);
    put32(&mut d, 12, 65536);
    let tiled = read_u64(&f.ioctl(space, 0xc0284106, &d, 40).unwrap(), 32).unwrap();
    let access = submit::Access {
        space: &f.nv.spaces[&space],
        memory: &f.memory,
    };
    let mut state = gpu::ExecutionState::default();
    state.pending_actions = vec![
        gpu::Action::Write {
            address: va + 4096,
            bytes: vec![9, 8, 7, 6],
            kind: 0,
        },
        gpu::Action::Copy {
            chunks: vec![(va + 4096, tiled + 8192, 4)],
        },
        gpu::Action::Increment {
            syncpoint: fd,
            increment: 1,
            wait: false,
        },
    ];
    let writes = super::super::commit::prepare(&state, fd, &access, &[]).unwrap();
    assert_eq!(writes[1].address, crate::kernel::HEAP + 8192);
    assert_eq!(writes[1].bytes, [9, 8, 7, 6]);
    let mut original = [0; 4];
    f.memory
        .read(crate::kernel::HEAP + 8192, &mut original)
        .unwrap();
    assert_eq!(original, [0; 4]);
    state.pending_actions[1] = gpu::Action::Copy {
        chunks: vec![(va + 4096, tiled + 4096, 4)],
    };
    assert!(super::super::commit::prepare(&state, fd, &access, &[]).is_err());
    state.pending_actions[1] = gpu::Action::Copy {
        chunks: vec![(va + 4096, tiled + 8192, 4)],
    };
    assert!(
        super::super::commit::prepare(&state, fd, &access, &[(crate::kernel::HEAP + 8192, 4)])
            .is_err()
    );
}

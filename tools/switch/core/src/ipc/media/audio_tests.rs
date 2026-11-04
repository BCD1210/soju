use super::*;
#[test]
fn timed_audio_pause_resume_contains_release_and_kernel_event_aliases() {
    let mut f = Fixture::new();
    let output = f.audio();
    let event = f.call(output, &req(4, vec![])).unwrap().copy_handles[0];
    let alias = f.kernel.duplicate_handle(event).unwrap();
    f.append(output, 10, 192).unwrap();
    f.append(output, 11, 192).unwrap();
    assert!(f.media.take_audio().is_none());
    f.poll(10_000_000);
    assert_eq!(
        f.call(output, &req(6, 10u64.to_le_bytes().to_vec()))
            .unwrap()
            .data,
        vec![1]
    );
    f.call(output, &req(1, vec![])).unwrap();
    assert_eq!(f.media.take_audio().unwrap().pcm.len(), 192);
    f.poll(10_500_000);
    f.call(output, &req(2, vec![])).unwrap();
    f.poll(20_000_000);
    assert_eq!(f.reset_signal(alias), INVALID_STATE);
    f.call(output, &req(1, vec![])).unwrap();
    f.poll(20_499_999);
    assert_eq!(f.reset_signal(alias), INVALID_STATE);
    f.poll(20_500_000);
    assert_eq!(f.reset_signal(alias), 0);
    assert_eq!(f.reset_signal(event), INVALID_STATE);
    assert_eq!(
        f.call(output, &req(6, 10u64.to_le_bytes().to_vec()))
            .unwrap()
            .data,
        vec![0]
    );
    let mut released = req(5, vec![]);
    released.buffers = io_buffers(40);
    let reply = f.call(output, &released).unwrap();
    assert_eq!(reply.data, words(&[1]));
    assert_eq!(reply.writes[0].bytes, 10u64.to_le_bytes());
    f.poll(21_500_000);
    assert_eq!(
        f.call(output, &released).unwrap().writes[0].bytes,
        11u64.to_le_bytes()
    );
}
#[test]
fn audio_backpressure_and_invalid_linked_lists_are_atomic() {
    let mut f = Fixture::new();
    let output = f.audio();
    let mut request = req(3, 99u64.to_le_bytes().to_vec());
    request.buffers = io_buffers(40);
    let raw: Vec<u8> = [INPUT, PIXELS, 4, 4, 0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    f.memory.write(INPUT, &raw).unwrap();
    assert!(f.call(output, &request).is_err());
    assert_eq!(
        f.call(output, &req(6, 99u64.to_le_bytes().to_vec()))
            .unwrap()
            .data,
        vec![0]
    );
    for tag in 0..64 {
        f.append(output, tag, 4).unwrap();
    }
    assert!(f.append(output, 65, 4).is_err());
}

#[test]
fn completed_audio_survives_host_slice_and_is_delivered_once() {
    let mut f = Fixture::new();
    let output = f.audio();
    f.memory.write(PIXELS, &[1, 2, 3, 4]).unwrap();
    f.append(output, 12, 4).unwrap();
    f.call(output, &req(1, vec![])).unwrap();
    f.poll(16_000_000);
    assert_eq!(f.media.take_audio().unwrap().pcm, vec![1, 2, 3, 4]);
    assert!(f.media.take_audio().is_none());
    assert_eq!(
        f.call(output, &req(6, 12u64.to_le_bytes().to_vec()))
            .unwrap()
            .data,
        vec![0]
    );
}

#[test]
fn linked_audio_buffers_are_joined_in_order_and_invalid_layouts_rejected() {
    let mut f = Fixture::new();
    let output = f.audio();
    f.memory.write(PIXELS, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    let mut descriptor: Vec<u8> = [INPUT + 40, PIXELS, 4, 4, 0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    descriptor.extend(
        [0, PIXELS + 4, 4, 4, 0]
            .iter()
            .flat_map(|v| v.to_le_bytes()),
    );
    f.memory.write(INPUT, &descriptor).unwrap();
    let mut request = req(3, 55u64.to_le_bytes().to_vec());
    request.buffers = io_buffers(40);
    f.call(output, &request).unwrap();
    f.call(output, &req(1, vec![])).unwrap();
    assert_eq!(
        f.media.take_audio().unwrap().pcm,
        vec![1, 2, 3, 4, 5, 6, 7, 8]
    );
    for raw in [
        [0, PIXELS, 4, 3, 0],
        [0, PIXELS + 1, 4, 8, 0],
        [0, PIXELS, 4, 4, 1],
        [u64::MAX, PIXELS, 4, 4, 0],
    ] {
        f.memory
            .write(
                INPUT,
                &raw.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>(),
            )
            .unwrap();
        request.data = 56u64.to_le_bytes().to_vec();
        assert!(f.call(output, &request).is_err());
        assert_eq!(
            f.call(output, &req(6, 56u64.to_le_bytes().to_vec()))
                .unwrap()
                .data,
            vec![0]
        );
    }
}

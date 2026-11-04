use super::*;
fn capture(f: &mut Fixture) -> Handle {
    let service = f.open("audin:u");
    let mut r = req(1, words(&[48000, 2, 0, 0]));
    r.pid = Some(1);
    r.copy_handles = vec![0xffff8001];
    r.buffers = io_buffers(256);
    f.call(service, &r).unwrap().objects[0]
}
#[test]
fn silent_capture_waits_for_deadline_and_releases_each_tag_once() {
    let mut f = Fixture::new();
    let input = capture(&mut f);
    let event = f.call(input, &req(4, vec![])).unwrap().copy_handles[0];
    f.memory.write(PIXELS, &[123; 192]).unwrap();
    f.append(input, 42, 192).unwrap();
    assert!(f.append(input, 42, 192).is_err());
    f.call(input, &req(1, vec![])).unwrap();
    f.poll(500_000);
    f.call(input, &req(2, vec![])).unwrap();
    f.poll(10_000_000);
    let mut bytes = [0; 192];
    f.memory.read(PIXELS, &mut bytes).unwrap();
    assert_eq!(bytes, [123; 192]);
    assert_eq!(f.reset_signal(event), INVALID_STATE);
    f.call(input, &req(1, vec![])).unwrap();
    f.poll(10_499_999);
    assert_eq!(f.reset_signal(event), INVALID_STATE);
    f.poll(10_500_000);
    f.memory.read(PIXELS, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 192]);
    assert_eq!(f.reset_signal(event), 0);
    assert_eq!(
        f.call(input, &req(6, 42u64.to_le_bytes().to_vec()))
            .unwrap()
            .data,
        [0]
    );
    let mut release = req(5, vec![]);
    release.buffers = io_buffers(8);
    assert_eq!(
        f.call(input, &release).unwrap().writes[0].bytes,
        42u64.to_le_bytes()
    );
    assert_eq!(f.call(input, &release).unwrap().data, words(&[0]));
}
#[test]
fn capture_validates_all_bytes_before_queueing_or_touching_memory() {
    let mut f = Fixture::new();
    let input = capture(&mut f);
    f.memory.write(PIXELS, &[9; 192]).unwrap();
    let mut append = req(3, 55u64.to_le_bytes().to_vec());
    append.buffers = io_buffers(40);
    for raw in [
        [0, PIXELS, 191, 192, 0],
        [0, PIXELS, 192, 191, 0],
        [INPUT, PIXELS, 192, 192, 0],
    ] {
        f.memory
            .write(
                INPUT,
                &raw.into_iter()
                    .flat_map(u64::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert!(f.call(input, &append).is_err());
        assert_eq!(f.call(input, &req(11, vec![])).unwrap().data, words(&[0]));
    }
    let mut bytes = [0; 192];
    f.memory.read(PIXELS, &mut bytes).unwrap();
    assert_eq!(bytes, [9; 192]);
}

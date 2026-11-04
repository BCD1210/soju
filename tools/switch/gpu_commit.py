"""Commit the bounded CPU-only subset of a virtual GPU submission.

This profile has no shader or tiled cache and no asynchronous GPU work. It
accepts only the observed single-increment submission contract. It cannot
complete a draw, shader, macro outside the interpreter subset, or GPU fence
wait. Unsupported requests fail without advancing either syncpoint counter.
"""


def commit_cpu_submission(channel, staged, flags, input_fence_id, increments,
                          read_gpu, write_gpu, check_gpu_write, publish_fence=None, restore_fence=None,
                          check_gpu_tiled_write=None, check_gpu_read=None, resolve_backing=None):
    if ((publish_fence is None) != (restore_fence is None)):
        raise ValueError('GPU fence publisher must provide rollback')
    if (channel.faulted or channel.syncpoint is None or flags != 0x104 or input_fence_id != 0
            or increments != 1 or channel.submitted != channel.completed
            or channel.submitted >= 0xffffffff or staged.inflight_work or staged.inline_uploads):
        raise ValueError('unsupported CPU-only GPU submission contract')
    allowed_barriers = {'wait_idle', 'invalidate_constant_cache',
                        'flush_pending_writes', 'flush_tiled_cache', 'memory_barrier',
                        'invalidate_shader_caches'}
    writes = []
    seen_increment = False
    for action in staged.pending_actions:
        if not action:
            raise ValueError('invalid GPU action')
        if action[0] in ('write', 'tiled_write'):
            if len(action) != 3 or seen_increment:
                raise ValueError('unsupported GPU write ordering')
            address, data = action[1:]
            validate = check_gpu_write if action[0] == 'write' else check_gpu_tiled_write
            if not isinstance(data, bytes) or not data or validate is None or not validate(address, len(data)):
                raise ValueError('GPU write target is unavailable')
            original = bytes(read_gpu(address, len(data)))
            if len(original) != len(data):
                raise ValueError('GPU write backup is incomplete')
            writes.append((address, data, original, None))
        elif action[0] == 'pitch_to_tiled':
            if (len(action) != 2 or seen_increment or check_gpu_read is None
                    or check_gpu_tiled_write is None or resolve_backing is None):
                raise ValueError('unsupported GPU copy contract')
            copies = action[1]
            if not copies or len(copies) > 65536 or sum(size for _, _, size in copies) > 8 * 1024 * 1024:
                raise ValueError('invalid GPU copy size')
            source_ranges = []
            for source, destination, size in copies:
                if (size <= 0 or not check_gpu_read(source, size)
                        or not check_gpu_tiled_write(destination, size)):
                    raise ValueError('GPU copy range unavailable')
                source_ranges.append((resolve_backing(source, size), size))
            source_min = min(address for address, _ in source_ranges)
            source_max = max(address + size for address, size in source_ranges)
            for source, destination, size in copies:
                destination_backing = resolve_backing(destination, size)
                if destination_backing < source_max and source_min < destination_backing + size:
                    raise ValueError('overlapping GPU copy is unsupported')
                original = bytes(read_gpu(destination, size))
                if len(original) != size:
                    raise ValueError('GPU copy backup is incomplete')
                writes.append((destination, None, original, source))
        elif action[0] == 'increment_syncpoint':
            if (len(action) != 4 or seen_increment or action[1] != channel.syncpoint
                    or action[2] != 1 or action[3] is not False):
                raise ValueError('unsupported GPU syncpoint operation')
            seen_increment = True
        elif action[0] in allowed_barriers:
            if len(action) != 2:
                raise ValueError('invalid GPU barrier action')
        else:
            raise ValueError('unsupported GPU commit action')
    if not seen_increment:
        raise ValueError('GPU submission did not execute its requested increment')
    target = channel.submitted + 1
    applied = []
    publishing = False
    try:
        for address, data, original, source in writes:
            applied.append((address, original))
            if source is not None:
                data = bytes(read_gpu(source, len(original)))
                if len(data) != len(original):
                    raise ValueError('GPU copy source is truncated')
            write_gpu(address, data)
        if publish_fence is not None:
            publishing = True
            publish_fence(channel.syncpoint, target)
    except Exception as original_error:
        rollback_failed = False
        if publishing and restore_fence is not None:
            try:
                restore_fence()
            except Exception:
                rollback_failed = True
        for address, original in reversed(applied):
            try:
                write_gpu(address, original)
            except Exception:
                rollback_failed = True
        if rollback_failed:
            channel.faulted = True
            raise RuntimeError('GPU rollback failed; channel faulted') from original_error
        raise
    channel.submitted = target
    channel.completed = target
    staged.pending_actions.clear()
    staged.pending_writes.clear()
    return channel.syncpoint, target

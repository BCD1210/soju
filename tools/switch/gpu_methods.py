"""Conservative Maxwell method classification; this module executes nothing."""

from gpu_mme import MacroUpload


def inspect_macro_uploads(decoded):
    """Stage 3D MME uploads for analysis without completing the submission."""
    bindings = {}
    staged = MacroUpload()
    for _, _, _, methods in decoded:
        for subchannel, method, value in methods:
            if method == 0:
                bindings[subchannel] = value
            elif bindings.get(subchannel) == 0xb197 and method in (0x114, 0x118, 0x11c, 0x120):
                staged.write(method, value)
    return staged

# Method offsets are byte addresses. Only known side effects are classified;
# every other method remains unknown until its class-specific semantics exist.
KNOWN_EFFECTS = {
    (0xa140, 0x1b0): 'inline_memory_launch',
    (0xa140, 0x1b4): 'inline_memory_data',
    (0xb0b5, 0x300): 'copy_launch',
    (0x902d, 0x8dc): '2d_blit_launch',
    (0xb197, 0x118): 'mme_instruction_upload',
    (0xb197, 0x120): 'mme_entry_upload',
    (0xb197, 0x2c8): '3d_syncpoint_increment',
}
FIFO_EFFECTS = {0x1c: 'semaphore_operation', 0x20: 'interrupt',
                0x74: 'fifo_syncpoint_operation', 0x78: 'wait_for_idle'}


def inspect_effects(decoded):
    """Follow subchannel class bindings and report known side effects.

    The returned analysis is diagnostic only: it does not change channel state,
    write memory, complete fences, or treat unknown methods as harmless.
    """
    bindings = {}
    effects = []
    for address, _, _, methods in decoded:
        for subchannel, method, value in methods:
            if method == 0:
                bindings[subchannel] = value
                continue
            effect = FIFO_EFFECTS.get(method) if method < 0x100 else KNOWN_EFFECTS.get((bindings.get(subchannel), method))
            if effect is not None:
                effects.append((address, subchannel, method, value, effect))
    return bindings, effects

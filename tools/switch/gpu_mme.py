"""Bounded MME upload storage for the Soju virtual GPU.

The limits here are virtual-profile limits, not claimed GM20B RAM sizes.
Instructions and entry points remain opaque until an MME interpreter executes
them. Uploading them never executes a macro or completes a GPU submission.
"""

INSTRUCTION_WORD_LIMIT = 16384
ENTRY_LIMIT = 128


class MacroUpload:
    def __init__(self):
        self.instruction_pointer = 0
        self.entry_pointer = 0
        self.instructions = {}
        self.entries = {}

    def write(self, method, value):
        """Apply one MME upload method; reject unsupported values and overflow."""
        if not isinstance(value, int) or not 0 <= value <= 0xffffffff:
            raise ValueError('invalid MME word')
        if method == 0x114:
            if value >= INSTRUCTION_WORD_LIMIT:
                raise ValueError('MME instruction pointer out of bounds')
            self.instruction_pointer = value
        elif method == 0x118:
            if self.instruction_pointer >= INSTRUCTION_WORD_LIMIT:
                raise ValueError('MME instruction upload exceeds limit')
            self.instructions[self.instruction_pointer] = value
            self.instruction_pointer += 1
        elif method == 0x11c:
            if value >= ENTRY_LIMIT:
                raise ValueError('MME entry pointer out of bounds')
            self.entry_pointer = value
        elif method == 0x120:
            if self.entry_pointer >= ENTRY_LIMIT:
                raise ValueError('MME entry upload exceeds limit')
            self.entries[self.entry_pointer] = value
            self.entry_pointer += 1
        else:
            raise ValueError('unsupported MME upload method')

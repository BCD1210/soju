"""Bounded interpreter for a deliberately limited Maxwell MME subset.

Unsupported operations, unknown state reads and complex delay-slot combinations
raise ValueError. Emitted GPU methods are returned to the caller for validation;
this function never modifies GPU or host memory.
"""

MAX_INSTRUCTIONS = 4096
MAX_EMITTED = 1024
MASK32 = 0xffffffff


def run_macro(memory, entry, argument, read_state, parameters=()):
    if entry not in memory.entries or not 0 <= argument <= MASK32:
        raise ValueError('unknown MME entry or invalid argument')
    parameters = tuple(parameters)
    if len(parameters) > 256 or any(not isinstance(p, int) or not 0 <= p <= MASK32 for p in parameters):
        raise ValueError('invalid MME parameters')
    parameter_index = 0
    pc = memory.entries[entry]
    registers = [0] * 8
    registers[1] = argument
    method = None
    method_increment = 0
    emitted = []
    branch_target = None
    finish_after_delay = False
    for _ in range(MAX_INSTRUCTIONS):
        if pc not in memory.instructions:
            raise ValueError(f'MME PC not initialized: {pc}')
        instruction = memory.instructions[pc]
        op = instruction & 0xf
        assignment = (instruction >> 4) & 7
        end_next = bool(instruction & 0x80)
        dst = (instruction >> 8) & 7
        source_a = (instruction >> 11) & 7
        source_b = (instruction >> 14) & 7
        immediate = instruction >> 14
        if immediate & (1 << 17):
            immediate -= 1 << 18
        in_branch_delay = branch_target is not None
        in_exit_delay = finish_after_delay
        next_pc = pc + 1
        new_branch = None
        new_exit = False
        if op == 7:
            if in_branch_delay or in_exit_delay or end_next:
                raise ValueError('unsupported nested MME branch/delay')
            nonzero = bool(instruction & 0x10)
            no_delay = bool(instruction & 0x20)
            taken = bool(registers[source_a]) == nonzero
            target = pc + immediate if taken else pc + 2
            if target < 0:
                raise ValueError('MME branch target out of bounds')
            if no_delay:
                if taken:
                    next_pc = target
            else:
                new_branch = target
        else:
            if op == 1:
                result = (registers[source_a] + immediate) & MASK32
            elif op == 5:
                state_index = registers[source_a] + immediate
                if not 0 <= state_index < 0x1000:
                    raise ValueError('MME state address out of bounds')
                state_address = state_index * 4
                if any(emitted_method == state_address for emitted_method, _ in emitted):
                    raise ValueError('MME read after emitted method is unsupported')
                result = read_state(state_address)
                if not isinstance(result, int) or not 0 <= result <= MASK32:
                    raise ValueError('invalid MME state value')
            elif op == 2:
                source_bit = (instruction >> 17) & 31
                bit_count = (instruction >> 22) & 31
                destination_bit = (instruction >> 27) & 31
                if source_bit + bit_count > 32 or destination_bit + bit_count > 32:
                    raise ValueError('unsupported MME bitfield merge')
                mask = ((1 << bit_count) - 1) << destination_bit
                result = ((registers[source_a] & ~mask)
                          | (((registers[source_b] >> source_bit) << destination_bit) & mask)) & MASK32
            elif op == 0:
                function = (instruction >> 17) & 0x1f
                if function == 9:
                    result = registers[source_a] | registers[source_b]
                elif function == 10:
                    result = registers[source_a] & registers[source_b]
                else:
                    raise ValueError(f'unsupported MME ALU operation {function}')
            else:
                raise ValueError(f'unsupported MME opcode {op}')
            if assignment not in (0, 1, 2, 4):
                raise ValueError(f'unsupported MME assignment {assignment}')
            if assignment == 0:
                if parameter_index >= len(parameters):
                    raise ValueError('MME parameter underflow')
                result = parameters[parameter_index]
                parameter_index += 1
            if dst:
                registers[dst] = result
            if assignment == 2:
                if result >> 16:
                    raise ValueError('unsupported MME method selector')
                method = (result & 0xfff) * 4
                method_increment = (result >> 12) & 15
            elif assignment == 4:
                if method is None or len(emitted) >= MAX_EMITTED:
                    raise ValueError('invalid or excessive MME method output')
                if method > 0x3ffc:
                    raise ValueError('MME method address overflow')
                emitted.append((method, result))
                method += method_increment * 4
            new_exit = end_next
        if in_exit_delay:
            if new_exit:
                raise ValueError('unsupported nested MME exit delay')
            if parameter_index != len(parameters):
                raise ValueError('unused MME parameters')
            return emitted
        if in_branch_delay:
            if new_exit:
                raise ValueError('unsupported MME exit in branch delay')
            next_pc = branch_target
        branch_target = new_branch
        finish_after_delay = new_exit
        pc = next_pc
    raise ValueError('MME instruction budget exceeded')

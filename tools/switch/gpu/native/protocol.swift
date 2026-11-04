func text(_ value: SpanByte) -> String { String(decoding: UnsafeBufferPointer(start: value.pointer, count: value.length), as: UTF8.self) }
func items(_ value: SpanReg) -> UnsafeBufferPointer<Reg> { UnsafeBufferPointer(start: value.pointer, count: value.length) }
func items(_ value: SpanTargetCommand) -> UnsafeBufferPointer<TargetCommand> { UnsafeBufferPointer(start: value.pointer, count: value.length) }
func items(_ value: SpanDepthCommand) -> UnsafeBufferPointer<DepthCommand> { UnsafeBufferPointer(start: value.pointer, count: value.length) }
func items(_ value: SpanConstantCommand) -> UnsafeBufferPointer<ConstantCommand> { UnsafeBufferPointer(start: value.pointer, count: value.length) }
func items(_ value: SpanAttributeCommand) -> UnsafeBufferPointer<AttributeCommand> { UnsafeBufferPointer(start: value.pointer, count: value.length) }
func items(_ value: SpanBufferCommand) -> UnsafeBufferPointer<BufferCommand> { UnsafeBufferPointer(start: value.pointer, count: value.length) }
func items(_ value: SpanSegmentCommand) -> UnsafeBufferPointer<SegmentCommand> { UnsafeBufferPointer(start: value.pointer, count: value.length) }
func items(_ value: SpanTextureCommand) -> UnsafeBufferPointer<TextureCommand> { UnsafeBufferPointer(start: value.pointer, count: value.length) }
func items(_ value: SpanCommand) -> UnsafeBufferPointer<Command> { UnsafeBufferPointer(start: value.pointer, count: value.length) }

func uniformOffset(_ command: Command, _ stage: Int, _ slot: Int) -> Int {
    var offsets = stage == 0 ? command.constant_vertex : command.constant_fragment
    return withUnsafeBytes(of: &offsets) { Int($0.load(fromByteOffset: slot * 8, as: Int64.self)) }
}

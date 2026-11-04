"""Bounded single-consumer queue for experimental Switch framebuffers."""

class BufferQueue:
    def __init__(self):
        self.buffers = {}
        self.available = set()
        self.dequeued = set()
        self.last_slot = -1

    def register(self, slot, descriptor):
        if not 0 <= slot < 64 or slot in self.dequeued:
            raise ValueError('invalid or busy display slot')
        self.buffers[slot] = descriptor
        self.available.add(slot)

    def dequeue(self):
        if not self.available:
            raise ValueError('no available display buffer')
        ordered = sorted(self.available)
        slot = next((item for item in ordered if item > self.last_slot), ordered[0])
        self.available.remove(slot)
        self.dequeued.add(slot)
        self.last_slot = slot
        return slot

    def queue(self, slot):
        if slot not in self.dequeued:
            raise ValueError('queued display slot was not dequeued')
        self.dequeued.remove(slot)
        self.available.add(slot)
        return self.buffers[slot]

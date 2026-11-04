"""Bounded timing model for submitted guest audio buffers."""
class AudioQueue:
    def __init__(self):
        self.running = False
        self.pending = []  # tag, duration remaining, completion deadline or None
        self.released = []

    def set_running(self, running, now):
        if running == self.running:
            return
        if running:
            end = now
            for item in self.pending:
                end += item[1]
                item[2] = end
        else:
            previous = now
            for item in self.pending:
                deadline = item[2]
                item[1] = max(0, deadline - previous)
                item[2] = None
                previous = max(previous, deadline)
        self.running = running

    def append(self, tag, duration, now):
        if duration < 0 or len(self.pending) + len(self.released) >= 64:
            raise ValueError('audio queue is full or duration is invalid')
        end = max(now, self.pending[-1][2]) if self.running and self.pending else now
        self.pending.append([tag, duration, end + duration if self.running else None])

    def pump(self, now):
        count = 0
        while self.running and self.pending and self.pending[0][2] <= now:
            self.released.append(self.pending.pop(0)[0])
            count += 1
        return count

    def pop_released(self, count):
        items = self.released[:count]
        del self.released[:len(items)]
        return items

    def contains(self, tag):
        return any(item[0] == tag for item in self.pending)

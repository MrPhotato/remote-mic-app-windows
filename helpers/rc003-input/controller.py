"""RC003 key lifecycle, separate from Frida and Windows for deterministic tests."""


class InputState:
    def __init__(self, emit):
        self.emit = emit
        self.generation = None
        self.sequence = 0
        self.armed = False
        self.mask = 0
        self.tv_enabled = False
        self.ok_enabled = False

    def begin(self, generation, tv_enabled=False, ok_enabled=False):
        if type(tv_enabled) is not bool:
            raise ValueError("invalid_tv_configuration")
        if type(ok_enabled) is not bool:
            raise ValueError("invalid_ok_configuration")
        if self.generation != generation:
            self.sequence = 0
        self.generation = generation
        self.armed = False
        self.mask = 0
        self.tv_enabled = tv_enabled
        self.ok_enabled = ok_enabled
        self.status("waiting", "awaiting_neutral")

    def status(self, phase, reason):
        self.emit({"type": "status", "generation": self.generation or 0, "phase": phase, "reason": reason})

    def observe(self, mask):
        allowed = 7 | (8 if self.tv_enabled else 0) | (16 if self.ok_enabled else 0)
        if type(mask) is not int or mask < 0 or mask & ~allowed:
            raise ValueError("invalid_mask")
        if not self.armed:
            if mask != 0:
                return
            self.armed = True
            self.publish(0)
            self.status("ready", "source_armed")
        elif mask != self.mask:
            self.publish(mask)

    def publish(self, mask):
        self.mask = mask
        self.sequence += 1
        self.emit({"type": "state", "generation": self.generation,
                   "sequence": self.sequence, "pressed_mask": mask})

    def stop(self, reason):
        # Status first: the parent cancels pending gestures before releasing its source.
        self.armed = False
        self.mask = 0
        self.status("waiting", reason)

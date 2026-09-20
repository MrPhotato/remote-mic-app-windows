"""Three-button lifecycle, separate from Frida and Windows for deterministic tests."""


class InputState:
    def __init__(self, emit):
        self.emit = emit
        self.generation = None
        self.sequence = 0
        self.armed = False
        self.mask = 0

    def begin(self, generation):
        if self.generation != generation:
            self.sequence = 0
        self.generation = generation
        self.armed = False
        self.mask = 0
        self.status("waiting", "awaiting_neutral")

    def status(self, phase, reason):
        self.emit({"type": "status", "generation": self.generation or 0, "phase": phase, "reason": reason})

    def observe(self, mask):
        if type(mask) is not int or not 0 <= mask <= 7:
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

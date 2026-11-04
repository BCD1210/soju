"""Optional SDL2 audio output for the experimental Switch runtime."""
import ctypes as c

class AudioSpec(c.Structure):
    _fields_ = [('freq', c.c_int), ('format', c.c_uint16), ('channels', c.c_uint8),
                ('silence', c.c_uint8), ('samples', c.c_uint16), ('padding', c.c_uint16),
                ('size', c.c_uint32), ('callback', c.c_void_p), ('userdata', c.c_void_p)]

class AudioDevice:
    def __init__(self, sdl):
        self.sdl = sdl
        self.device = 0
        sdl.SDL_InitSubSystem.argtypes = [c.c_uint32]
        sdl.SDL_InitSubSystem.restype = c.c_int
        sdl.SDL_OpenAudioDevice.argtypes = [c.c_char_p, c.c_int, c.POINTER(AudioSpec), c.POINTER(AudioSpec), c.c_int]
        sdl.SDL_OpenAudioDevice.restype = c.c_uint32
        sdl.SDL_QueueAudio.argtypes = [c.c_uint32, c.c_void_p, c.c_uint32]
        sdl.SDL_QueueAudio.restype = c.c_int
        sdl.SDL_GetQueuedAudioSize.argtypes = [c.c_uint32]
        sdl.SDL_GetQueuedAudioSize.restype = c.c_uint32
        sdl.SDL_PauseAudioDevice.argtypes = [c.c_uint32, c.c_int]
        sdl.SDL_CloseAudioDevice.argtypes = [c.c_uint32]
        if sdl.SDL_InitSubSystem(0x10) != 0:
            raise RuntimeError('SDL audio initialization failed')
        desired = AudioSpec(48000, 0x8010, 2, 0, 1024, 0, 0, None, None)
        actual = AudioSpec()
        self.device = sdl.SDL_OpenAudioDevice(None, 0, c.byref(desired), c.byref(actual), 0)
        if not self.device:
            raise RuntimeError('SDL could not open the audio device')
        if (actual.freq, actual.format, actual.channels) != (48000, 0x8010, 2):
            self.close()
            raise RuntimeError('SDL audio format mismatch')

    def pause(self, paused):
        self.sdl.SDL_PauseAudioDevice(self.device, int(paused))

    def queue(self, pcm):
        if self.sdl.SDL_GetQueuedAudioSize(self.device) + len(pcm) > 48000 * 4 * 4:
            raise ValueError('host audio queue exceeds four seconds')
        data = c.create_string_buffer(pcm, len(pcm))
        if self.sdl.SDL_QueueAudio(self.device, data, len(pcm)) != 0:
            raise RuntimeError('SDL audio queue failed')

    def close(self):
        if self.device:
            self.sdl.SDL_CloseAudioDevice(self.device)
            self.device = 0

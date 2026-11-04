"""Small SDL2 host window for the experimental 2048 NRO runner."""
import ctypes as c
import ctypes.util
import sys
import time

class Window:
    def __init__(self):
        name = ctypes.util.find_library('SDL2') or '/opt/homebrew/lib/libSDL2.dylib'
        self.sdl = c.CDLL(name)
        s = self.sdl
        s.SDL_Init.argtypes = [c.c_uint32]
        s.SDL_Init.restype = c.c_int
        s.SDL_CreateWindow.argtypes = [c.c_char_p, c.c_int, c.c_int, c.c_int, c.c_int, c.c_uint32]
        s.SDL_CreateWindow.restype = c.c_void_p
        s.SDL_CreateRenderer.argtypes = [c.c_void_p, c.c_int, c.c_uint32]
        s.SDL_CreateRenderer.restype = c.c_void_p
        s.SDL_CreateTexture.argtypes = [c.c_void_p, c.c_uint32, c.c_int, c.c_int, c.c_int]
        s.SDL_CreateTexture.restype = c.c_void_p
        s.SDL_UpdateTexture.argtypes = [c.c_void_p, c.c_void_p, c.c_void_p, c.c_int]
        s.SDL_UpdateTexture.restype = c.c_int
        s.SDL_RenderCopy.argtypes = [c.c_void_p, c.c_void_p, c.c_void_p, c.c_void_p]
        s.SDL_RenderCopy.restype = c.c_int
        s.SDL_RenderPresent.argtypes = [c.c_void_p]
        s.SDL_PollEvent.argtypes = [c.c_void_p]
        s.SDL_PollEvent.restype = c.c_int
        s.SDL_DestroyTexture.argtypes = [c.c_void_p]
        s.SDL_DestroyRenderer.argtypes = [c.c_void_p]
        s.SDL_DestroyWindow.argtypes = [c.c_void_p]
        s.SDL_Delay.argtypes = [c.c_uint32]
        s.SDL_GetError.restype = c.c_char_p
        self.window = self.renderer = self.texture = None
        self.keys = set()
        self.pending_taps = {}
        if s.SDL_Init(0x20) != 0:
            raise RuntimeError(s.SDL_GetError().decode())
        self.window = s.SDL_CreateWindow(b'Soju Switch (experimental)', 0x2fff0000, 0x2fff0000, 960, 540, 0x24)
        if self.window:
            self.renderer = s.SDL_CreateRenderer(self.window, -1, 0)
        self.texture_size = None
        self.last_present = None
        if not self.renderer:
            msg = s.SDL_GetError().decode()
            self.close()
            raise RuntimeError(msg)

    def draw_and_poll(self, pixels, width, height):
        s = self.sdl
        if len(pixels) != width * height * 4:
            raise ValueError('invalid RGBA frame size')
        if self.texture_size != (width, height):
            if self.texture:
                s.SDL_DestroyTexture(self.texture)
            # ABGR8888 has RGBA byte order on little-endian hosts.
            self.texture = s.SDL_CreateTexture(self.renderer, 0x16762004, 1, width, height)
            self.texture_size = (width, height) if self.texture else None
            if not self.texture:
                raise RuntimeError(s.SDL_GetError().decode())
        data = (c.c_ubyte * len(pixels)).from_buffer_copy(pixels)
        if s.SDL_UpdateTexture(self.texture, None, data, width * 4) != 0:
            raise RuntimeError(s.SDL_GetError().decode())
        s.SDL_RenderCopy(self.renderer, self.texture, None, None)
        now = time.monotonic()
        if self.last_present is not None:
            delay = self.last_present + 1 / 60 - now
            if delay > 0:
                time.sleep(delay)
        s.SDL_RenderPresent(self.renderer)
        self.last_present = time.monotonic()
        quit_requested = self.poll()
        return quit_requested, self.controller_mask()

    def controller_mask(self):
        now = time.monotonic()
        self.pending_taps = {key: until for key, until in self.pending_taps.items() if until > now}
        mask = 0
        for key, bit in ((80,12), (82,13), (79,14), (81,15), (5,1)):
            if key in self.keys or key in self.pending_taps:
                mask |= 1 << bit
        return mask

    def poll(self):
        s = self.sdl
        event = (c.c_ubyte * 64)()
        quit_requested = False
        while s.SDL_PollEvent(c.byref(event)):
            kind = c.c_uint32.from_buffer(event, 0).value
            if kind == 0x100:
                quit_requested = True
            elif kind == 0x200 and event[12] == 13:  # SDL_WINDOWEVENT_FOCUS_LOST
                self.keys.clear()
                self.pending_taps.clear()
            elif kind in (0x300, 0x301):
                scancode = c.c_int.from_buffer(event, 16).value
                if scancode == 41 and kind == 0x300:
                    quit_requested = True
                if kind == 0x300:
                    self.keys.add(scancode)
                    self.pending_taps[scancode] = time.monotonic() + 0.5
                else:
                    self.keys.discard(scancode)
        return quit_requested

    def close(self):
        s = self.sdl
        if self.texture:
            s.SDL_DestroyTexture(self.texture)
            self.texture = None
        if self.renderer:
            s.SDL_DestroyRenderer(self.renderer)
            self.renderer = None
        if self.window:
            s.SDL_DestroyWindow(self.window)
            self.window = None
        s.SDL_Quit()

#!/usr/bin/env python3
"""Switch the macOS keyboard input source to ABC (US) before a game starts.
A Korean/Japanese/Chinese IME swallows key presses in Wine games; macOS also
remembers the input source per app, so the game can inherit the IME from the
app you were typing in. Safe no-op when ABC is already selected or missing."""
import ctypes, ctypes.util

cf = ctypes.CDLL("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")
tis = ctypes.CDLL("/System/Library/Frameworks/Carbon.framework/Frameworks/HIToolbox.framework/HIToolbox")
cf.CFStringCreateWithCString.restype = ctypes.c_void_p
cf.CFStringCreateWithCString.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_uint32]
cf.CFDictionaryCreate.restype = ctypes.c_void_p
cf.CFDictionaryCreate.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_long, ctypes.c_void_p, ctypes.c_void_p]
cf.CFArrayGetCount.restype = ctypes.c_long; cf.CFArrayGetCount.argtypes = [ctypes.c_void_p]
cf.CFArrayGetValueAtIndex.restype = ctypes.c_void_p; cf.CFArrayGetValueAtIndex.argtypes = [ctypes.c_void_p, ctypes.c_long]
tis.TISCreateInputSourceList.restype = ctypes.c_void_p
tis.TISCreateInputSourceList.argtypes = [ctypes.c_void_p, ctypes.c_bool]
tis.TISSelectInputSource.restype = ctypes.c_int32; tis.TISSelectInputSource.argtypes = [ctypes.c_void_p]

def cfstr(s): return cf.CFStringCreateWithCString(None, s.encode(), 0x08000100)
key = ctypes.c_void_p(cfstr("TISPropertyInputSourceID"))
val = ctypes.c_void_p(cfstr("com.apple.keylayout.ABC"))
keys = (ctypes.c_void_p * 1)(key); vals = (ctypes.c_void_p * 1)(val)
d = cf.CFDictionaryCreate(None, keys, vals, 1, None, None)
lst = tis.TISCreateInputSourceList(d, False)
if lst and cf.CFArrayGetCount(lst) > 0:
    tis.TISSelectInputSource(cf.CFArrayGetValueAtIndex(lst, 0))

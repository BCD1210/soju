#!/usr/bin/env bash
# Per-game OpenGL settings in the current WINEPREFIX. Usage: gl-app-defaults.sh <wine>
#
# GoldSrc (hl.exe: Counter-Strike 1.6, Half-Life, Condition Zero, Day of Defeat...; svencoop.exe):
# with multitexture the engine uploads a lightmap rectangle before nearly every surface it draws
# (880 glTexSubImage2D per frame in one Sven Co-op scene). Apple's OpenGL runs on Metal, where a
# texture upload ends the render pass, so every upload cost a pass over the whole framebuffer:
# 7 fps at 2560x1440 on an M4 Pro. Without GL_ARB_multitexture the engine draws lightmaps in a
# second pass and uploads each lightmap page once, as with the game's own -nomtex option (#153).
# A value that is already there, an empty one included, is kept: that is how to turn this off.
set -u
WINE="${1:?path to wine}"
for APP in hl.exe svencoop.exe; do
  KEY="HKCU\\Software\\Wine\\AppDefaults\\$APP\\OpenGL"
  "$WINE" reg query "$KEY" /v DisabledExtensions >/dev/null 2>&1 ||
    "$WINE" reg add "$KEY" /v DisabledExtensions /t REG_SZ /d GL_ARB_multitexture /f >/dev/null 2>&1 || true
done

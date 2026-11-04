#pragma once
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct {
    float px,py,pz,qx,qy,qz,qw,left,right,up,down;
    float headX,headY,headZ;
    uint32_t width,height,eye,tracked;
    int64_t time;
} SojuXREye;
typedef struct {
    float px,py,pz,qx,qy,qz,qw,moveX,moveY,turnX;
    int aimValid,leftPressed,rightPressed,leftGripPressed,rightGripPressed;
} SojuXRInput;
typedef void (*SojuXRDraw)(void*,void*,void*,const SojuXREye*);
typedef void (*SojuXRControls)(void*,const SojuXRInput*);
int soju_xr_init(void);
int soju_xr_probe(void);
void* soju_xr_device(void);
int soju_xr_step(void*,SojuXRDraw,SojuXRControls);
const char* soju_xr_status(void);
void soju_xr_close(void);
#ifdef __cplusplus
}
#endif

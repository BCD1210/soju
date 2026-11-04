// Live native capture probe. Only timing/count metadata leaves process memory.
#import <AVFoundation/AVFoundation.h>
#import <Foundation/Foundation.h>

extern void *soju_mic_open(void);
extern size_t soju_mic_read(void *, int16_t *, size_t);
extern void soju_mic_close(void *);

int main(void) {
    @autoreleasepool {
        printf("authorization_before=%ld\n", (long)[AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio]);
        void *mic = soju_mic_open();
        // Give an explicitly requested interactive permission response time to arrive.
        NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:30];
        while (!mic && [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio] == AVAuthorizationStatusNotDetermined && deadline.timeIntervalSinceNow > 0) {
            [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
            if ([AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio] == AVAuthorizationStatusAuthorized) mic = soju_mic_open();
        }
        printf("authorization_after=%ld engine_open=%d\n", (long)[AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio], mic != NULL);
        if (!mic) return 2;
        double start = NSProcessInfo.processInfo.systemUptime;
        size_t total = 0, buffers = 0;
        while (NSProcessInfo.processInfo.systemUptime - start < 3) {
            int16_t samples[1920] = {0};
            size_t count = soju_mic_read(mic, samples, 1920);
            memset(samples, 0, sizeof(samples));
            if (count) {
                printf("buffer=%zu elapsed_ms=%.3f samples=%zu\n", buffers++, (NSProcessInfo.processInfo.systemUptime - start) * 1000, count);
                total += count;
            }
            [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
        }
        soju_mic_close(mic);
        printf("total_samples=%zu buffers=%zu duration_ms=%.3f\n", total, buffers, (NSProcessInfo.processInfo.systemUptime - start) * 1000);
        return total ? 0 : 3;
    }
}

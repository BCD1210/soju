#import <AVFoundation/AVFoundation.h>
#import <Foundation/Foundation.h>

#define MIC_SAMPLES (48000 * 2)
@interface SojuMicrophone : NSObject {
@public
    AVAudioEngine *engine;
    NSLock *lock;
    int16_t samples[MIC_SAMPLES];
    size_t readIndex, count;
}
@end
@implementation SojuMicrophone
@end

void *soju_mic_open(void) {
    @autoreleasepool {
        NSString *usage = [[NSBundle mainBundle] objectForInfoDictionaryKey:@"NSMicrophoneUsageDescription"];
        if (![usage isKindOfClass:[NSString class]] || usage.length == 0) return NULL;
        AVAuthorizationStatus status = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
        if (status == AVAuthorizationStatusNotDetermined) {
            [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:^(BOOL granted) { (void)granted; }];
            return NULL;
        }
        if (status != AVAuthorizationStatusAuthorized) return NULL;
        SojuMicrophone *mic = [SojuMicrophone new];
        mic->lock = [NSLock new];
        mic->engine = [AVAudioEngine new];
        @try {
            AVAudioInputNode *input = mic->engine.inputNode;
            AVAudioFormat *source = [input outputFormatForBus:0];
            if (source.sampleRate <= 0 || source.channelCount == 0) return NULL;
            AVAudioFormat *destination = [[AVAudioFormat alloc] initWithCommonFormat:AVAudioPCMFormatInt16 sampleRate:48000 channels:2 interleaved:YES];
            AVAudioConverter *converter = [[AVAudioConverter alloc] initFromFormat:source toFormat:destination];
            if (!converter) return NULL;
            __weak SojuMicrophone *weakMic = mic;
            [input installTapOnBus:0 bufferSize:1024 format:source block:^(AVAudioPCMBuffer *buffer, AVAudioTime *when) {
                (void)when;
                SojuMicrophone *live = weakMic;
                if (!live) return;
                AVAudioFrameCount capacity = (AVAudioFrameCount)ceil(buffer.frameLength * 48000.0 / source.sampleRate) + 64;
                AVAudioPCMBuffer *out = [[AVAudioPCMBuffer alloc] initWithPCMFormat:destination frameCapacity:capacity];
                __block BOOL supplied = NO;
                NSError *error = nil;
                AVAudioConverterOutputStatus result = [converter convertToBuffer:out error:&error withInputFromBlock:^AVAudioBuffer *(AVAudioPacketCount packets, AVAudioConverterInputStatus *state) {
                    (void)packets;
                    if (supplied) { *state = AVAudioConverterInputStatus_NoDataNow; return nil; }
                    supplied = YES; *state = AVAudioConverterInputStatus_HaveData; return buffer;
                }];
                if (error || result == AVAudioConverterOutputStatus_Error) return;
                const int16_t *data = out.int16ChannelData[0];
                size_t n = (size_t)out.frameLength * 2;
                [live->lock lock];
                for (size_t i = 0; i < n; ++i) {
                    if (live->count == MIC_SAMPLES) { live->readIndex = (live->readIndex + 1) % MIC_SAMPLES; --live->count; }
                    live->samples[(live->readIndex + live->count) % MIC_SAMPLES] = data[i];
                    ++live->count;
                }
                [live->lock unlock];
            }];
            NSError *error = nil;
            if (![mic->engine startAndReturnError:&error]) { [input removeTapOnBus:0]; return NULL; }
        } @catch (NSException *exception) { (void)exception; [mic->engine stop]; return NULL; }
        return (__bridge_retained void *)mic;
    }
}
size_t soju_mic_read(void *handle, int16_t *out, size_t capacity) {
    SojuMicrophone *mic = (__bridge SojuMicrophone *)handle;
    [mic->lock lock];
    size_t n = MIN(capacity, mic->count);
    for (size_t i = 0; i < n; ++i) { out[i] = mic->samples[mic->readIndex]; mic->readIndex = (mic->readIndex + 1) % MIC_SAMPLES; }
    mic->count -= n;
    [mic->lock unlock];
    return n;
}
void soju_mic_close(void *handle) {
    SojuMicrophone *mic = (__bridge_transfer SojuMicrophone *)handle;
    @try {
        [mic->engine stop];
        [mic->engine.inputNode removeTapOnBus:0];
    } @catch (NSException *exception) { (void)exception; }
}

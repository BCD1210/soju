#include "common.h"
int main(void) {
    if (!CALL(hidInitialize())) return finish();
    u32 styles = HidNpadStyleSet_NpadStandard, actual = 0;
    CALL(hidSetSupportedNpadStyleSet(styles));
    CALL(hidGetSupportedNpadStyleSet(&actual));
    check("Npad styles roundtrip", 0, actual == styles);
    const HidNpadIdType ids[] = {HidNpadIdType_No1, HidNpadIdType_Handheld};
    CALL(hidSetSupportedNpadIdType(ids, 2));
    CALL(hidSetNpadHandheldActivationMode(HidNpadHandheldActivationMode_Dual));
    HidNpadHandheldActivationMode mode;
    CALL(hidGetNpadHandheldActivationMode(&mode));
    check("handheld dual", 0, mode == HidNpadHandheldActivationMode_Dual);
    CALL(hidSetNpadJoyAssignmentModeDual(HidNpadIdType_No1));
    HidVibrationDeviceHandle vibrations[2];
    if (CALL(hidInitializeVibrationDevices(vibrations, 2, HidNpadIdType_Handheld, HidNpadStyleTag_NpadHandheld))) {
        const HidVibrationValue value = {0.1f, 160.f, 0.1f, 320.f};
        CALL(hidSendVibrationValue(vibrations[0], &value));
    }
    HidSixAxisSensorHandle sensor;
    if (CALL(hidGetSixAxisSensorHandles(&sensor, 1, HidNpadIdType_Handheld, HidNpadStyleTag_NpadHandheld))) {
        CALL(hidStartSixAxisSensor(sensor));
        bool rest = false;
        CALL(hidIsSixAxisSensorAtRest(sensor, &rest));
        check("stationary sensor", 0, rest);
        CALL(hidStopSixAxisSensor(sensor));
    }
    hidExit();
    return finish();
}

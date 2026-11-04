#include "common.h"
#include <stddef.h>
_Static_assert(offsetof(HidNpadInternalState, full_key_six_axis_sensor_lifo) == 0x1758, "sensor layout");
_Static_assert(sizeof(HidNpadSixAxisSensorLifo) == 0x708, "sensor ring layout");
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
        char info[96];
        int n = snprintf(info, sizeof(info), "sensor|style=%u|player=%u|device=%u\n", sensor.npad_style_index, sensor.player_number, sensor.device_idx);
        svcOutputDebugString(info, n);
        CALL(hidStartSixAxisSensor(sensor));
        bool rest = false;
        CALL(hidIsSixAxisSensorAtRest(sensor, &rest));
        check("stationary sensor", 0, rest);
        svcSleepThread(20000000);
        HidSixAxisSensorState state = {0};
        size_t count = hidGetSixAxisSensorStates(sensor, &state, 1);
        check("six-axis sample published", 0, count == 1 && state.sampling_number > 0);
        check("stationary gravity and angular velocity", 0,
              state.acceleration.z == 1.f && state.angular_velocity.x == 0.f &&
              state.angular_velocity.y == 0.f && state.angular_velocity.z == 0.f);
        CALL(hidStopSixAxisSensor(sensor));
    }
    hidExit();
    return finish();
}

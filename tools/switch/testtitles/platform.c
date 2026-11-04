#include "common.h"
int main(void) {
    u64 language = 0, user_time = 0, network_time = 0;
    SetRegion region;
    SetSysFirmwareVersion version;
    if (CALL(setInitialize())) {
        CALL(setGetLanguageCode(&language));
        check("language=en-US", 0, language == 0x53552d6e65ULL);
        CALL(setGetRegionCode(&region));
        check("region=USA", 0, region == SetRegion_USA);
        setExit();
    }
    if (CALL(setsysInitialize())) {
        CALL(setsysGetFirmwareVersion(&version));
        check("firmware ABI=17", 0, version.major == 17);
        setsysExit();
    }
    if (CALL(timeInitialize())) {
        CALL(timeGetCurrentTime(TimeType_UserSystemClock, &user_time));
        CALL(timeGetCurrentTime(TimeType_NetworkSystemClock, &network_time));
        check("clocks nonzero and coherent", 0, user_time > 0 && network_time >= user_time && network_time-user_time <= 2);
        timeExit();
    }
    if (CALL(plInitialize(PlServiceType_User))) {
        PlFontData font = {0};
        CALL(plGetSharedFontByType(&font, PlSharedFontType_Standard));
        check("mapped font", 0, font.address && font.size > 0);
        plExit();
    }
    return finish();
}

#include "common.h"
int main(void) {
    if (CALL(nifmInitialize(NifmServiceType_User))) {
        bool enabled = true;
        CALL(nifmIsWirelessCommunicationEnabled(&enabled));
        check("wireless offline", 0, !enabled);
        u32 ip = 0;
        Result rc = nifmGetCurrentIpAddress(&ip);
        check("no IP while offline", 0, R_FAILED(rc) && ip == 0);
        nifmExit();
    }
    if (CALL(sslInitialize(1))) sslExit();
    if (CALL(friendsInitialize(FriendsServiceType_User))) friendsExit();
    if (CALL(pctlInitialize())) {
        bool restricted = true;
        CALL(pctlIsRestrictionEnabled(&restricted));
        check("local unrestricted profile", 0, !restricted);
        pctlExit();
    }
    return finish();
}

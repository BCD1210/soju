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
    if (CALL(sslInitialize(1))) {
        SslContext context;
        if (CALL(sslCreateContext(&context, SslVersion_Auto))) {
            u32 count=0;
            CALL(sslGetContextCount(&count));
            check("local TLS context", 0, count == 1);
            sslContextClose(&context);
            CALL(sslGetContextCount(&count));
            check("TLS context released", 0, count == 0);
        }
        sslExit();
    }
    if (CALL(friendsInitialize(FriendsServiceType_User))) friendsExit();
    if (CALL(pctlInitialize())) {
        bool restricted = true;
        CALL(pctlIsRestrictionEnabled(&restricted));
        check("local unrestricted profile", 0, !restricted);
        pctlExit();
    }
    Service prepo;
    if (CALL(smGetService(&prepo, "prepo:u"))) {
        u32 status = ~0U;
        CALL(serviceDispatchOut(&prepo, 10300, status));
        check("prepo not transmitting", 0, status == 0);
        serviceClose(&prepo);
    }
    return finish();
}

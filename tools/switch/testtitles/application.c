#include "common.h"
u32 __nx_applet_type = AppletType_Application;
int main(void) {
    __nx_applet_type = AppletType_Application;
    if (!CALL(appletInitialize())) return finish();
    check("focus=in-focus", 0, appletGetFocusState() == AppletFocusState_InFocus);
    check("operation=handheld", 0, appletGetOperationMode() == AppletOperationMode_Handheld);
    check("performance=normal", 0, appletGetPerformanceMode() == ApmPerformanceMode_Normal);
    u64 language = 0;
    CALL(appletGetDesiredLanguage(&language));
    check("desired language", 0, language == 0x53552d6e65ULL);
    if (CALL(accountInitialize(AccountServiceType_Application))) {
        AccountUid uid = {0};
        if (CALL(accountGetPreselectedUser(&uid))) {
            check("preselected local user", 0, uid.uid[0] == 1 && uid.uid[1] == 0);
            AccountProfile profile;
            if (CALL(accountGetProfile(&profile, uid))) {
                AccountProfileBase base = {0}; AccountUserData data = {0};
                CALL(accountProfileGet(&profile, &data, &base));
                check("profile name", 0, strcmp(base.nickname, "Soju") == 0);
                accountProfileClose(&profile);
            }
        }
        accountExit();
    }
    appletExit();
    return finish();
}

#include "common.h"
int main(void) {
    if (!CALL(fsInitialize())) return finish();
    CALL(fsdevMountSdmc());
    FsSaveDataAttribute attr = {0};
    attr.uid.uid[0] = 1;
    attr.save_data_type = FsSaveDataType_Account;
    FsSaveDataCreationInfo creation = {0};
    creation.save_data_size = 0x100000;
    creation.journal_size = 0x100000;
    creation.save_data_space_id = FsSaveDataSpaceId_User;
    FsSaveDataMetaInfo meta = {0};
    CALL(fsCreateSaveDataFileSystem(&attr, &creation, &meta));
    FsFileSystem save;
    if (CALL(fsOpen_SaveData(&save, 0, attr.uid))) {
        FsFile file;
        Result rc = fsFsOpenFile(&save, "/probe.txt", FsOpenMode_Read|FsOpenMode_Write, &file);
        if (R_SUCCEEDED(rc)) {
            char previous[8] = {0}; u64 count = 0;
            CALL(fsFileRead(&file, 0, previous, sizeof(previous), FsReadOption_None, &count));
            check("previous committed save", 0, count == 8 && memcmp(previous, "persist!", 8) == 0);
        }
        if (R_FAILED(rc)) {
            CALL(fsFsCreateFile(&save, "/probe.txt", 8, 0));
            rc = fsFsOpenFile(&save, "/probe.txt", FsOpenMode_Read|FsOpenMode_Write, &file);
        }
        if (check("open save file", rc, true)) {
            CALL(fsFileWrite(&file, 0, "persist!", 8, FsWriteOption_Flush));
            char bytes[8] = {0}; u64 count = 0;
            CALL(fsFileRead(&file, 0, bytes, sizeof(bytes), FsReadOption_None, &count));
            check("save readback", 0, count == 8 && memcmp(bytes, "persist!", 8) == 0);
            fsFileClose(&file);
        }
        CALL(fsFsCommit(&save));
        fsFsClose(&save);
    }
    for (unsigned i=0; i<2; i++) {
        attr.uid = (AccountUid){0};
        attr.save_data_type = i ? FsSaveDataType_Cache : FsSaveDataType_Bcat;
        CALL(fsCreateSaveDataFileSystem(&attr, &creation, &meta));
        FsFileSystem volume;
        Result opened = i ? fsOpen_CacheStorage(&volume, 0, 0) : fsOpen_BcatSaveData(&volume, 0);
        if (check(i ? "cache mount" : "BCAT mount", opened, true)) {
            CALL(fsFsCommit(&volume));
            fsFsClose(&volume);
        }
    }
    Result rc = romfsInit();
    if (check("RomFS mount", rc, true)) {
        FILE *f = fopen("romfs:/probe.txt", "rb"); char bytes[64] = {0};
        if (f) { fread(bytes, 1, sizeof(bytes)-1, f); fclose(f); }
        check("RomFS content", 0, strcmp(bytes, "Soju own test title RomFS\n") == 0);
        romfsExit();
    }
    Service arp;
    if (CALL(smGetService(&arp, "arp:r"))) {
        u64 process=0, instance=0;
        CALL(svcGetProcessId(&process, CUR_PROCESS_HANDLE));
        CALL(serviceDispatchInOut(&arp, 3, process, instance));
        NacpStruct control = {0};
        CALL(serviceDispatchIn(&arp, 1, instance,
            .buffer_attrs = { SfBufferAttr_HipcMapAlias | SfBufferAttr_Out },
            .buffers = { { &control, sizeof(control) } }));
        check("control title from container", 0, strcmp(control.lang[0].name, "Soju storage probe") == 0);
        serviceClose(&arp);
    }
    fsExit();
    return finish();
}

#include "common.h"
int main(void) {
    if (!CALL(fsInitialize())) return finish();
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
    Result rc = romfsInit();
    if (check("RomFS mount", rc, true)) {
        FILE *f = fopen("romfs:/probe.txt", "rb"); char bytes[64] = {0};
        if (f) { fread(bytes, 1, sizeof(bytes)-1, f); fclose(f); }
        check("RomFS content", 0, strcmp(bytes, "Soju own test title RomFS\n") == 0);
        romfsExit();
    }
    fsExit();
    return finish();
}

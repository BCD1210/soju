#include <os/signpost.h>
#include <stdint.h>
#include <pthread.h>
static os_log_t log_handle;
static pthread_once_t once = PTHREAD_ONCE_INIT;
static void initialize(void) { log_handle = os_log_create("org.soju.switch", OS_LOG_CATEGORY_POINTS_OF_INTEREST); }
uint64_t soju_profile_begin(const char *stage) {
    pthread_once(&once, initialize);
    os_log_t log = log_handle;
    os_signpost_id_t id = os_signpost_id_generate(log);
    os_signpost_interval_begin(log, id, "Stage", "%{public}s", stage);
    return id;
}
void soju_profile_end(uint64_t id) {
    pthread_once(&once, initialize);
    os_log_t log = log_handle;
    os_signpost_interval_end(log, id, "Stage");
}

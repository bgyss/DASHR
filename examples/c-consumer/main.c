#include "dashr.h"
#include <stdio.h>

_Static_assert(sizeof(DashrCTrace) == 20, "DashrCTrace ABI layout changed");
#if UINTPTR_MAX == UINT64_MAX
_Static_assert(sizeof(DashrCFrameView) == 48, "DashrCFrameView ABI layout changed");
#endif

static void log_message(uint32_t level, const char *message, void *user_data) {
    (void)user_data;
    fprintf(stderr, "DASHR[%u] %s\n", level, message);
}

int main(void) {
    if (dashr_ffi_abi_version() != DASHR_FFI_ABI_VERSION) {
        fprintf(stderr, "DASHR C ABI version mismatch\n");
        return 1;
    }
    if (dashr_set_log_callback(log_message, NULL) != 0) {
        fprintf(stderr, "DASHR log callback registration failed: %s\n", dashr_last_error_message());
        return 1;
    }
    if (dashr_status_code(DASHR_STATUS_HIT) != DASHR_STATUS_HIT ||
        dashr_status_code(999u) != UINT32_MAX) {
        fprintf(stderr, "DASHR C ABI status mapping failed\n");
        return 2;
    }
    const char *settings = "{\"mesh\":\"tube\",\"atlas\":64,\"width\":64,\"height\":64,\"texture_set\":3,\"time\":0,\"animation_amount\":0}";
    DashrFfiSession *session = dashr_session_create(settings, NULL, "demo/assets");
    if (!session) {
        fprintf(stderr, "DASHR create failed: %s\n", dashr_last_error_message());
        return 3;
    }
    if (dashr_session_update_pose(session, 0.4f, 1.0f) != 0) {
        fprintf(stderr, "DASHR pose update failed: %s\n", dashr_last_error_message());
        dashr_session_destroy(session);
        return 4;
    }
    const DashrCFrameView *frame = dashr_session_render(session);
    if (!frame || frame->abi_version != DASHR_FFI_ABI_VERSION || frame->width != 64 || frame->height != 64) {
        fprintf(stderr, "DASHR render failed: %s\n", dashr_last_error_message());
        dashr_session_destroy(session);
        return 5;
    }
    uint32_t hits = 0;
    for (uint32_t i = 0; i < frame->width * frame->height; ++i) {
        hits += frame->primary[i].status == DASHR_STATUS_HIT;
    }
    printf("DASHR C ABI %u; %ux%u; %u primary hits\n",
        dashr_ffi_abi_version(), frame->width, frame->height, hits);
    dashr_session_destroy(session);
    return 0;
}

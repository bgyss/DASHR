#if defined(_WIN32)
#  if defined(DASHR_BUILDING_LIBRARY)
#    define DASHR_API __declspec(dllexport)
#  else
#    define DASHR_API __declspec(dllimport)
#  endif
#else
#  define DASHR_API __attribute__((visibility("default")))
#endif


#ifndef DASHR_H
#define DASHR_H

/* Generated from src/ffi.rs by cbindgen 0.29.4. Do not edit directly. */

#include <stdint.h>

#define DASHR_FFI_ABI_VERSION 1

#define DASHR_STATUS_NOT_LAUNCHED 0

#define DASHR_STATUS_HIT 1

#define DASHR_STATUS_ESCAPED 2

#define DASHR_STATUS_BUDGET_EXHAUSTED 3

#define DASHR_STATUS_INVALID_BASIS 4

#define DASHR_STATUS_DEBUG_FORCED_HIT 5

#define DASHR_LOG_DEBUG 0

#define DASHR_LOG_INFO 1

#define DASHR_LOG_WARNING 2

#define DASHR_LOG_ERROR 3

/*
 Opaque C handle. Calls that use a handle must be serialized on one host thread.
 */
typedef struct DashrFfiSession DashrFfiSession;

/*
 Nullable host callback invoked synchronously on the thread that registered it.
 */
typedef void (*DashrLogCallback)(uint32_t, const char*, void*);

typedef struct {
  uint32_t struct_size;
  uint32_t status;
  uint32_t steps;
  uint32_t teleports;
  float auxiliary;
} DashrCTrace;

typedef struct {
  uint32_t struct_size;
  uint32_t abi_version;
  uint32_t width;
  uint32_t height;
  /*
   `width * height * 4` floats in RGBA order. Valid until the next handle call.
   */
  const float *color_rgba;
  /*
   `width * height * 4` floats: hit U, hit V, ray distance, reverse-Z depth.
   */
  const float *hit_uv_distance_depth;
  /*
   `width * height` entries. Valid until the next handle call.
   */
  const DashrCTrace *primary;
  /*
   `width * height` entries. Valid until the next handle call.
   */
  const DashrCTrace *shadow;
} DashrCFrameView;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/*
 Returns the version of the C struct/function contract, separate from asset and Rust API versions.
 */
DASHR_API uint32_t dashr_ffi_abi_version(void);

/*
 Returns a thread-local error string, valid until the next DASHR FFI call on this thread.
 */
DASHR_API const char *dashr_last_error_message(void);

/*
 Registers or clears a per-thread host log callback.

 The callback is invoked synchronously on this thread. `message` is a temporary NUL-terminated
 UTF-8 string valid only for the callback duration. The host owns `user_data` and must keep it
 valid until it clears the registration. Passing a null callback unregisters the current one.
 */
DASHR_API int32_t dashr_set_log_callback(DashrLogCallback callback, void *user_data);

/*
 Creates a headless session from optional settings/asset JSON and an asset root path.

 A null settings string selects `Settings::default`. A null asset string selects the procedural
 mesh in settings; otherwise the string must contain a valid versioned `AssetDocument`. The
 asset root must be a valid NUL-terminated UTF-8 string. The returned handle must be destroyed
 exactly once.

 # Safety
 Each non-null input pointer must reference a readable NUL-terminated string for the duration
 of this call.
 */
DASHR_API
DashrFfiSession *dashr_session_create(const char *settings_json,
                                      const char *asset_json,
                                      const char *asset_root);

/*
 Updates the four-bone reference animation. Returns zero on success and minus one on error.

 # Safety
 `handle` must be a live handle returned by `dashr_session_create`; calls using it must be
 serialized on the creating thread.
 */
DASHR_API
int32_t dashr_session_update_pose(DashrFfiSession *handle,
                                  float time,
                                  float animation_amount);

/*
 Renders and returns a borrowed frame view, or null on error.

 All pointers in the returned view remain valid until the next update, render, or destroy call
 on this handle. The caller must serialize handle access.

 # Safety
 `handle` must be a live handle returned by `dashr_session_create`; calls using it must be
 serialized on the creating thread.
 */
DASHR_API const DashrCFrameView *dashr_session_render(DashrFfiSession *handle);

/*
 Destroys a session handle. Passing null is an error and does not modify memory.

 # Safety
 A non-null `handle` must be a live handle returned by `dashr_session_create`, and it must not
 have been destroyed previously.
 */
DASHR_API void dashr_session_destroy(DashrFfiSession *handle);

/*
 Maps a public status code to the stable numeric C status value, or `u32::MAX` if unknown.
 */
DASHR_API uint32_t dashr_status_code(uint32_t status);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* DASHR_H */

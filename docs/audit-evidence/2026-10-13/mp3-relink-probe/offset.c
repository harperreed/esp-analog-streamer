// ABOUTME: Verifies the private MP3 header member's offset for the ESP32 compiler.
// ABOUTME: Produces target assembly without running or linking any firmware.
#include <stddef.h>
#include "types.h"
_Static_assert(offsetof(shine_global_config, mpeg.ext) == 88, "unexpected ext offset");
const unsigned ext_offset = offsetof(shine_global_config, mpeg.ext);

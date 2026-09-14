#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef int32_t (*NeloaIosDiscoveryStart)(const char *json);

void neloa_ios_discovery_register_start(NeloaIosDiscoveryStart start);
void neloa_ios_discovery_peer_upsert(const char *json);
void neloa_ios_discovery_peer_remove(const char *fullname);
void neloa_ios_discovery_status(const char *json);

#ifdef __cplusplus
}
#endif

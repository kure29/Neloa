#pragma once

#ifdef __cplusplus
extern "C" {
#endif

void neloa_ios_discovery_peer_upsert(const char *json);
void neloa_ios_discovery_peer_remove(const char *fullname);
void neloa_ios_discovery_status(const char *json);

#ifdef __cplusplus
}
#endif

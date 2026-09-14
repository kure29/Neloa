#include "bindings/bindings.h"
#include "BonjourBridge.h"

extern "C" int32_t neloa_ios_discovery_start(const char *json);

int main(int argc, char * argv[]) {
	neloa_ios_discovery_register_start(neloa_ios_discovery_start);
	ffi::start_app();
	return 0;
}

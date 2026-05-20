/**
 * \file plugin.c
 * \brief C plugin template - minimal lifecycle skeleton.
 */

#include "host_api.h"
#include "plugin_lifecycle.h"

uint16_t plugin_required_api_major(void) { return HOST_API_LEVEL_MAJOR; }
uint16_t plugin_required_api_minor(void) { return HOST_API_LEVEL_MINOR; }

int plugin_init  (void) { return 0; }
int plugin_deinit(void) { return 0; }
int plugin_on_exit(void) { return 0; }

int plugin_on_enter(void) {
    host_ui_push_toast("Hello from C!", UI_ICON_SUCCESS, 1500);
    return 0;
}

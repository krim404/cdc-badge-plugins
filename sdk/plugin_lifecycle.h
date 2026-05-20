/**
 * \file plugin_lifecycle.h
 * \brief Lifecycle exports a CDC Badge plugin must (and may) provide.
 *
 * A plugin is a WebAssembly module that the badge loads from its plugins
 * partition. The functions below are imported by the host from the WASM
 * module's export table. Functions marked REQUIRED are checked at load time
 * and a missing export causes the plugin to be rejected.
 */

#ifndef CDC_BADGE_PLUGIN_LIFECYCLE_H
#define CDC_BADGE_PLUGIN_LIFECYCLE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* REQUIRED - API level compatibility */
uint16_t plugin_required_api_major(void);
uint16_t plugin_required_api_minor(void);

/* REQUIRED - lifecycle */
int plugin_init  (void);                /* once after load */
int plugin_deinit(void);                /* before unload */
int plugin_on_enter(void);              /* user opens the plugin */
int plugin_on_exit (void);              /* user leaves the plugin */

/* OPTIONAL - event handlers, omit if not used */
int plugin_on_action(uint32_t action_id, uint32_t selected_idx, uint32_t user_data);
int plugin_on_button(uint32_t button_code);
int plugin_on_event (uint32_t event_type, uint32_t event_value);
int plugin_on_tick  (uint64_t uptime_ms);

/* OPTIONAL - declare this export if any prerequisite uses on_fail=callback */
int plugin_on_prerequisite_failed(uint32_t prereq_id, uint32_t error_code);

#ifdef __cplusplus
}
#endif

#endif /* CDC_BADGE_PLUGIN_LIFECYCLE_H */

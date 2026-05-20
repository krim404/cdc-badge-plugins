/**
 * \file host_api.h
 * \brief CDC Badge OS plugin host API - canonical C ABI contract.
 *
 * Mirrored from cdc-badge-os/components/plugin_manager/include/plugin_manager/host_api.h.
 * A CI job in both repositories detects drift between the two copies.
 *
 * All host functions are imported by the WASM plugin from the `cdc` module.
 * Plugin lifecycle exports (plugin_init, plugin_on_enter, ...) are declared
 * in plugin_lifecycle.h.
 */

#ifndef CDC_BADGE_HOST_API_H
#define CDC_BADGE_HOST_API_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------------- */
/* API Level                                                                 */
/* ------------------------------------------------------------------------- */

#define HOST_API_LEVEL_MAJOR  0
#define HOST_API_LEVEL_MINOR  5
#define HOST_API_LEVEL_STR    "0.5"
#define HOST_API_LEVEL_PACKED (((uint32_t)HOST_API_LEVEL_MAJOR << 16) | HOST_API_LEVEL_MINOR)

/* ------------------------------------------------------------------------- */
/* Return Codes                                                              */
/* ------------------------------------------------------------------------- */

#define HOST_OK                  0
#define HOST_ERR_GENERIC        -1
#define HOST_ERR_INVALID_ARG    -2
#define HOST_ERR_NO_CAPABILITY  -3
#define HOST_ERR_NOT_FOUND      -4
#define HOST_ERR_TIMEOUT        -5
#define HOST_ERR_NO_MEMORY      -6
#define HOST_ERR_BUSY           -7
#define HOST_ERR_NOT_SUPPORTED  -8
#define HOST_ERR_RMEM_FULL      -9

/* ------------------------------------------------------------------------- */
/* Logging                                                                   */
/* ------------------------------------------------------------------------- */

#define LOG_LEVEL_ERROR   0
#define LOG_LEVEL_WARN    1
#define LOG_LEVEL_INFO    2
#define LOG_LEVEL_DEBUG   3
#define LOG_LEVEL_VERBOSE 4

void host_log    (uint8_t level, const char* tag, const char* msg);
void host_log_hex(const char* tag, const char* label, const uint8_t* data, size_t len);

/* ------------------------------------------------------------------------- */
/* Time / RTC                                                                */
/* ------------------------------------------------------------------------- */

struct host_tm {
    uint16_t year;        /* 1900-3000 */
    uint8_t  month;       /* 1-12 */
    uint8_t  day;         /* 1-31 */
    uint8_t  hour;        /* 0-23 */
    uint8_t  minute;      /* 0-59 */
    uint8_t  second;      /* 0-59 */
    uint8_t  weekday;     /* 0=Sunday */
};

uint64_t host_uptime_ms        (void);
int64_t  host_unix_time        (void);
int      host_local_time       (struct host_tm* out);
int32_t  host_timezone_offset  (void);
bool     host_is_time_set      (void);

/* ------------------------------------------------------------------------- */
/* Power                                                                     */
/* ------------------------------------------------------------------------- */

#define POWER_SRC_UNKNOWN  0
#define POWER_SRC_BATTERY  1
#define POWER_SRC_USB      2

#define CHARGE_NOT_CHARGING 0
#define CHARGE_PRE_CHARGE   1
#define CHARGE_FAST         2
#define CHARGE_DONE         3
#define CHARGE_FAULT        4

uint16_t host_battery_mv         (void);
uint8_t  host_battery_pct        (void);
bool     host_is_usb_connected   (void);
uint8_t  host_power_source       (void);
uint8_t  host_charge_status      (void);
bool     host_is_battery_low     (void);
bool     host_is_battery_critical(void);

/* ------------------------------------------------------------------------- */
/* Crypto                                                                    */
/* ------------------------------------------------------------------------- */

int host_random        (uint8_t* buf, size_t len);
int host_random_strict (uint8_t* buf, size_t len);
int host_sha256        (const uint8_t* data, size_t len, uint8_t out[32]);
int host_hmac_sha256   (const uint8_t* key, size_t klen,
                        const uint8_t* data, size_t dlen, uint8_t out[32]);
int host_aes_gcm_encrypt(const uint8_t* key, const uint8_t* iv,
                         const uint8_t* aad, size_t aad_len,
                         const uint8_t* pt, size_t pt_len,
                         uint8_t* ct, uint8_t tag[16]);
int host_aes_gcm_decrypt(const uint8_t* key, const uint8_t* iv,
                         const uint8_t* aad, size_t aad_len,
                         const uint8_t* ct, size_t ct_len,
                         const uint8_t tag[16], uint8_t* pt);

int host_base32_encode(const uint8_t* in, size_t in_len, char* out, size_t out_size);
int host_base32_decode(const char* in, size_t in_len, uint8_t* out, size_t out_size);
int host_base64_encode(const uint8_t* in, size_t in_len, char* out, size_t out_size);
int host_base64_decode(const char* in, size_t in_len, uint8_t* out, size_t out_size);
int host_hex_encode   (const uint8_t* in, size_t in_len, char* out, size_t out_size);
int host_hex_decode   (const char* in, size_t in_len, uint8_t* out, size_t out_size);

/* ------------------------------------------------------------------------- */
/* SecureElement / TROPIC01                                                  */
/* ------------------------------------------------------------------------- */

#define ECC_CURVE_P256    0
#define ECC_CURVE_ED25519 1

/*
 * Retained memory (rmem) is allocated by name from a shared plugin pool.
 * Slots persist across reboot, plugin uninstall, and plugin reinstall as
 * long as the name remains declared in some installed plugin's
 * `capabilities.rmem`. Plugins declaring the same name share the same
 * physical slot (intentional, common scope).
 *
 * Name length: max 15 bytes excluding the trailing NUL.
 */

#define HOST_RMEM_NAME_MAX 15

int      host_rmem_read_named  (const char* name, uint8_t* buf, size_t* len);
int      host_rmem_write_named (const char* name, const uint8_t* buf, size_t len);
int      host_rmem_erase_named (const char* name);
bool     host_rmem_name_used   (const char* name);
uint16_t host_rmem_slot_size   (void);

int      host_ecc_generate (uint8_t slot, uint8_t curve);
int      host_ecc_import   (uint8_t slot, const uint8_t* priv, uint8_t curve);
int      host_ecc_pubkey   (uint8_t slot, uint8_t* pub, uint8_t curve);
int      host_ecc_delete   (uint8_t slot);
bool     host_ecc_slot_used(uint8_t slot);
int      host_ecdsa_sign   (uint8_t slot, const uint8_t* msg, size_t len, uint8_t sig[64]);
int      host_eddsa_sign   (uint8_t slot, const uint8_t* msg, size_t len, uint8_t sig[64]);

int      host_se_chip_id   (uint8_t* serial, size_t* len);
int      host_se_fw_version(uint8_t* riscv, uint8_t* spect);

/* ------------------------------------------------------------------------- */
/* HTTP (streamed)                                                           */
/* ------------------------------------------------------------------------- */

#define HTTP_GET    0
#define HTTP_POST   1
#define HTTP_PUT    2
#define HTTP_DELETE 3

int    host_http_open          (uint8_t method, const char* url, uint32_t timeout_ms);
int    host_http_set_header    (int handle, const char* key, const char* value);
int    host_http_set_body      (int handle, const uint8_t* body, size_t len);
int    host_http_perform       (int handle);
int    host_http_status        (int handle);
int    host_http_read_chunk    (int handle, uint8_t* buf, size_t buf_size, size_t* out_len);
size_t host_http_content_length(int handle);
int    host_http_close         (int handle);

/* ------------------------------------------------------------------------- */
/* WiFi                                                                      */
/* ------------------------------------------------------------------------- */

typedef struct {
    char    ssid[33];
    uint8_t bssid[6];
    int8_t  rssi;
    uint8_t channel;
    uint8_t auth_mode;
} wifi_scan_result_t;

int     host_wifi_request    (uint32_t timeout_ms);
int     host_wifi_release    (void);
bool    host_wifi_is_connected(void);
int     host_wifi_ssid       (char* out, size_t out_size);
int     host_wifi_ip         (char* out, size_t out_size);
int8_t  host_wifi_rssi       (void);
int     host_wifi_mac        (uint8_t out[6]);
int     host_wifi_start_scan (void);
bool    host_wifi_scan_done  (void);
int     host_wifi_scan_results(wifi_scan_result_t* out, size_t* count);

/* ------------------------------------------------------------------------- */
/* BLE                                                                       */
/* ------------------------------------------------------------------------- */

typedef struct {
    uint8_t  uuid[16];
    uint8_t  is_primary;
    uint8_t  reserved[2];
    uint16_t handle;
} ble_service_def_t;

typedef struct {
    uint8_t addr[6];
    int8_t  rssi;
    char    name[32];
} ble_scan_result_t;

bool    host_ble_is_enabled       (void);
int     host_ble_mac              (uint8_t out[6]);
int     host_ble_device_name      (char* out, size_t out_size);
int8_t  host_ble_rssi             (void);
int     host_ble_register_service (const ble_service_def_t* def, uint32_t* service_handle_out);
int     host_ble_send_notification(uint32_t char_handle, const uint8_t* data, size_t len);
int     host_ble_send_indication  (uint32_t char_handle, const uint8_t* data, size_t len);
int     host_ble_unregister_service(uint32_t service_handle);

int     host_ble_scan_start       (void);
int     host_ble_scan_results     (ble_scan_result_t* out, size_t* count);
int     host_ble_connect          (const uint8_t addr[6]);
int     host_ble_read_char        (uint32_t conn, const uint8_t uuid[16],
                                   uint8_t* buf, size_t* len);
int     host_ble_write_char       (uint32_t conn, const uint8_t uuid[16],
                                   const uint8_t* data, size_t len);
int     host_ble_subscribe        (uint32_t conn, const uint8_t uuid[16], uint32_t action_id);
int     host_ble_disconnect       (uint32_t conn);

/* ------------------------------------------------------------------------- */
/* NVS (plugin-namespaced)                                                   */
/* ------------------------------------------------------------------------- */

int host_nvs_get_blob (const char* key, uint8_t* buf, size_t* len);
int host_nvs_set_blob (const char* key, const uint8_t* buf, size_t len);
int host_nvs_get_u32  (const char* key, uint32_t* out);
int host_nvs_set_u32  (const char* key, uint32_t value);
int host_nvs_get_str  (const char* key, char* buf, size_t buf_size);
int host_nvs_set_str  (const char* key, const char* value);
int host_nvs_erase    (const char* key);
int host_nvs_erase_all(void);
int host_nvs_list_keys(char* out, size_t* out_len);

/* ------------------------------------------------------------------------- */
/* UI - Views                                                                */
/* ------------------------------------------------------------------------- */

typedef struct {
    const char* label;
    uint8_t     icon;
    bool        icon_disabled;
    uint32_t    item_id;
} ui_item_t;

/* CP437 glyph codes routed straight to Adafruit-GFX. cdc_log + plugin SDKs
 * mirror these. UI_ICON_NONE renders a default bullet point. */
#define UI_ICON_NONE            0
#define UI_ICON_SUCCESS         1    /* smiley                             */
#define UI_ICON_ERROR           2    /* dark smiley                        */
#define UI_ICON_HEART           3    /* favorite                           */
#define UI_ICON_DIAMOND         4
#define UI_ICON_CLUB            5
#define UI_ICON_SPADE           6
#define UI_ICON_BULLET          7
#define UI_ICON_INVERSE_BULLET  8    /* remove / delete                    */
#define UI_ICON_CIRCLE          9    /* info / hollow circle               */
#define UI_ICON_INVERSE_CIRCLE  0x0A
#define UI_ICON_MALE            0x0B
#define UI_ICON_FEMALE          0x0C
#define UI_ICON_MUSIC           0x0D
#define UI_ICON_NOTES           0x0E /* scene / playlist                   */
#define UI_ICON_SUN             0x0F /* brightness / light                 */
#define UI_ICON_PLAY            0x10 /* action / switch / submenu          */
#define UI_ICON_REVERSE_PLAY    0x11 /* back                               */
#define UI_ICON_UPDOWN          0x12 /* cover / vertical adjust            */
#define UI_ICON_ALERT           0x13 /* double exclamation                 */
#define UI_ICON_PARAGRAPH       0x14
#define UI_ICON_SECTION         0x15
#define UI_ICON_BAR             0x16 /* sensor / list / count              */
#define UI_ICON_UPDOWN_BAR      0x17
#define UI_ICON_ARROW_UP        0x18
#define UI_ICON_ARROW_DOWN      0x19
#define UI_ICON_ARROW_RIGHT     0x1A
#define UI_ICON_ARROW_LEFT      0x1B
#define UI_ICON_ANGLE           0x1C
#define UI_ICON_LEFTRIGHT       0x1D
#define UI_ICON_TRIANGLE_UP     0x1E
#define UI_ICON_TRIANGLE_DOWN   0x1F

/* Semantic aliases. Plugins are free to use either the CP437 name or
 * a semantic one - they map to the same byte. */
#define UI_ICON_INFO    UI_ICON_CIRCLE
#define UI_ICON_TASK    UI_ICON_PLAY
#define UI_ICON_REMOVE  UI_ICON_INVERSE_BULLET
#define UI_ICON_LIGHT   UI_ICON_SUN
#define UI_ICON_COVER   UI_ICON_UPDOWN
#define UI_ICON_SENSOR  UI_ICON_BAR
#define UI_ICON_SWITCH  UI_ICON_PLAY
#define UI_ICON_SCENE   UI_ICON_NOTES
#define UI_ICON_BACK    UI_ICON_REVERSE_PLAY

int host_ui_push_toast      (const char* text, uint8_t icon, uint16_t duration_ms);
int host_ui_push_message    (const char* text, uint8_t icon, uint32_t duration_ms);
int host_ui_push_confirm    (const char* text, uint8_t icon, uint32_t action_id);
int host_ui_push_info       (const char* title, const char* body);
int host_ui_push_context_menu(const char* title, const ui_item_t* items, uint16_t count,
                              uint32_t select_action_id);
int host_ui_push_t9_input   (const char* title, const char* initial,
                             uint16_t max_len, uint32_t action_id);
int host_ui_push_password   (const char* title, const char* initial,
                             uint16_t max_len, uint32_t action_id);
int host_ui_push_pin_entry  (const char* title, uint8_t max_len, uint8_t max_attempts,
                             uint32_t action_id);
int host_ui_push_slider     (const char* title, int32_t min, int32_t max, int32_t init,
                             int32_t step, const char* unit, uint32_t action_id);
/* RGB color picker. On Y the host fires action_id with idx = packed RGB
 * (0xRRGGBB) and user_data = 1. On N the action_id is not fired (view pops).
 * Read the packed value via host_ui_consume_input_int(). */
int host_ui_push_color_picker(uint8_t initial_r, uint8_t initial_g, uint8_t initial_b,
                              uint32_t action_id);
int host_ui_push_date       (const char* title, uint8_t d, uint8_t m, uint16_t y,
                             uint32_t action_id);
int host_ui_push_time       (const char* title, uint8_t h, uint8_t m, uint32_t action_id);
int host_ui_push_list       (const char* title, const ui_item_t* items, uint16_t count,
                             uint32_t select_action_id, uint32_t menu_action_id);
/* Replace the plugin's currently-top list view with a fresh one. Falls back
 * to a plain push when the plugin has no list on top (e.g. on first call).
 * Use this for "refresh after toggle" patterns so the view stack does not
 * grow on every action. */
int host_ui_replace_list    (const char* title, const ui_item_t* items, uint16_t count,
                             uint32_t select_action_id, uint32_t menu_action_id);
/* Override the footer hint for the plugin's current top view (list, confirm,
 * T9, pin, slider, date or time). Pass NULL or an empty string to fall back
 * to the view's default hint. The text is copied internally so the caller
 * can free it after the call returns. Returns HOST_ERR_NOT_FOUND if the top
 * view is not owned by the plugin runtime. */
int host_ui_set_view_footer (const char* hint);
int host_ui_set_view_empty  (const char* text);

int host_ui_pop                (void);
int host_ui_pop_to_plugin      (void);
int host_ui_repaint            (void);
int host_ui_consume_input_text (char* out, size_t out_size);
int host_ui_consume_input_int  (int32_t* out);
int host_ui_acquire_exclusive  (void);
int host_ui_release_exclusive  (void);
int host_ui_set_inactivity     (uint32_t timeout_ms, uint32_t action_id);

/*
 * Blink the badge backlight as a visual "look at me" signal. Count is clamped
 * to 1..10, period_ms (each off- and on-phase) to 50..1000. Use 0 for either
 * argument to take the default (2 cycles, 150 ms). Blocks the calling task
 * for `2 * count * period_ms` milliseconds; the underlying LEDC PWM is
 * thread-safe so no framebuffer ordering is involved.
 */
int host_ui_wink               (uint8_t count, uint16_t period_ms);

/* ------------------------------------------------------------------------- */
/* UI - Canvas view (plugin-drawn custom UIs with inline widgets)            */
/* ------------------------------------------------------------------------- */

/* Canvas widget event subtypes used as the user_data on the widget callback. */
#define CANVAS_WIDGET_CHANGED   1
#define CANVAS_WIDGET_COMMITTED 2
#define CANVAS_WIDGET_CANCELLED 3

int host_view_canvas_push          (const char* title, uint32_t key_action_id,
                                    uint32_t widget_action_id);
int host_view_canvas_get_body_size (uint16_t* w, uint16_t* h);
int host_view_canvas_set_footer    (const char* hint);
int host_view_canvas_clear         (void);
int host_view_canvas_set_text_size (uint8_t size);
int host_view_canvas_set_text_color(bool inverted);
int host_view_canvas_draw_text     (int16_t x, int16_t y, const char* text);
int host_view_canvas_draw_text_aligned(int16_t x, int16_t y, int16_t w,
                                       const char* text, uint8_t align);
int host_view_canvas_draw_rect     (int16_t x, int16_t y, int16_t w, int16_t h, bool filled);
int host_view_canvas_invert_rect   (int16_t x, int16_t y, int16_t w, int16_t h);
int host_view_canvas_hline         (int16_t x, int16_t y, int16_t w);
int host_view_canvas_vline         (int16_t x, int16_t y, int16_t h);
int host_view_canvas_commit        (bool full_refresh);

int host_view_canvas_add_slider    (uint32_t widget_id, int32_t min, int32_t max,
                                    int32_t initial, int32_t step);
int host_view_canvas_add_text      (uint32_t widget_id, uint16_t max_len, const char* initial);
int host_view_canvas_add_button    (uint32_t widget_id);
int host_view_canvas_remove_widget (uint32_t widget_id);

int host_view_canvas_set_value     (uint32_t widget_id, int32_t value);
int host_view_canvas_get_value     (uint32_t widget_id, int32_t* out);
int host_view_canvas_set_text      (uint32_t widget_id, const char* text);
int host_view_canvas_get_text      (uint32_t widget_id, char* out, size_t cap);

int host_view_canvas_set_focus     (uint32_t widget_id);
int host_view_canvas_get_focus     (uint32_t* out);
int host_view_canvas_set_key_repeat(uint16_t initial_ms, uint16_t repeat_ms);

/* ------------------------------------------------------------------------- */
/* UI - Low-Level GFX (opt-in via capability "display_lowlevel")             */
/* ------------------------------------------------------------------------- */

uint16_t host_display_width    (void);
uint16_t host_display_height   (void);
int      host_display_clear    (void);
int      host_display_draw_pixel(int16_t x, int16_t y, uint16_t color);
int      host_display_draw_line (int16_t x0, int16_t y0, int16_t x1, int16_t y1,
                                 uint16_t color);
int      host_display_draw_rect (int16_t x, int16_t y, int16_t w, int16_t h, uint16_t color);
int      host_display_fill_rect (int16_t x, int16_t y, int16_t w, int16_t h, uint16_t color);
int      host_display_draw_text (int16_t x, int16_t y, const char* text, uint8_t size,
                                 uint16_t color);
int      host_display_flush     (uint8_t refresh_mode);
bool     host_display_is_busy   (void);

/* ------------------------------------------------------------------------- */
/* I18n                                                                      */
/* ------------------------------------------------------------------------- */

#define HOST_LANG_EN 0
#define HOST_LANG_DE 1

int      host_i18n_tr_key          (const char* key,   char* out, uint32_t out_cap);
int      host_i18n_tr_meta         (const char* field, char* out, uint32_t out_cap);
int      host_i18n_tr_core         (const char* key,   char* out, uint32_t out_cap);
uint8_t  host_i18n_current_language(void);

/* ------------------------------------------------------------------------- */
/* EventBus                                                                  */
/* ------------------------------------------------------------------------- */

#define EVENT_KEY_PRESSED       (1u <<  0)
#define EVENT_KEY_RELEASED      (1u <<  1)
#define EVENT_KEY_LONG_PRESS    (1u <<  2)
#define EVENT_POWER_USB_CONN    (1u <<  3)
#define EVENT_POWER_USB_DISCONN (1u <<  4)
#define EVENT_POWER_CHARGING    (1u <<  5)
#define EVENT_POWER_BATT_LOW    (1u <<  6)
#define EVENT_POWER_BATT_CRIT   (1u <<  7)
#define EVENT_SYSTEM_UNLOCK     (1u <<  8)
#define EVENT_SYSTEM_LOCK       (1u <<  9)
#define EVENT_SYSTEM_SLEEP      (1u << 10)
#define EVENT_SYSTEM_WAKE       (1u << 11)
#define EVENT_BLE_CONNECTED     (1u << 12)
#define EVENT_BLE_DISCONNECTED  (1u << 13)
#define EVENT_TIMER_TICK        (1u << 14)
#define EVENT_LANGUAGE_CHANGED  (1u << 15)
#define EVENT_MODULE_EVENT      (1u << 16)

int host_event_subscribe  (uint32_t event_mask, uint32_t action_id);
int host_event_unsubscribe(uint32_t subscription_id);
int host_event_publish    (uint32_t module_event_subtype, uint32_t value);

/* ------------------------------------------------------------------------- */
/* Keypad                                                                    */
/* ------------------------------------------------------------------------- */

#define KEY_0 0
#define KEY_1 1
#define KEY_2 2
#define KEY_3 3
#define KEY_4 4
#define KEY_5 5
#define KEY_6 6
#define KEY_7 7
#define KEY_8 8
#define KEY_9 9
#define KEY_Y 10
#define KEY_N 11

bool host_key_pressed     (uint8_t key);
int  host_key_consume_next(uint8_t* out_key);

/* ------------------------------------------------------------------------- */
/* USB                                                                       */
/* ------------------------------------------------------------------------- */

int host_usb_cdc_write(const uint8_t* data, size_t len);

/* ------------------------------------------------------------------------- */
/* System Info                                                               */
/* ------------------------------------------------------------------------- */

bool host_feature_enabled       (uint16_t feature_id);
int  host_get_firmware_version  (char* out, size_t out_size);
int  host_get_build_profile     (char* out, size_t out_size);

/* ------------------------------------------------------------------------- */
/* Hardware: GPIO / PWM / ADC / I2C / SAO                                    */
/* ------------------------------------------------------------------------- */

#define GPIO_DIR_IN     0
#define GPIO_DIR_OUT    1
#define GPIO_DIR_OUT_OD 2

#define GPIO_PULL_NONE  0
#define GPIO_PULL_UP    1
#define GPIO_PULL_DOWN  2

int host_gpio_set_direction(uint8_t pin, uint8_t direction);
int host_gpio_set_pull     (uint8_t pin, uint8_t pull);
int host_gpio_write        (uint8_t pin, bool level);
int host_gpio_read         (uint8_t pin, bool* level);
int host_gpio_release      (uint8_t pin);

int host_gpio_pwm_start    (uint8_t pin, uint32_t freq_hz, uint16_t duty_per_mille);
int host_gpio_pwm_set_duty (uint8_t pin, uint16_t duty_per_mille);
int host_gpio_pwm_stop     (uint8_t pin);

int host_adc_read          (uint8_t pin, uint16_t* raw, uint16_t* millivolt);

int host_i2c_write         (uint8_t bus, uint8_t addr, const uint8_t* data, size_t len);
int host_i2c_read          (uint8_t bus, uint8_t addr, uint8_t* data, size_t len);
int host_i2c_write_read    (uint8_t bus, uint8_t addr,
                            const uint8_t* wr, size_t wr_len,
                            uint8_t* rd, size_t rd_len);
int host_i2c_scan          (uint8_t bus, uint8_t* found_addrs, size_t* count);

int host_sao_eeprom_read   (uint16_t offset, uint8_t* buf, size_t len);
int host_sao_eeprom_write  (uint16_t offset, const uint8_t* buf, size_t len);

/* ------------------------------------------------------------------------- */
/* Addressable pixel strip (WS2811/WS2812/WS2813/SK6812 ...)                 */
/* ------------------------------------------------------------------------- */
/*
 * Generic RMT-driven pixel strip API. The host owns one global strip handle
 * keyed to the (gpio_pin, num_pixels, format) tuple given to the first
 * successful init. Re-init with the same tuple is a no-op; with a different
 * tuple the previous handle is replaced.
 *
 * Requires manifest capability "pixel_strip".
 */

#define PIXEL_FORMAT_GRB  0  /* WS2812/WS2813/SK6812 */
#define PIXEL_FORMAT_RGB  1
#define PIXEL_FORMAT_GRBW 2  /* SK6812 RGBW (white byte = 0 for plugin-side use) */
#define PIXEL_FORMAT_RGBW 3

int      host_pixel_strip_init    (uint8_t gpio_pin, uint16_t num_pixels, uint8_t format);
int      host_pixel_strip_deinit  (void);
int      host_pixel_strip_set     (uint16_t index, uint8_t r, uint8_t g, uint8_t b);
int      host_pixel_strip_fill    (uint8_t r, uint8_t g, uint8_t b);
int      host_pixel_strip_clear   (void);
int      host_pixel_strip_refresh (void);
uint16_t host_pixel_strip_length  (void);
bool     host_pixel_strip_ready   (void);

/* ------------------------------------------------------------------------- */
/* Lockscreen quick-action slot for background plugins                       */
/* ------------------------------------------------------------------------- */
/*
 * A plugin may register exactly one quick-action item that appears in the
 * lockscreen context menu (opened by KEY_BACK). When the user selects it,
 * the plugin's `plugin_on_action(action_id, 0, 0)` fires. The label is an
 * i18n key resolved per-language via `host_i18n_tr_key`.
 */

int host_lockscreen_register_action  (const char* label_key, uint32_t action_id);
int host_lockscreen_unregister_action(void);

#ifdef __cplusplus
}
#endif

#endif /* CDC_BADGE_HOST_API_H */

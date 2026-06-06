# gpio_mqtt

Example plugin that polls the Grove and SAO GPIO lines as digital inputs and
publishes the initial state plus changes to an MQTT broker.

Settings are opened with `[3]` from the canvas view and stored in plugin NVS:

- MQTT server URL, for example `mqtt://broker.local:1883`
- optional username
- optional password

This example uses MQTT 3.1.1 QoS 0 over the generic TCP socket host API. TLS
(`mqtts://`) and subscriptions are intentionally out of scope for this first
version.

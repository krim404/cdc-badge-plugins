//! \file
//! \brief GPIO / PWM / ADC access via the badge's expansion ports.
//!
//! Only pins exposed on the RPi-style header, the SAO port, or the Grove
//! port can be used. The plugin manifest declares each pin it wants in
//! `capabilities.gpio_pins` / `pwm_pins` / `adc_pins`, or the convenience
//! shortcuts `grove` / `sao`. Host calls for undeclared pins return
//! `HOST_ERR_NO_CAPABILITY`.

use crate::{check, Result};

/// \brief GPIO direction modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Input,
    Output,
    OutputOpenDrain,
}

impl Direction {
    fn as_u8(self) -> u8 {
        match self {
            Direction::Input => 0,
            Direction::Output => 1,
            Direction::OutputOpenDrain => 2,
        }
    }
}

/// \brief Internal pull resistor configuration for input pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pull {
    None,
    Up,
    Down,
}

impl Pull {
    fn as_u8(self) -> u8 {
        match self {
            Pull::None => 0,
            Pull::Up => 1,
            Pull::Down => 2,
        }
    }
}

/// \brief Configure a pin's direction (input / push-pull / open-drain).
/// \param pin Pin number from the manifest's `gpio_pins`.
/// \param dir Desired direction.
/// \return `Ok(())` on success, `Err` on denial / hw failure.
pub fn set_direction(pin: u8, dir: Direction) -> Result<()> {
    check(unsafe { ffi_gpio::host_gpio_set_direction(pin, dir.as_u8()) })
}

/// \brief Configure the internal pull resistor on a pin.
/// \param pin  Pin number from the manifest's `gpio_pins`.
/// \param pull Pull mode to apply.
/// \return `Ok(())` on success, `Err` on failure.
pub fn set_pull(pin: u8, pull: Pull) -> Result<()> {
    check(unsafe { ffi_gpio::host_gpio_set_pull(pin, pull.as_u8()) })
}

/// \brief Drive an output pin high or low.
/// \param pin   Pin number from the manifest's `gpio_pins`.
/// \param level `true` = high, `false` = low.
/// \return `Ok(())` on success, `Err` on failure.
pub fn write(pin: u8, level: bool) -> Result<()> {
    check(unsafe { ffi_gpio::host_gpio_write(pin, level) })
}

/// \brief Sample an input pin.
/// \param pin Pin number from the manifest's `gpio_pins`.
/// \return `Ok(true)` if the pin reads high, `Ok(false)` if low,
///         `Err` on failure.
pub fn read(pin: u8) -> Result<bool> {
    let mut level = false;
    check(unsafe { ffi_gpio::host_gpio_read(pin, &mut level) })?;
    Ok(level)
}

/// \brief Release the pin so other capabilities (e.g. SAO, Grove) can
///        claim it again.
/// \param pin Pin number to release.
pub fn release(pin: u8) {
    unsafe {
        ffi_gpio::host_gpio_release(pin);
    }
}

/// \brief Start a PWM signal on a pin.
/// \param pin            Pin number from the manifest's `pwm_pins`.
/// \param freq_hz        Output frequency in Hz.
/// \param duty_per_mille Duty cycle in tenths of a percent (`0..=1000`).
/// \return `Ok(())` on success, `Err` on failure.
pub fn pwm_start(pin: u8, freq_hz: u32, duty_per_mille: u16) -> Result<()> {
    check(unsafe { ffi_gpio::host_gpio_pwm_start(pin, freq_hz, duty_per_mille) })
}

/// \brief Adjust the duty cycle of an active PWM channel.
/// \param pin            Pin number with an active PWM channel.
/// \param duty_per_mille New duty cycle (`0..=1000`).
/// \return `Ok(())` on success, `Err` on failure.
pub fn pwm_set_duty(pin: u8, duty_per_mille: u16) -> Result<()> {
    check(unsafe { ffi_gpio::host_gpio_pwm_set_duty(pin, duty_per_mille) })
}

/// \brief Stop PWM output and free the channel.
/// \param pin Pin number to stop.
pub fn pwm_stop(pin: u8) {
    unsafe {
        ffi_gpio::host_gpio_pwm_stop(pin);
    }
}

/// \brief Single ADC sample, returned both as the raw ADC reading and as
///        a calibrated millivolt value.
#[derive(Debug, Clone, Copy)]
pub struct AdcReading {
    pub raw: u16,
    pub millivolt: u16,
}

/// \brief Sample an ADC pin.
/// \param pin Pin number from the manifest's `adc_pins`.
/// \return The reading on success, `Err` on failure.
pub fn adc_read(pin: u8) -> Result<AdcReading> {
    let mut raw: u16 = 0;
    let mut mv: u16 = 0;
    check(unsafe { ffi_gpio::host_adc_read(pin, &mut raw, &mut mv) })?;
    Ok(AdcReading { raw, millivolt: mv })
}

mod ffi_gpio {
    use core::ffi::c_int;

    #[link(wasm_import_module = "cdc")]
    extern "C" {
        pub fn host_gpio_set_direction(pin: u8, direction: u8) -> c_int;
        pub fn host_gpio_set_pull(pin: u8, pull: u8) -> c_int;
        pub fn host_gpio_write(pin: u8, level: bool) -> c_int;
        pub fn host_gpio_read(pin: u8, level: *mut bool) -> c_int;
        pub fn host_gpio_release(pin: u8) -> c_int;

        pub fn host_gpio_pwm_start(pin: u8, freq_hz: u32, duty_per_mille: u16) -> c_int;
        pub fn host_gpio_pwm_set_duty(pin: u8, duty_per_mille: u16) -> c_int;
        pub fn host_gpio_pwm_stop(pin: u8) -> c_int;

        pub fn host_adc_read(pin: u8, raw: *mut u16, millivolt: *mut u16) -> c_int;
    }
}

/// \brief Convenience pin numbers for the on-badge expansion ports.
pub mod pins {
    /// \brief Grove SIG0 pin.
    pub const GROVE_0: u8 = 2;
    /// \brief Grove SIG1 pin.
    pub const GROVE_1: u8 = 3;
    /// \brief SAO GPIO1 pin.
    pub const SAO_GPIO1: u8 = 15;
    /// \brief SAO GPIO2 pin.
    pub const SAO_GPIO2: u8 = 16;
}

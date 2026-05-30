//! \file
//! \brief Battery and power state read-out.

use crate::ffi;

/// \brief Current battery voltage.
/// \return Voltage in millivolts.
pub fn battery_mv() -> u16 {
    unsafe { ffi::host_battery_mv() }
}

/// \brief Current battery state of charge.
/// \return Percentage in the inclusive range `0..=100`.
pub fn battery_pct() -> u8 {
    unsafe { ffi::host_battery_pct() }
}

/// \brief Whether the badge currently sees USB Vbus.
/// \return `true` if a USB host is connected.
pub fn usb_connected() -> bool {
    unsafe { ffi::host_is_usb_connected() }
}

/// \brief Active power source as reported by the firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    Battery,
    Usb,
    Unknown,
}

/// \brief Read the active power source.
/// \return The source as a [`PowerSource`] variant.
pub fn power_source() -> PowerSource {
    match unsafe { ffi::host_power_source() } {
        1 => PowerSource::Battery,
        2 => PowerSource::Usb,
        _ => PowerSource::Unknown,
    }
}

/// \brief Whether the battery is below the warn threshold.
/// \return `true` when the firmware flags the battery as low.
pub fn battery_low() -> bool {
    unsafe { ffi::host_is_battery_low() }
}

/// \brief Whether the battery is below the critical threshold.
/// \return `true` when the firmware flags the battery as critical.
pub fn battery_critical() -> bool {
    unsafe { ffi::host_is_battery_critical() }
}

/// \brief Charge-controller state as reported by the firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeStatus {
    NotCharging,
    PreCharge,
    Fast,
    Done,
    Fault,
    Unknown,
}

/// \brief Read the current charge-controller state.
/// \return The state as a [`ChargeStatus`] variant.
pub fn charge_status() -> ChargeStatus {
    match unsafe { ffi::host_charge_status() } {
        0 => ChargeStatus::NotCharging,
        1 => ChargeStatus::PreCharge,
        2 => ChargeStatus::Fast,
        3 => ChargeStatus::Done,
        4 => ChargeStatus::Fault,
        _ => ChargeStatus::Unknown,
    }
}

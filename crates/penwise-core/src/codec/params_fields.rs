//! Field-group readers/writers behind `stroke::{encode_params, decode_params}` (private to the codec).
//!
//! Owner: slice d.

use super::error::CodecError;
use super::varint::Reader;
use crate::brush::{
    CapStyle, GrainParams, Jitter, NibParams, OutlineParams, SprayDistribution, SprayParams,
    StampParams,
};
use crate::dynamics::{Dynamics, PressureCurve, TiltResponse, VelocityResponse};
use crate::float::canon;
use crate::smoothing::OneEuroParams;

/// Appends each value as canonical LE `f32` bits (non-finite → `+0.0`).
pub(super) fn write_f32s(out: &mut Vec<u8>, values: &[f32]) {
    for &v in values {
        let v = canon(v).unwrap_or(0.0);
        out.extend_from_slice(&v.to_bits().to_le_bytes());
    }
}

pub(super) fn invalid(field: &'static str) -> CodecError {
    CodecError::InvalidValue { field }
}

pub(super) fn read_dynamics(r: &mut Reader<'_>) -> Result<Dynamics, CodecError> {
    Ok(Dynamics {
        pressure: PressureCurve {
            floor: r.f32_le("pressure.floor")?,
            saturation: r.f32_le("pressure.saturation")?,
            gamma: r.f32_le("pressure.gamma")?,
        },
        velocity: VelocityResponse {
            v0: r.f32_le("velocity.v0")?,
            v1: r.f32_le("velocity.v1")?,
            size: r.f32_le("velocity.size")?,
            opacity: r.f32_le("velocity.opacity")?,
        },
        tilt: TiltResponse {
            theta0: r.f32_le("tilt.theta0")?,
            theta1: r.f32_le("tilt.theta1")?,
            max_aspect: r.f32_le("tilt.max_aspect")?,
            max_radius_scale: r.f32_le("tilt.max_radius_scale")?,
            side_hardness: r.f32_le("tilt.side_hardness")?,
            side_alpha: r.f32_le("tilt.side_alpha")?,
        },
    })
}

pub(super) fn read_one_euro(r: &mut Reader<'_>) -> Result<OneEuroParams, CodecError> {
    Ok(OneEuroParams {
        min_cutoff_hz: r.f32_le("smoothing.min_cutoff_hz")?,
        beta: r.f32_le("smoothing.beta")?,
        d_cutoff_hz: r.f32_le("smoothing.d_cutoff_hz")?,
    })
}

pub(super) fn read_outline(r: &mut Reader<'_>) -> Result<OutlineParams, CodecError> {
    let smoothing = r.f32_le("outline.smoothing")?;
    let taper_start = r.f32_le("outline.taper_start")?;
    let taper_end = r.f32_le("outline.taper_end")?;
    let cap_start = CapStyle::from_code(r.u8()?).ok_or(invalid("outline.cap_start"))?;
    let cap_end = CapStyle::from_code(r.u8()?).ok_or(invalid("outline.cap_end"))?;
    let nib = NibParams {
        angle: r.f32_le("nib.angle")?,
        aspect: r.f32_le("nib.aspect")?,
        thickness: r.f32_le("nib.thickness")?,
        fixation: r.f32_le("nib.fixation")?,
    };
    Ok(OutlineParams {
        smoothing,
        taper_start,
        taper_end,
        cap_start,
        cap_end,
        nib,
    })
}

pub(super) fn read_stamp(r: &mut Reader<'_>) -> Result<StampParams, CodecError> {
    let flow = r.f32_le("stamp.flow")?;
    let flow_pressure = r.f32_le("stamp.flow_pressure")?;
    let hardness = r.f32_le("stamp.hardness")?;
    let spacing = r.f32_le("stamp.spacing")?;
    let scatter = r.f32_le("stamp.scatter")?;
    let jitter = Jitter {
        size: r.f32_le("jitter.size")?,
        opacity: r.f32_le("jitter.opacity")?,
        angle: r.f32_le("jitter.angle")?,
    };
    let dabs_per_second = r.f32_le("stamp.dabs_per_second")?;
    let grain = if read_flag(r, "grain flag")? {
        Some(GrainParams {
            scale: r.f32_le("grain.scale")?,
            strength: r.f32_le("grain.strength")?,
            softness: r.f32_le("grain.softness")?,
        })
    } else {
        None
    };
    let spray = if read_flag(r, "spray flag")? {
        Some(SprayParams {
            particles_max: r.uv32()?,
            particle_size_min: r.f32_le("spray.particle_size_min")?,
            particle_size_max: r.f32_le("spray.particle_size_max")?,
            distribution: SprayDistribution::from_code(r.u8()?)
                .ok_or(invalid("spray.distribution"))?,
        })
    } else {
        None
    };
    Ok(StampParams {
        flow,
        flow_pressure,
        hardness,
        spacing,
        scatter,
        jitter,
        dabs_per_second,
        grain,
        spray,
    })
}

fn read_flag(r: &mut Reader<'_>, field: &'static str) -> Result<bool, CodecError> {
    match r.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid(field)),
    }
}
